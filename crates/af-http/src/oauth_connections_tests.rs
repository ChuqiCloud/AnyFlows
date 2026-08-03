use std::sync::{Arc, Mutex};

use af_admin::{
    LoginCredentials, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole,
};
use af_domain::{ChannelId, CredentialId, GroupId, UserId};
use axum::{body::Body, extract::ConnectInfo};
use http::{Method, Request, StatusCode, header::AUTHORIZATION};
use tower::ServiceExt as _;

use super::oauth_connections::*;

struct AcceptAdminSessions;

impl SessionAuthenticator for AcceptAdminSessions {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, _token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async {
            Ok(SessionAuthentication::new(
                admin_principal(),
                GroupId::new(1).unwrap(),
                u64::MAX,
            ))
        })
    }
}

#[derive(Default)]
struct RecordingOAuthService {
    callbacks: Mutex<Vec<String>>,
}

impl RecordingOAuthService {
    fn callbacks(&self) -> Vec<String> {
        self.callbacks.lock().unwrap().clone()
    }
}

impl AdminOAuthConnectionService for RecordingOAuthService {
    fn provider_statuses(
        &self,
        principal: SessionPrincipal,
    ) -> Result<Vec<AdminOAuthProviderStatus>, AdminOAuthConnectionError> {
        assert_eq!(principal, admin_principal());
        Ok(vec![AdminOAuthProviderStatus::new(
            AdminOAuthProvider::Codex,
            "http://localhost:1455/auth/callback".to_owned(),
            1_455,
            "/auth/callback".to_owned(),
            true,
        )])
    }

    fn begin<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
        provider: AdminOAuthProvider,
    ) -> AdminOAuthBeginFuture<'a> {
        Box::pin(async move {
            assert_eq!(principal, admin_principal());
            assert_eq!(channel_id.get(), 11);
            assert_eq!(credential_id.get(), 22);
            assert_eq!(provider, AdminOAuthProvider::Codex);
            Ok(AdminOAuthAuthorizationStart::new(
                provider,
                "https://auth.example/authorize?state=private-state".to_owned(),
                "http://localhost:1455/auth/callback".to_owned(),
                600,
                true,
            ))
        })
    }

    fn complete_manual<'a>(
        &'a self,
        principal: SessionPrincipal,
        provider: AdminOAuthProvider,
        callback_url: String,
    ) -> AdminOAuthCompleteFuture<'a> {
        Box::pin(async move {
            assert_eq!(principal, admin_principal());
            assert_eq!(provider, AdminOAuthProvider::Codex);
            self.callbacks.lock().unwrap().push(callback_url);
            Ok(AdminOAuthCompletionOutcome::Connected)
        })
    }

    fn complete_loopback<'a>(
        &'a self,
        provider: AdminOAuthProvider,
        callback_url: String,
    ) -> AdminOAuthCompleteFuture<'a> {
        Box::pin(async move {
            assert_eq!(provider, AdminOAuthProvider::Codex);
            self.callbacks.lock().unwrap().push(callback_url);
            Ok(AdminOAuthCompletionOutcome::Connected)
        })
    }
}

#[tokio::test]
async fn admin_oauth_routes_require_session_and_return_no_store_contracts() {
    let service = Arc::new(RecordingOAuthService::default());
    let service_port: Arc<dyn AdminOAuthConnectionService> = service.clone();
    let router = build_admin_oauth_connection_router(service_port, Arc::new(AcceptAdminSessions));

    let unauthenticated = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/admin/oauth/providers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let providers = router
        .clone()
        .oneshot(admin_request(
            Method::GET,
            "/api/admin/oauth/providers",
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(providers.status(), StatusCode::OK);
    assert_eq!(providers.headers()["cache-control"], "no-store");
    let providers = response_text(providers).await;
    assert!(providers.contains("loopback_listener_ready"));
    assert!(providers.contains("manual_callback_supported"));

    let start = router
        .clone()
        .oneshot(admin_request(
            Method::POST,
            "/api/admin/channels/11/credentials/22/oauth-authorizations",
            Body::from(r#"{"provider":"codex"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::CREATED);
    assert_eq!(start.headers()["cache-control"], "no-store");
    let start = response_text(start).await;
    assert!(start.contains("authorization_url"));
    assert!(start.contains("private-state"));

    let callback = "http://localhost:1455/auth/callback?state=state-marker&code=code-marker";
    let completed = router
        .oneshot(admin_request(
            Method::POST,
            "/api/admin/oauth/authorizations/manual-callback",
            Body::from(format!(
                r#"{{"provider":"codex","callback_url":"{callback}"}}"#
            )),
        ))
        .await
        .unwrap();
    assert_eq!(completed.status(), StatusCode::OK);
    assert_eq!(service.callbacks(), vec![callback]);
}

#[tokio::test]
async fn loopback_router_uses_fixed_origin_and_rejects_wrong_host_or_method() {
    let service = Arc::new(RecordingOAuthService::default());
    let binding = OAuthLoopbackBinding::new(
        AdminOAuthProvider::Codex,
        "127.0.0.1:1455".parse().unwrap(),
        "http://localhost:1455/auth/callback".to_owned(),
    )
    .unwrap();
    let service_port: Arc<dyn AdminOAuthConnectionService> = service.clone();
    let router = build_oauth_loopback_callback_router(service_port, binding).into_router();

    let succeeded = router
        .clone()
        .oneshot(loopback_request(
            Method::GET,
            "/auth/callback?state=state-marker&code=code-marker",
            "localhost:1455",
        ))
        .await
        .unwrap();
    assert_eq!(succeeded.status(), StatusCode::OK);
    assert_eq!(succeeded.headers()["cache-control"], "no-store");
    assert_eq!(succeeded.headers()["referrer-policy"], "no-referrer");
    let body = response_text(succeeded).await;
    assert!(body.contains("授权已完成"));
    assert!(!body.contains("state-marker"));
    assert!(!body.contains("code-marker"));
    assert_eq!(
        service.callbacks(),
        vec!["http://localhost:1455/auth/callback?state=state-marker&code=code-marker"]
    );

    let wrong_host = router
        .clone()
        .oneshot(loopback_request(
            Method::GET,
            "/auth/callback?state=other&code=other",
            "127.0.0.1:1455",
        ))
        .await
        .unwrap();
    assert_eq!(wrong_host.status(), StatusCode::BAD_REQUEST);
    assert_eq!(service.callbacks().len(), 1);

    let wrong_method = router
        .clone()
        .oneshot(loopback_request(
            Method::POST,
            "/auth/callback?state=other&code=other",
            "localhost:1455",
        ))
        .await
        .unwrap();
    assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(wrong_method.headers()["cache-control"], "no-store");
    assert_eq!(service.callbacks().len(), 1);

    let head = router
        .oneshot(loopback_request(
            Method::HEAD,
            "/auth/callback?state=other&code=other",
            "localhost:1455",
        ))
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(service.callbacks().len(), 1);
}

#[tokio::test]
async fn loopback_router_rejects_untrusted_peer_body_path_and_unbounded_query() {
    let service = Arc::new(RecordingOAuthService::default());
    let binding = OAuthLoopbackBinding::new(
        AdminOAuthProvider::Codex,
        "127.0.0.1:1455".parse().unwrap(),
        "http://localhost:1455/auth/callback".to_owned(),
    )
    .unwrap();
    let service_port: Arc<dyn AdminOAuthConnectionService> = service.clone();
    let router = build_oauth_loopback_callback_router(service_port, binding).into_router();

    let mut wrong_peer = loopback_request(
        Method::GET,
        "/auth/callback?state=state&code=code",
        "localhost:1455",
    );
    wrong_peer.extensions_mut().insert(ConnectInfo(
        "198.51.100.9:52123"
            .parse::<std::net::SocketAddr>()
            .unwrap(),
    ));
    let wrong_peer = router.clone().oneshot(wrong_peer).await.unwrap();
    assert_eq!(wrong_peer.status(), StatusCode::BAD_REQUEST);

    let oversized_query = format!(
        "/auth/callback?state={}",
        "a".repeat(MAX_OAUTH_LOOPBACK_QUERY_BYTES + 1)
    );
    let oversized = router
        .clone()
        .oneshot(loopback_request(
            Method::GET,
            &oversized_query,
            "localhost:1455",
        ))
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::BAD_REQUEST);

    let missing_path = router
        .clone()
        .oneshot(loopback_request(
            Method::GET,
            "/wrong-callback?state=state&code=code",
            "localhost:1455",
        ))
        .await
        .unwrap();
    assert_eq!(missing_path.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing_path.headers()["cache-control"], "no-store");
    assert_eq!(
        missing_path.headers()["content-security-policy"],
        "default-src 'none'; frame-ancestors 'none'; base-uri 'none'"
    );

    let mut body_request = loopback_request(
        Method::GET,
        "/auth/callback?state=state&code=code",
        "localhost:1455",
    );
    body_request
        .headers_mut()
        .insert("content-length", "1".parse().unwrap());
    *body_request.body_mut() = Body::from("x");
    let body = router.oneshot(body_request).await.unwrap();
    assert_eq!(body.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body.headers()["cache-control"], "no-store");
    assert!(service.callbacks().is_empty());
}

#[test]
fn loopback_binding_rejects_non_loopback_or_non_registered_hosts() {
    for (bind, redirect) in [
        ("0.0.0.0:1455", "http://localhost:1455/auth/callback"),
        ("127.0.0.1:1455", "http://127.0.0.1:1455/auth/callback"),
        ("127.0.0.1:1455", "http://localhost:1456/auth/callback"),
    ] {
        assert_eq!(
            OAuthLoopbackBinding::new(
                AdminOAuthProvider::Codex,
                bind.parse().unwrap(),
                redirect.to_owned(),
            )
            .unwrap_err(),
            OAuthLoopbackBindingError::InvalidContract
        );
    }
}

fn admin_principal() -> SessionPrincipal {
    SessionPrincipal::new(UserId::new(7).unwrap(), SessionRole::Admin)
}

fn admin_request(method: Method, uri: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, "Bearer management-token")
        .header("content-type", "application/json")
        .body(body)
        .unwrap()
}

fn loopback_request(method: Method, uri: &str, host: &str) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", host)
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:52123".parse::<std::net::SocketAddr>().unwrap(),
    ));
    request
}

async fn response_text(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1_024)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}
