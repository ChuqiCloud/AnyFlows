use std::{
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use af_admin::{
    AdminCustomOAuth2Provider, AdminCustomOAuth2ProviderCommand, AdminCustomOAuth2ProviderError,
    AdminCustomOAuth2ProviderGetFuture, AdminCustomOAuth2ProviderListFuture,
    AdminCustomOAuth2ProviderService, AdminCustomOAuth2ProviderUpdateFuture, LoginCredentials,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_domain::{GroupId, UserId};
use axum::body::{Body, to_bytes};
use http::{
    Request, StatusCode,
    header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::custom_oauth2::build_custom_oauth2_router;

const ADMIN_TOKEN: &str = "custom-oauth-admin-token";
const USER_TOKEN: &str = "custom-oauth-user-token";
const CLIENT_SECRET: &str = "client-secret-that-must-not-echo";

struct FakeCustomOAuth2Service {
    provider: AdminCustomOAuth2Provider,
    conflict: AtomicBool,
    not_found: AtomicBool,
    save_calls: Mutex<Vec<AdminCustomOAuth2ProviderCommand>>,
}

impl FakeCustomOAuth2Service {
    fn new() -> Self {
        Self {
            provider: AdminCustomOAuth2Provider::new(
                "custom_acme".to_owned(),
                "Acme 登录".to_owned(),
                "client-id".to_owned(),
                true,
                true,
                true,
                true,
                true,
                true,
                true,
                1,
            )
            .unwrap(),
            conflict: AtomicBool::new(false),
            not_found: AtomicBool::new(false),
            save_calls: Mutex::new(Vec::new()),
        }
    }

    fn set_conflict(&self, value: bool) {
        self.conflict.store(value, Ordering::SeqCst);
    }

    fn set_not_found(&self, value: bool) {
        self.not_found.store(value, Ordering::SeqCst);
    }
}

impl AdminCustomOAuth2ProviderService for FakeCustomOAuth2Service {
    fn list(&self, _principal: SessionPrincipal) -> AdminCustomOAuth2ProviderListFuture<'_> {
        let provider = self.provider.clone();
        Box::pin(async move { Ok(vec![provider]) })
    }

    fn get(
        &self,
        _principal: SessionPrincipal,
        _provider_key: String,
    ) -> AdminCustomOAuth2ProviderGetFuture<'_> {
        let result = if self.not_found.load(Ordering::SeqCst) {
            Err(AdminCustomOAuth2ProviderError::NotFound)
        } else {
            Ok(self.provider.clone())
        };
        Box::pin(async move { result })
    }

    fn save(
        &self,
        _principal: SessionPrincipal,
        _provider_key: String,
        command: AdminCustomOAuth2ProviderCommand,
    ) -> AdminCustomOAuth2ProviderUpdateFuture<'_> {
        if self.conflict.load(Ordering::SeqCst) {
            return Box::pin(async { Err(AdminCustomOAuth2ProviderError::Conflict) });
        }
        self.save_calls.lock().unwrap().push(command);
        let provider = self.provider.clone();
        Box::pin(async move { Ok(provider) })
    }
}

struct FakeSessionAuthenticator;

impl SessionAuthenticator for FakeSessionAuthenticator {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        let principal = match token {
            ADMIN_TOKEN => Some(SessionPrincipal::new(
                UserId::new(1).unwrap(),
                SessionRole::Admin,
            )),
            USER_TOKEN => Some(SessionPrincipal::new(
                UserId::new(2).unwrap(),
                SessionRole::User,
            )),
            _ => None,
        };
        Box::pin(async move {
            principal
                .map(|principal| {
                    SessionAuthentication::new(
                        principal,
                        GroupId::new(3).unwrap(),
                        current_timestamp() + 60,
                    )
                })
                .ok_or(SessionAuthenticationError::InvalidSession)
        })
    }
}

fn fixture() -> (Arc<FakeCustomOAuth2Service>, axum::Router) {
    let service = Arc::new(FakeCustomOAuth2Service::new());
    let router = build_custom_oauth2_router(service.clone(), Arc::new(FakeSessionAuthenticator));
    (service, router)
}

#[tokio::test]
async fn admin_can_read_and_update_without_secret_echo() -> Result<(), Box<dyn Error>> {
    let (service, app) = fixture();
    let listed = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/authentication-settings/oauth/custom",
            Value::Null,
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(listed.headers()[CACHE_CONTROL], "no-store");
    let listed_body = response_json(listed).await?;
    assert_eq!(listed_body["providers"][0]["provider_key"], "custom_acme");
    assert_eq!(listed_body["providers"][0]["secret_configured"], true);
    assert!(listed_body["providers"][0].get("client_secret").is_none());

    let updated = app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/admin/authentication-settings/oauth/custom/custom_acme",
            json!({
                "expected_version": 1,
                "display_name": "Acme 登录",
                "client_id": "client-id",
                "authorization_endpoint": "https://login.example.com/authorize",
                "token_endpoint": "https://login.example.com/token",
                "userinfo_endpoint": "https://login.example.com/userinfo",
                "scope": "openid profile",
                "subject_field": "sub",
                "enabled": true,
                "client_secret": CLIENT_SECRET,
                "clear_client_secret": false
            }),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(updated.headers()[CACHE_CONTROL], "no-store");
    let updated_body = response_json(updated).await?;
    assert!(updated_body.get("client_secret").is_none());
    assert!(!updated_body.to_string().contains(CLIENT_SECRET));
    assert_eq!(service.save_calls.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn authentication_not_found_and_cas_errors_are_stable() -> Result<(), Box<dyn Error>> {
    let (service, app) = fixture();
    let unauthorized = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/authentication-settings/oauth/custom",
            Value::Null,
            None,
        ))
        .await?;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let forbidden = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/authentication-settings/oauth/custom",
            Value::Null,
            Some(USER_TOKEN),
        ))
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    service.set_not_found(true);
    let missing = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/authentication-settings/oauth/custom/custom_missing",
            Value::Null,
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(missing).await?["code"],
        "custom_oauth2_provider_not_found"
    );

    service.set_conflict(true);
    let conflict = app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/admin/authentication-settings/oauth/custom/custom_acme",
            valid_update_body(),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(conflict).await?["code"],
        "custom_oauth2_provider_conflict"
    );
    Ok(())
}

fn valid_update_body() -> Value {
    json!({
        "expected_version": 1,
        "display_name": "Acme 登录",
        "client_id": "client-id",
        "authorization_endpoint": "https://login.example.com/authorize",
        "token_endpoint": "https://login.example.com/token",
        "userinfo_endpoint": "https://login.example.com/userinfo",
        "scope": "openid profile",
        "subject_field": "sub",
        "enabled": true,
        "client_secret": null,
        "clear_client_secret": false
    })
}

fn json_request(method: &str, uri: &str, body: Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if method != "GET" {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }
    if let Some(token) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    builder
        .body(if method == "GET" {
            Body::empty()
        } else {
            Body::from(serde_json::to_vec(&body).unwrap())
        })
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Result<Value, Box<dyn Error>> {
    let body = to_bytes(response.into_body(), 64 * 1024).await?;
    Ok(serde_json::from_slice(&body)?)
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
