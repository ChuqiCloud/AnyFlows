use std::sync::{
    Arc,
    atomic::{AtomicI64, AtomicUsize, Ordering},
};

use af_admin::{
    IssuedApiKey, IssuedUserToken, LoginCredentials, SessionAuthentication,
    SessionAuthenticationError, SessionAuthenticationFuture, SessionAuthenticator,
    SessionLoginFuture, SessionPrincipal, SessionRole, UserToken, UserTokenCreateFuture,
    UserTokenDeleteFuture, UserTokenError, UserTokenGetFuture, UserTokenListFuture,
    UserTokenListQuery, UserTokenPage, UserTokenService, UserTokenStatus, UserTokenUpdateFuture,
    UserTokenWriteCommand,
};
use af_domain::{GroupId, TokenId, UserId};
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use http::{
    StatusCode,
    header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::user_tokens::build_user_token_router;

const USER_SESSION: &str = "user-session";
const ADMIN_SESSION: &str = "admin-session";
const LIMIT_SESSION: &str = "limit-session";

struct FakeSessionAuthenticator;

impl SessionAuthenticator for FakeSessionAuthenticator {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        let principal = match token {
            USER_SESSION => Some(SessionPrincipal::new(
                UserId::new(2).unwrap(),
                SessionRole::User,
            )),
            ADMIN_SESSION => Some(SessionPrincipal::new(
                UserId::new(1).unwrap(),
                SessionRole::Admin,
            )),
            LIMIT_SESSION => Some(SessionPrincipal::new(
                UserId::new(3).unwrap(),
                SessionRole::User,
            )),
            _ => None,
        };
        Box::pin(async move {
            principal
                .map(|principal| {
                    SessionAuthentication::new(
                        principal,
                        GroupId::new(1).unwrap(),
                        current_timestamp() + 60,
                    )
                })
                .ok_or(SessionAuthenticationError::InvalidSession)
        })
    }
}

struct FakeUserTokenService {
    calls: AtomicUsize,
    last_user_id: AtomicI64,
}

impl FakeUserTokenService {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            last_user_id: AtomicI64::new(0),
        }
    }

    fn record(&self, principal: SessionPrincipal) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.last_user_id
            .store(principal.user_id().get(), Ordering::Relaxed);
    }
}

impl UserTokenService for FakeUserTokenService {
    fn list(
        &self,
        principal: SessionPrincipal,
        query: UserTokenListQuery,
    ) -> UserTokenListFuture<'_> {
        self.record(principal);
        Box::pin(async move {
            if query.after().map(TokenId::get) == Some(99) {
                return Err(UserTokenError::Internal);
            }
            Ok(UserTokenPage::from_parts(
                vec![sample_token(10, "personal")],
                None,
            ))
        })
    }

    fn get(&self, principal: SessionPrincipal, token_id: TokenId) -> UserTokenGetFuture<'_> {
        self.record(principal);
        Box::pin(async move {
            match token_id.get() {
                404 => Err(UserTokenError::NotFound),
                500 => Err(UserTokenError::Internal),
                id => Ok(sample_token(id, "detail")),
            }
        })
    }

    fn create(
        &self,
        principal: SessionPrincipal,
        _command: UserTokenWriteCommand,
    ) -> UserTokenCreateFuture<'_> {
        self.record(principal);
        Box::pin(async move {
            if principal.user_id().get() == 3 {
                return Err(UserTokenError::LimitReached);
            }
            let api_key = IssuedApiKey::generate().map_err(|_| UserTokenError::Internal)?;
            Ok(IssuedUserToken::from_parts(
                sample_token(20, "created"),
                api_key,
            ))
        })
    }

    fn update(
        &self,
        principal: SessionPrincipal,
        token_id: TokenId,
        _command: UserTokenWriteCommand,
    ) -> UserTokenUpdateFuture<'_> {
        self.record(principal);
        Box::pin(async move {
            if token_id.get() == 404 {
                Err(UserTokenError::NotFound)
            } else {
                Ok(sample_token(token_id.get(), "updated"))
            }
        })
    }

    fn delete(&self, principal: SessionPrincipal, token_id: TokenId) -> UserTokenDeleteFuture<'_> {
        self.record(principal);
        Box::pin(async move {
            if token_id.get() == 404 {
                Err(UserTokenError::NotFound)
            } else {
                Ok(())
            }
        })
    }
}

#[tokio::test]
async fn authenticated_user_can_manage_only_principal_scoped_keys() {
    let service = Arc::new(FakeUserTokenService::new());
    let app = router(service.clone());

    let listed = app
        .clone()
        .oneshot(request("GET", "/api/tokens?limit=50", USER_SESSION, None))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(listed.headers()[CACHE_CONTROL], "no-store");
    let list_body = response_json(listed).await;
    assert_eq!(list_body["capacity"], 32);
    assert_eq!(list_body["tokens"][0]["id"], 10);
    let rendered = list_body.to_string();
    for hidden in [
        "user_id",
        "group_id",
        "cross_group_retry",
        "rate_limit_5h",
        "max_requests",
        "key_hash",
    ] {
        assert!(!rendered.contains(hidden), "响应不得包含 {hidden}");
    }
    assert_eq!(service.last_user_id.load(Ordering::Relaxed), 2);

    let created = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/tokens",
            USER_SESSION,
            Some(valid_body()),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_body = response_json(created).await;
    assert_eq!(created_body["token"]["name"], "created");
    assert_eq!(created_body["api_key"].as_str().unwrap().len(), 49);

    let updated = app
        .clone()
        .oneshot(request(
            "PUT",
            "/api/tokens/10",
            USER_SESSION,
            Some(valid_body()),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(response_json(updated).await["name"], "updated");

    let deleted = app
        .clone()
        .oneshot(request("DELETE", "/api/tokens/10", USER_SESSION, None))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(deleted.headers()[CACHE_CONTROL], "no-store");

    let admin = app
        .oneshot(request("GET", "/api/tokens/10", ADMIN_SESSION, None))
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);
    assert_eq!(service.last_user_id.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn unknown_owner_fields_and_invalid_sessions_are_rejected_before_service() {
    let service = Arc::new(FakeUserTokenService::new());
    let app = router(service.clone());
    let body = json!({
        "user_id": 999,
        "name": "personal",
        "status": "enabled",
        "remain_quota": 100,
        "unlimited_quota": false,
        "expired_at": null,
        "model_limits": null,
        "allow_ips": null
    });
    let unknown_owner = app
        .clone()
        .oneshot(request("POST", "/api/tokens", USER_SESSION, Some(body)))
        .await
        .unwrap();
    assert_error(unknown_owner, StatusCode::BAD_REQUEST, "invalid_request").await;

    let reserved_name = json!({
        "name": af_domain::PLAYGROUND_TOKEN_NAME,
        "status": "enabled",
        "remain_quota": 0,
        "unlimited_quota": true,
        "expired_at": null,
        "model_limits": null,
        "allow_ips": null
    });
    let reserved_name = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/tokens",
            USER_SESSION,
            Some(reserved_name),
        ))
        .await
        .unwrap();
    assert_error(reserved_name, StatusCode::BAD_REQUEST, "invalid_request").await;

    let invalid_query = app
        .clone()
        .oneshot(request(
            "GET",
            "/api/tokens?limit=1&limit=2",
            USER_SESSION,
            None,
        ))
        .await
        .unwrap();
    assert_error(invalid_query, StatusCode::BAD_REQUEST, "invalid_request").await;

    let no_session = app
        .oneshot(
            Request::builder()
                .uri("/api/tokens")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_error(no_session, StatusCode::UNAUTHORIZED, "invalid_session").await;
    assert_eq!(service.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn capacity_and_owner_misses_use_stable_error_codes() {
    let service = Arc::new(FakeUserTokenService::new());
    let app = router(service);
    let limit = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/tokens",
            LIMIT_SESSION,
            Some(valid_body()),
        ))
        .await
        .unwrap();
    assert_error(limit, StatusCode::CONFLICT, "token_limit_reached").await;

    for (method, uri, body) in [
        ("GET", "/api/tokens/404", None),
        ("PUT", "/api/tokens/404", Some(valid_body())),
        ("DELETE", "/api/tokens/404", None),
    ] {
        let response = app
            .clone()
            .oneshot(request(method, uri, USER_SESSION, body))
            .await
            .unwrap();
        assert_error(response, StatusCode::NOT_FOUND, "token_not_found").await;
    }
}

fn router(service: Arc<FakeUserTokenService>) -> axum::Router {
    let service: Arc<dyn UserTokenService> = service;
    let authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    build_user_token_router(service, authenticator)
}

fn sample_token(id: i64, name: &str) -> UserToken {
    UserToken::from_parts(
        TokenId::new(id).unwrap(),
        "sk-af-public000001".to_owned(),
        name.to_owned(),
        UserTokenStatus::Enabled,
        100,
        false,
        5,
        None,
        Some(vec!["gpt-5".to_owned()]),
        Some(vec!["127.0.0.1".to_owned()]),
        1_800_000_000,
        1_800_000_100,
    )
}

fn valid_body() -> Value {
    json!({
        "name": "personal",
        "status": "enabled",
        "remain_quota": 100,
        "unlimited_quota": false,
        "expired_at": null,
        "model_limits": ["gpt-5"],
        "allow_ips": ["127.0.0.1"]
    })
}

fn request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    let body = if let Some(body) = body {
        builder = builder.header(CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    } else {
        Body::empty()
    };
    builder.body(body).unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 256 * 1024).await.unwrap()).unwrap()
}

async fn assert_error(response: axum::response::Response, status: StatusCode, code: &str) {
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(response_json(response).await["code"], code);
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
