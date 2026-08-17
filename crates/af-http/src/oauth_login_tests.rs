use std::sync::{Arc, Mutex};

use af_admin::{
    AdminOAuthLoginProviderSettings, AdminOAuthLoginProviderSettingsCommand, IssuedSession,
    LoginCredentials, OAuthLoginCallbackResult, OAuthLoginError, OAuthLoginService,
    OAuthLoginServiceFuture, OAuthLoginStart, PublicOAuthLoginProvider, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole, SessionToken,
};
use af_domain::UserId;
use axum::body::{Body, to_bytes};
use http::{
    Request, StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, LOCATION, SET_COOKIE},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::oauth_login::build_oauth_login_router;

const TICKET: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

struct FakeOAuthLoginService {
    calls: Mutex<Vec<&'static str>>,
}

impl FakeOAuthLoginService {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, call: &'static str) {
        self.calls.lock().unwrap().push(call);
    }
}

impl OAuthLoginService for FakeOAuthLoginService {
    fn public_providers(&self) -> OAuthLoginServiceFuture<'_, Vec<PublicOAuthLoginProvider>> {
        Box::pin(async { Ok(vec![PublicOAuthLoginProvider::new("github", "GitHub")]) })
    }

    fn begin<'a>(&'a self, _provider: &'a str) -> OAuthLoginServiceFuture<'a, OAuthLoginStart> {
        self.record("begin");
        Box::pin(async {
            Ok(OAuthLoginStart::new(
                "https://github.com/login/oauth/authorize?state=opaque&redirect_uri=https%3A%2F%2Fconsole.example%2Fapi%2Fauth%2Foauth%2Fgithub%2Fcallback"
                    .to_owned(),
            ))
        })
    }

    fn complete<'a>(
        &'a self,
        _provider: &'a str,
        _state: String,
        _code: String,
    ) -> OAuthLoginServiceFuture<'a, OAuthLoginCallbackResult> {
        self.record("complete");
        Box::pin(async {
            Ok(OAuthLoginCallbackResult::new(format!(
                "https://console.example/#/oauth/callback?ticket={TICKET}"
            )))
        })
    }

    fn cancel<'a>(
        &'a self,
        _provider: &'a str,
        _state: String,
    ) -> OAuthLoginServiceFuture<'a, OAuthLoginCallbackResult> {
        self.record("cancel");
        Box::pin(async {
            Ok(OAuthLoginCallbackResult::new(
                "https://console.example/#/oauth/callback?error=cancelled".to_owned(),
            ))
        })
    }

    fn callback_failure(&self) -> OAuthLoginServiceFuture<'_, OAuthLoginCallbackResult> {
        self.record("failure");
        Box::pin(async {
            Ok(OAuthLoginCallbackResult::new(
                "https://console.example/#/oauth/callback?error=failed".to_owned(),
            ))
        })
    }

    fn exchange_ticket(&self, ticket: String) -> OAuthLoginServiceFuture<'_, UserId> {
        self.record("exchange");
        Box::pin(async move {
            if ticket == TICKET {
                Ok(UserId::new(42).unwrap())
            } else {
                Err(OAuthLoginError::Rejected)
            }
        })
    }

    fn admin_settings(
        &self,
        _principal: SessionPrincipal,
    ) -> OAuthLoginServiceFuture<'_, AdminOAuthLoginProviderSettings> {
        Box::pin(async { Err(OAuthLoginError::Internal) })
    }

    fn update_admin_settings(
        &self,
        _principal: SessionPrincipal,
        _command: AdminOAuthLoginProviderSettingsCommand,
    ) -> OAuthLoginServiceFuture<'_, AdminOAuthLoginProviderSettings> {
        Box::pin(async { Err(OAuthLoginError::Internal) })
    }
}

struct FakeSessionAuthenticator;

impl SessionAuthenticator for FakeSessionAuthenticator {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn issue_for_user(&self, user_id: UserId) -> SessionLoginFuture<'_> {
        Box::pin(async move {
            Ok(IssuedSession::from_parts(
                SessionToken::from_string("oauth.session.token".to_owned()),
                SessionPrincipal::new(user_id, SessionRole::User),
                current_timestamp() + 3_600,
            ))
        })
    }

    fn authenticate<'a>(&'a self, _token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidSession) })
    }
}

#[tokio::test]
async fn start_and_exchange_use_no_store_json_and_existing_session_shape() {
    let service = Arc::new(FakeOAuthLoginService::new());
    let app = router(Arc::clone(&service));
    let start = app
        .clone()
        .oneshot(
            Request::post("/api/auth/oauth/github/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::OK);
    assert_eq!(start.headers()[CACHE_CONTROL], "no-store");
    let set_cookie = start.headers()[SET_COOKIE].to_str().unwrap();
    assert!(set_cookie.starts_with("af_oauth_login_state="));
    assert!(!set_cookie.contains("opaque"));
    assert!(set_cookie.contains("Path=/api/auth/oauth/github/callback"));
    assert!(set_cookie.contains("Max-Age=600"));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));
    assert!(set_cookie.contains("Secure"));
    let body: Value =
        serde_json::from_slice(&to_bytes(start.into_body(), 4096).await.unwrap()).unwrap();
    assert!(
        body["authorization_url"]
            .as_str()
            .unwrap()
            .starts_with("https://github.com/")
    );

    let exchange = app
        .oneshot(json_request(
            "/api/auth/oauth/exchange",
            json!({"ticket": TICKET}),
        ))
        .await
        .unwrap();
    assert_eq!(exchange.status(), StatusCode::OK);
    assert_eq!(exchange.headers()[CACHE_CONTROL], "no-store");
    let body: Value =
        serde_json::from_slice(&to_bytes(exchange.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body["access_token"], "oauth.session.token");
    assert_eq!(body["user"]["id"], 42);
    assert_eq!(body["user"]["role"], "user");
}

#[tokio::test]
async fn callback_redirects_with_no_store_and_no_referrer_for_success_and_cancel() {
    let service = Arc::new(FakeOAuthLoginService::new());
    let app = router(Arc::clone(&service));
    let success_cookie = begin_cookie(&app).await;
    let success = app
        .clone()
        .oneshot(
            Request::get("/api/auth/oauth/github/callback?state=opaque&code=code")
                .header(COOKIE, success_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(success.status(), StatusCode::SEE_OTHER);
    assert_eq!(success.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(success.headers()["referrer-policy"], "no-referrer");
    assert!(
        success.headers()[SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert!(
        success.headers()[LOCATION]
            .to_str()
            .unwrap()
            .contains("ticket=")
    );

    let cancel_cookie = begin_cookie(&app).await;
    let cancelled = app
        .oneshot(
            Request::get("/api/auth/oauth/github/callback?state=opaque&error=access_denied")
                .header(COOKIE, cancel_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cancelled.status(), StatusCode::SEE_OTHER);
    assert!(
        cancelled.headers()[LOCATION]
            .to_str()
            .unwrap()
            .ends_with("error=cancelled")
    );
    assert_eq!(
        service.calls.lock().unwrap().as_slice(),
        ["begin", "complete", "begin", "cancel"]
    );
}

#[tokio::test]
async fn callback_rejects_missing_or_mismatched_browser_state_cookie() {
    let service = Arc::new(FakeOAuthLoginService::new());
    let app = router(Arc::clone(&service));
    let missing = app
        .clone()
        .oneshot(
            Request::get("/api/auth/oauth/github/callback?state=opaque&code=code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::SEE_OTHER);
    assert!(
        missing.headers()[LOCATION]
            .to_str()
            .unwrap()
            .ends_with("error=failed")
    );
    assert!(
        missing.headers()[SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );

    let mismatched = app
        .oneshot(
            Request::get("/api/auth/oauth/github/callback?state=opaque&error=access_denied")
                .header(COOKIE, format!("af_oauth_login_state={}", "0".repeat(64)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mismatched.status(), StatusCode::SEE_OTHER);
    assert!(
        mismatched.headers()[LOCATION]
            .to_str()
            .unwrap()
            .ends_with("error=failed")
    );
    assert_eq!(
        service.calls.lock().unwrap().as_slice(),
        ["failure", "failure"]
    );
}

fn router(service: Arc<FakeOAuthLoginService>) -> axum::Router {
    let service: Arc<dyn OAuthLoginService> = service;
    build_oauth_login_router(service, Arc::new(FakeSessionAuthenticator))
}

fn json_request(uri: &str, body: Value) -> Request<Body> {
    Request::post(uri)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn begin_cookie(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/auth/oauth/github/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.headers()[SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
