use std::{
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
    },
};

use af_admin::{
    AdminEmailSettings, AdminEmailSettingsCommand, AdminEmailSettingsError,
    AdminEmailSettingsReadFuture, AdminEmailSettingsService, AdminEmailSettingsUpdateFuture,
    AdminEmailTestCommand, AdminEmailTestFuture, AdminEmailTlsMode, LoginCredentials,
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

use crate::email_settings::build_email_settings_router;

const ADMIN_TOKEN: &str = "email-admin-token";
const USER_TOKEN: &str = "email-user-token";
const SMTP_PASSWORD: &str = "private smtp password";

struct FakeEmailSettingsService {
    configured: AtomicBool,
    version: AtomicI64,
    result: Mutex<Result<(), AdminEmailSettingsError>>,
    test_calls: AtomicUsize,
}

impl FakeEmailSettingsService {
    fn new() -> Self {
        Self {
            configured: AtomicBool::new(false),
            version: AtomicI64::new(1),
            result: Mutex::new(Ok(())),
            test_calls: AtomicUsize::new(0),
        }
    }

    fn fail(&self) {
        *self.result.lock().unwrap() = Err(AdminEmailSettingsError::DeliveryFailed);
    }

    fn test_calls(&self) -> usize {
        self.test_calls.load(Ordering::SeqCst)
    }

    fn snapshot(&self) -> AdminEmailSettings {
        let version = self.version.load(Ordering::SeqCst);
        if self.configured.load(Ordering::SeqCst) {
            configured_settings(version)
        } else {
            disabled_settings(version)
        }
    }
}

impl AdminEmailSettingsService for FakeEmailSettingsService {
    fn settings<'a>(&'a self, _principal: SessionPrincipal) -> AdminEmailSettingsReadFuture<'a> {
        let settings = self.snapshot();
        Box::pin(async move { Ok(settings) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminEmailSettingsCommand,
    ) -> AdminEmailSettingsUpdateFuture<'a> {
        self.configured.store(true, Ordering::SeqCst);
        let version = self.version.fetch_add(1, Ordering::SeqCst) + 1;
        Box::pin(async move { Ok(configured_settings(version)) })
    }

    fn send_test<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminEmailTestCommand,
    ) -> AdminEmailTestFuture<'a> {
        if !self.configured.load(Ordering::SeqCst) {
            return Box::pin(async { Err(AdminEmailSettingsError::NotConfigured) });
        }
        self.test_calls.fetch_add(1, Ordering::SeqCst);
        let result = *self.result.lock().unwrap();
        Box::pin(async move { result })
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

fn fixture() -> (Arc<FakeEmailSettingsService>, axum::Router) {
    let service = Arc::new(FakeEmailSettingsService::new());
    let router = build_email_settings_router(service.clone(), Arc::new(FakeSessionAuthenticator));
    (service, router)
}

fn disabled_settings(version: i64) -> AdminEmailSettings {
    AdminEmailSettings::new(
        false,
        String::new(),
        587,
        AdminEmailTlsMode::StartTls,
        None,
        false,
        String::new(),
        None,
        None,
        15,
        version,
    )
    .unwrap()
}

fn configured_settings(version: i64) -> AdminEmailSettings {
    AdminEmailSettings::new(
        true,
        "smtp.example.com".to_owned(),
        587,
        AdminEmailTlsMode::StartTls,
        Some("mailer@example.com".to_owned()),
        true,
        "from@example.com".to_owned(),
        Some("AnyFlows".to_owned()),
        Some("reply@example.com".to_owned()),
        15,
        version,
    )
    .unwrap()
}

#[tokio::test]
async fn admin_can_update_read_and_test_email_without_secret_echo() -> Result<(), Box<dyn Error>> {
    let (service, app) = fixture();

    let initial = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/email-settings",
            Value::Null,
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(initial.status(), StatusCode::OK);
    assert_eq!(initial.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(response_json(initial).await?["enabled"], false);

    let updated = app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/admin/email-settings",
            settings_body(Some(SMTP_PASSWORD)),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(updated.headers()[CACHE_CONTROL], "no-store");
    let updated_body = response_json(updated).await?;
    assert_eq!(updated_body["enabled"], true);
    assert_eq!(updated_body["password_configured"], true);
    assert_eq!(updated_body["delivery_ready"], true);
    assert_eq!(updated_body["version"], 2);
    assert!(updated_body.get("password").is_none());
    assert!(!updated_body.to_string().contains(SMTP_PASSWORD));

    let kept = app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/admin/email-settings",
            settings_body(None),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(response_json(kept).await?["version"], 3);

    let tested = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/email-settings/test",
            json!({"recipient": "recipient@example.com"}),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(tested.status(), StatusCode::NO_CONTENT);
    assert_eq!(tested.headers()[CACHE_CONTROL], "no-store");
    assert!(to_bytes(tested.into_body(), 1024).await?.is_empty());
    assert_eq!(service.test_calls(), 1);
    Ok(())
}

#[tokio::test]
async fn authentication_and_delivery_errors_use_stable_sanitized_codes()
-> Result<(), Box<dyn Error>> {
    let (service, app) = fixture();

    let unauthorized = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/email-settings",
            Value::Null,
            None,
        ))
        .await?;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let forbidden = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/email-settings",
            Value::Null,
            Some(USER_TOKEN),
        ))
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let not_configured = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/email-settings/test",
            json!({"recipient": "private-recipient@example.com"}),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(not_configured.status(), StatusCode::CONFLICT);
    let body = response_json(not_configured).await?;
    assert_eq!(body["code"], "email_not_configured");
    assert!(!body.to_string().contains("private-recipient"));

    let invalid = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/email-settings/test",
            json!({
                "recipient": "recipient@example.com",
                "subject": "must not be accepted"
            }),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    app.clone()
        .oneshot(json_request(
            "PUT",
            "/api/admin/email-settings",
            settings_body(Some(SMTP_PASSWORD)),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    service.fail();
    let failed = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/email-settings/test",
            json!({"recipient": "private-recipient@example.com"}),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(failed.status(), StatusCode::BAD_GATEWAY);
    let failed_body = response_json(failed).await?;
    assert_eq!(failed_body["code"], "email_delivery_failed");
    let rendered = failed_body.to_string();
    assert!(!rendered.contains("private-recipient"));
    assert!(!rendered.contains(SMTP_PASSWORD));

    Ok(())
}

fn settings_body(password: Option<&str>) -> Value {
    let mut body = json!({
        "enabled": true,
        "host": "smtp.example.com",
        "port": 587,
        "tls_mode": "start_tls",
        "username": "mailer@example.com",
        "from_address": "from@example.com",
        "from_name": "AnyFlows",
        "reply_to": "reply@example.com",
        "timeout_seconds": 15
    });
    if let Some(password) = password {
        body["password"] = Value::String(password.to_owned());
    }
    body
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
