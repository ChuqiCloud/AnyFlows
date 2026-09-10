use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use af_admin::{
    ApiKeyDigest, LoginCredentials, PlaygroundAuthenticationFuture, PresentedApiKey,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole, TokenAuthentication,
    TokenAuthenticationError, TokenAuthenticationFuture, TokenAuthenticator,
};
use af_config::{ClientIpSource, CorsOrigin, ServerConfig};
use af_domain::{
    GatewayPrincipal, GroupId, IpCidr, Protocol, TokenId, TokenModelPolicy, TrustedClientIp, UserId,
};
use axum::{
    Extension, Router,
    body::{Body, to_bytes},
    extract::ConnectInfo,
    middleware,
    response::Response,
    routing::{get, post},
};
use http::{
    HeaderMap, HeaderValue, Method, Request, StatusCode, Uri,
    header::{
        ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, ACCESS_CONTROL_REQUEST_METHOD,
        AUTHORIZATION, CONTENT_LENGTH, ORIGIN, RETRY_AFTER,
    },
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    HEALTH_PATH, QueryApiKeyPolicy, READINESS_PATH, REQUEST_ID_HEADER_NAME, ReadinessFuture,
    ReadinessHandle, ReadinessProbe,
    authentication::{AuthenticationState, authenticate_api_key, authenticate_playground_session},
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    operations::operations_router,
    router::build_router_with_routes,
};

const TEST_KEY: &str = "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
const ALLOWED_ORIGIN: &str = "https://console.example";
const TEST_PEER_IP: &str = "192.0.2.10";

#[derive(Clone, Copy)]
enum AuthenticationOutcome {
    Success,
    InvalidApiKey,
    Internal,
    RateLimited,
    RequestLimitReached,
}

struct FakeAuthenticator {
    outcome: AuthenticationOutcome,
    calls: AtomicUsize,
    digests: Mutex<Vec<String>>,
    client_ips: Mutex<Vec<TrustedClientIp>>,
}

struct FakePlaygroundAuthenticator {
    api_key_calls: AtomicUsize,
    user_ids: Mutex<Vec<UserId>>,
}

struct FakeSessionAuthenticator;

struct AlwaysReadyProbe;

impl ReadinessProbe for AlwaysReadyProbe {
    fn check(&self) -> ReadinessFuture<'_> {
        Box::pin(async { true })
    }
}

impl FakeAuthenticator {
    fn new(outcome: AuthenticationOutcome) -> Self {
        Self {
            outcome,
            calls: AtomicUsize::new(0),
            digests: Mutex::new(Vec::new()),
            client_ips: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    fn digests(&self) -> Vec<String> {
        self.digests.lock().unwrap().clone()
    }

    fn client_ips(&self) -> Vec<TrustedClientIp> {
        self.client_ips.lock().unwrap().clone()
    }
}

impl FakePlaygroundAuthenticator {
    fn new() -> Self {
        Self {
            api_key_calls: AtomicUsize::new(0),
            user_ids: Mutex::new(Vec::new()),
        }
    }

    fn api_key_calls(&self) -> usize {
        self.api_key_calls.load(Ordering::Relaxed)
    }

    fn user_ids(&self) -> Vec<UserId> {
        self.user_ids.lock().unwrap().clone()
    }
}

impl TokenAuthenticator for FakeAuthenticator {
    fn authenticate<'a>(
        &'a self,
        digest: &'a ApiKeyDigest,
        client_ip: TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.digests
            .lock()
            .unwrap()
            .push(digest.as_str().to_owned());
        self.client_ips.lock().unwrap().push(client_ip);
        let outcome = self.outcome;
        Box::pin(async move {
            match outcome {
                AuthenticationOutcome::Success => Ok(test_authentication()),
                AuthenticationOutcome::InvalidApiKey => {
                    Err(TokenAuthenticationError::InvalidApiKey)
                }
                AuthenticationOutcome::Internal => Err(TokenAuthenticationError::Internal),
                AuthenticationOutcome::RateLimited => Err(TokenAuthenticationError::RateLimited {
                    retry_after: af_domain::UpstreamRetryAfter::from_seconds(37).unwrap(),
                }),
                AuthenticationOutcome::RequestLimitReached => {
                    Err(TokenAuthenticationError::RequestLimitReached)
                }
            }
        })
    }
}

impl TokenAuthenticator for FakePlaygroundAuthenticator {
    fn authenticate<'a>(
        &'a self,
        _digest: &'a ApiKeyDigest,
        _client_ip: TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a> {
        self.api_key_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(TokenAuthenticationError::Internal) })
    }

    fn authenticate_playground<'a>(
        &'a self,
        user_id: UserId,
    ) -> PlaygroundAuthenticationFuture<'a> {
        self.user_ids.lock().unwrap().push(user_id);
        Box::pin(async { Ok(test_playground_authentication()) })
    }
}

impl SessionAuthenticator for FakeSessionAuthenticator {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async move {
            if token != "valid-session" {
                return Err(SessionAuthenticationError::InvalidSession);
            }
            Ok(SessionAuthentication::new(
                SessionPrincipal::new(UserId::new(22).unwrap(), SessionRole::User),
                GroupId::new(33).unwrap(),
                u64::MAX,
            ))
        })
    }
}

fn test_principal() -> GatewayPrincipal {
    GatewayPrincipal::new(
        TokenId::new(11).unwrap(),
        UserId::new(22).unwrap(),
        GroupId::new(33).unwrap(),
    )
}

fn test_authentication() -> TokenAuthentication {
    TokenAuthentication::new(test_principal(), TokenModelPolicy::unrestricted())
}

fn test_playground_authentication() -> TokenAuthentication {
    TokenAuthentication::new(
        GatewayPrincipal::playground(
            TokenId::new(41).unwrap(),
            UserId::new(22).unwrap(),
            GroupId::new(33).unwrap(),
        ),
        TokenModelPolicy::unrestricted(),
    )
}

async fn playground_handler(
    Extension(authentication): Extension<TokenAuthentication>,
    headers: HeaderMap,
) -> StatusCode {
    assert!(authentication.principal().is_playground());
    assert_eq!(
        authentication.principal().user_id(),
        UserId::new(22).unwrap()
    );
    assert!(headers.get(AUTHORIZATION).is_none());
    assert!(headers.get("x-api-key").is_none());
    assert!(headers.get("x-goog-api-key").is_none());
    StatusCode::NO_CONTENT
}

fn playground_router(authenticator: Arc<FakePlaygroundAuthenticator>) -> Router {
    let config = ServerConfig::default();
    let playground_authentication = middleware::from_fn_with_state(
        AuthenticationState::new(authenticator, QueryApiKeyPolicy::Deny, &config),
        authenticate_playground_session,
    );
    let session_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::new(FakeSessionAuthenticator)),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/playground/v1/chat/completions",
            post(playground_handler).route_layer(playground_authentication),
        )
        .layer(session_authentication)
}

fn server_config(origins: &[&str]) -> ServerConfig {
    ServerConfig::default().with_cors_allowed_origins(
        origins
            .iter()
            .map(|origin| origin.parse::<CorsOrigin>().unwrap()),
    )
}

fn authenticated_router(
    authenticator: Arc<FakeAuthenticator>,
    query_policy: QueryApiKeyPolicy,
    origins: &[&str],
    body_limit: usize,
) -> Router {
    let config = server_config(origins);
    authenticated_router_with_network(
        authenticator,
        query_policy,
        config,
        body_limit,
        Some(TEST_PEER_IP),
    )
}

fn authenticated_router_with_network(
    authenticator: Arc<FakeAuthenticator>,
    query_policy: QueryApiKeyPolicy,
    config: ServerConfig,
    body_limit: usize,
    peer_ip: Option<&str>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        AuthenticationState::new(authenticator, query_policy, &config),
        authenticate_api_key,
    );
    let public_routes = Router::new().route(
        "/protected",
        post(protected_handler).route_layer(authentication),
    );
    let readiness = ReadinessHandle::new(AlwaysReadyProbe);
    readiness.mark_ready();
    let operations_routes = operations_router(readiness).route("/ops", get(operations_handler));
    let router = build_router_with_routes(public_routes, operations_routes, &config, body_limit);
    match peer_ip {
        Some(peer_ip) => router.layer(Extension(ConnectInfo(SocketAddr::new(
            peer_ip.parse().unwrap(),
            45123,
        )))),
        None => router,
    }
}

async fn protected_handler(
    Extension(authentication): Extension<TokenAuthentication>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> StatusCode {
    assert_eq!(authentication.principal(), test_principal());
    assert!(authentication.model_policy().allows("test-model"));
    assert!(headers.get(AUTHORIZATION).is_none());
    assert!(headers.get("x-api-key").is_none());
    assert!(headers.get("x-goog-api-key").is_none());
    assert!(headers.get("x-forwarded-for").is_none());
    assert!(headers.get("forwarded").is_none());
    assert!(peer.is_none());
    assert!(!uri.to_string().contains(TEST_KEY));
    StatusCode::NO_CONTENT
}

async fn operations_handler() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn bearer_request(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {TEST_KEY}"))
        .header("x-forwarded-for", "198.51.100.7")
        .header("forwarded", "for=198.51.100.7")
        .body(Body::empty())
        .unwrap()
}

fn expected_digest() -> String {
    PresentedApiKey::parse(TEST_KEY)
        .unwrap()
        .digest()
        .as_str()
        .to_owned()
}

fn assert_request_id(response: &Response) {
    let values = response.headers().get_all(REQUEST_ID_HEADER_NAME);
    assert_eq!(values.iter().count(), 1);
    assert!(!values.iter().next().unwrap().is_empty());
}

async fn response_json(response: Response) -> Value {
    let body = to_bytes(response.into_body(), 16 * 1024).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn valid_key_is_hashed_once_and_verified_principal_reaches_handler() {
    let authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::Success));
    let response =
        authenticated_router(Arc::clone(&authenticator), QueryApiKeyPolicy::Deny, &[], 64)
            .oneshot(bearer_request("/protected"))
            .await
            .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_request_id(&response);
    assert_eq!(authenticator.calls(), 1);
    assert_eq!(authenticator.digests(), vec![expected_digest()]);
    assert_eq!(
        authenticator.client_ips(),
        vec![TrustedClientIp::new(TEST_PEER_IP.parse().unwrap())]
    );
}

#[tokio::test]
async fn playground_requires_session_and_authenticates_by_session_user() {
    let authenticator = Arc::new(FakePlaygroundAuthenticator::new());
    let missing_session = playground_router(Arc::clone(&authenticator))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/playground/v1/chat/completions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_session.status(), StatusCode::UNAUTHORIZED);
    assert!(authenticator.user_ids().is_empty());

    let valid_session = playground_router(Arc::clone(&authenticator))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/playground/v1/chat/completions")
                .header(AUTHORIZATION, "Bearer valid-session")
                .header("x-api-key", "sdk-placeholder")
                .header("x-goog-api-key", "sdk-placeholder")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(valid_session.status(), StatusCode::NO_CONTENT);
    assert_eq!(authenticator.user_ids(), vec![UserId::new(22).unwrap()]);
    assert_eq!(authenticator.api_key_calls(), 0);
}

#[tokio::test]
async fn authenticated_rate_limit_returns_retry_after_header() {
    let authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::RateLimited));
    let response = authenticated_router(
        Arc::clone(&authenticator),
        QueryApiKeyPolicy::Deny,
        &[ALLOWED_ORIGIN],
        64,
    )
    .oneshot({
        let mut request = bearer_request("/protected");
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_static(ALLOWED_ORIGIN));
        request
    })
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()[RETRY_AFTER], "37");
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "rate_limited");
    assert_eq!(body["error"]["type"], "rate_limit_error");
    assert_eq!(authenticator.calls(), 1);
}

#[tokio::test]
async fn cumulative_request_limit_returns_insufficient_quota_without_retry_after() {
    let authenticator = Arc::new(FakeAuthenticator::new(
        AuthenticationOutcome::RequestLimitReached,
    ));
    let response = authenticated_router(
        Arc::clone(&authenticator),
        QueryApiKeyPolicy::Deny,
        &[ALLOWED_ORIGIN],
        64,
    )
    .oneshot({
        let mut request = bearer_request("/protected");
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_static(ALLOWED_ORIGIN));
        request
    })
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(!response.headers().contains_key(RETRY_AFTER));
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "insufficient_quota");
    assert_eq!(body["error"]["type"], "insufficient_quota");
    assert_eq!(authenticator.calls(), 1);
}

#[tokio::test]
async fn query_key_requires_router_opt_in_and_is_removed_before_handler() {
    let denied_authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::Success));
    let denied = authenticated_router(
        Arc::clone(&denied_authenticator),
        QueryApiKeyPolicy::Deny,
        &[],
        64,
    )
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri(format!("/protected?model=test&key={TEST_KEY}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(denied_authenticator.calls(), 0);
    assert!(!response_json(denied).await.to_string().contains(TEST_KEY));

    let allowed_authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::Success));
    let allowed = authenticated_router(
        Arc::clone(&allowed_authenticator),
        QueryApiKeyPolicy::Allow,
        &[],
        64,
    )
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri(format!("/protected?model=test&key={TEST_KEY}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(allowed.status(), StatusCode::NO_CONTENT);
    assert_eq!(allowed_authenticator.calls(), 1);
}

#[tokio::test]
async fn presentation_and_repository_failures_use_closed_openai_errors() {
    let missing_authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::Success));
    let missing = authenticated_router(
        Arc::clone(&missing_authenticator),
        QueryApiKeyPolicy::Deny,
        &[ALLOWED_ORIGIN],
        64,
    )
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/protected")
            .header(ORIGIN, ALLOWED_ORIGIN)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_auth_error(
        missing,
        StatusCode::UNAUTHORIZED,
        "Invalid API key.",
        "invalid_request_error",
        "invalid_api_key",
    )
    .await;
    assert_eq!(missing_authenticator.calls(), 0);

    for (outcome, status, message, error_kind, code) in [
        (
            AuthenticationOutcome::InvalidApiKey,
            StatusCode::UNAUTHORIZED,
            "Invalid API key.",
            "invalid_request_error",
            "invalid_api_key",
        ),
        (
            AuthenticationOutcome::Internal,
            StatusCode::INTERNAL_SERVER_ERROR,
            "An internal server error occurred.",
            "server_error",
            "internal_error",
        ),
    ] {
        let authenticator = Arc::new(FakeAuthenticator::new(outcome));
        let mut request = bearer_request("/protected");
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_static(ALLOWED_ORIGIN));
        let response = authenticated_router(
            Arc::clone(&authenticator),
            QueryApiKeyPolicy::Deny,
            &[ALLOWED_ORIGIN],
            64,
        )
        .oneshot(request)
        .await
        .unwrap();
        assert_auth_error(response, status, message, error_kind, code).await;
        assert_eq!(authenticator.calls(), 1);
        assert_eq!(authenticator.digests(), vec![expected_digest()]);
    }
}

#[tokio::test]
async fn anthropic_route_authentication_failure_uses_anthropic_error_wire() {
    let config = ServerConfig::default();
    let authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::InvalidApiKey));
    let authentication_port: Arc<dyn TokenAuthenticator> = authenticator.clone();
    let authentication = middleware::from_fn_with_state(
        AuthenticationState::new(authentication_port, QueryApiKeyPolicy::Deny, &config)
            .with_error_protocol(Protocol::Anthropic),
        authenticate_api_key,
    );
    let router = Router::new()
        .route(
            "/v1/messages",
            post(protected_handler).route_layer(authentication),
        )
        .layer(Extension(ConnectInfo(SocketAddr::new(
            TEST_PEER_IP.parse().unwrap(),
            45123,
        ))));

    let response = router
        .oneshot(bearer_request("/v1/messages"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response_json(response).await;
    assert_eq!(
        body,
        json!({
            "type": "error",
            "error": {
                "type": "authentication_error",
                "message": "Invalid API key."
            }
        })
    );
    assert_eq!(authenticator.calls(), 1);
}

#[tokio::test]
async fn responses_route_authentication_failure_uses_openai_error_wire() {
    let config = ServerConfig::default();
    let authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::InvalidApiKey));
    let authentication_port: Arc<dyn TokenAuthenticator> = authenticator.clone();
    let authentication = middleware::from_fn_with_state(
        AuthenticationState::new(authentication_port, QueryApiKeyPolicy::Deny, &config)
            .with_error_protocol(Protocol::OpenAiResponses),
        authenticate_api_key,
    );
    let router = Router::new()
        .route(
            "/v1/responses",
            post(protected_handler).route_layer(authentication),
        )
        .layer(Extension(ConnectInfo(SocketAddr::new(
            TEST_PEER_IP.parse().unwrap(),
            45123,
        ))));

    let response = router
        .oneshot(bearer_request("/v1/responses"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response_json(response).await;
    assert_eq!(
        body,
        json!({
            "error": {
                "message": "Invalid API key.",
                "type": "invalid_request_error",
                "param": null,
                "code": "invalid_api_key",
            }
        })
    );
    assert_eq!(authenticator.calls(), 1);
}

#[tokio::test]
async fn gemini_route_authentication_failure_uses_google_error_wire() {
    let config = ServerConfig::default();
    let authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::InvalidApiKey));
    let authentication_port: Arc<dyn TokenAuthenticator> = authenticator.clone();
    let authentication = middleware::from_fn_with_state(
        AuthenticationState::new(authentication_port, QueryApiKeyPolicy::Deny, &config)
            .with_error_protocol(Protocol::Gemini),
        authenticate_api_key,
    );
    let router = Router::new()
        .route(
            "/v1beta/models/{model_action}",
            post(protected_handler).route_layer(authentication),
        )
        .layer(Extension(ConnectInfo(SocketAddr::new(
            TEST_PEER_IP.parse().unwrap(),
            45123,
        ))));

    let response = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1beta/models/test-model:generateContent")
                .header("x-goog-api-key", TEST_KEY)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response_json(response).await;
    assert_eq!(
        body,
        json!({
            "error": {
                "code": 401,
                "message": "Invalid API key.",
                "status": "UNAUTHENTICATED",
            }
        })
    );
    assert_eq!(authenticator.calls(), 1);
}

#[tokio::test]
async fn missing_peer_is_internal_and_never_invokes_authenticator() {
    let authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::Success));
    let response = authenticated_router_with_network(
        Arc::clone(&authenticator),
        QueryApiKeyPolicy::Deny,
        server_config(&[ALLOWED_ORIGIN]),
        64,
        None,
    )
    .oneshot({
        let mut request = bearer_request("/protected");
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_static(ALLOWED_ORIGIN));
        request
    })
    .await
    .unwrap();

    assert_auth_error(
        response,
        StatusCode::INTERNAL_SERVER_ERROR,
        "An internal server error occurred.",
        "server_error",
        "internal_error",
    )
    .await;
    assert_eq!(authenticator.calls(), 0);
    assert!(authenticator.client_ips().is_empty());
}

#[tokio::test]
async fn trusted_xff_chain_is_resolved_before_authentication() {
    let config = ServerConfig::default().with_client_ip_source(
        ClientIpSource::XForwardedFor,
        ["10.0.0.0/8".parse::<IpCidr>().unwrap()],
    );
    let authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::Success));
    let router = authenticated_router_with_network(
        Arc::clone(&authenticator),
        QueryApiKeyPolicy::Deny,
        config.clone(),
        64,
        Some("10.0.0.3"),
    );
    let mut request = bearer_request("/protected");
    request.headers_mut().insert(
        "x-forwarded-for",
        HeaderValue::from_static("198.51.100.99, 203.0.113.7, 10.0.0.2"),
    );
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        authenticator.client_ips(),
        vec![TrustedClientIp::new("203.0.113.7".parse().unwrap())]
    );

    let rejected_authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::Success));
    let mut invalid = bearer_request("/protected");
    invalid
        .headers_mut()
        .insert("x-forwarded-for", HeaderValue::from_static("10.0.0.2"));
    let response = authenticated_router_with_network(
        Arc::clone(&rejected_authenticator),
        QueryApiKeyPolicy::Deny,
        config,
        64,
        Some("10.0.0.3"),
    )
    .oneshot(invalid)
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(rejected_authenticator.calls(), 0);
}

async fn assert_auth_error(
    response: Response,
    status: StatusCode,
    message: &str,
    error_kind: &str,
    code: &str,
) {
    assert_eq!(response.status(), status);
    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&HeaderValue::from_static(ALLOWED_ORIGIN))
    );
    assert!(
        response.headers()[ACCESS_CONTROL_EXPOSE_HEADERS]
            .to_str()
            .unwrap()
            .contains(REQUEST_ID_HEADER_NAME)
    );
    assert_request_id(&response);
    let body = response_json(response).await;
    assert_eq!(
        body,
        json!({
            "error": {
                "message": message,
                "type": error_kind,
                "param": null,
                "code": code,
            }
        })
    );
    let rendered = body.to_string();
    assert!(!rendered.contains(TEST_KEY));
    assert!(!rendered.contains(&expected_digest()));
}

#[tokio::test]
async fn outer_boundaries_and_operations_never_invoke_authenticator() {
    const BODY_LIMIT: usize = 16;
    let authenticator = Arc::new(FakeAuthenticator::new(AuthenticationOutcome::Success));
    let router = authenticated_router_with_network(
        Arc::clone(&authenticator),
        QueryApiKeyPolicy::Deny,
        server_config(&[ALLOWED_ORIGIN]),
        BODY_LIMIT,
        None,
    );

    let preflight = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/protected")
                .header(ORIGIN, ALLOWED_ORIGIN)
                .header(ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .header("x-forwarded-for", "invalid,")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preflight.status(), StatusCode::OK);
    assert_request_id(&preflight);

    let forbidden = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/protected")
                .header(ORIGIN, "https://evil.example")
                .header("x-forwarded-for", "invalid,")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_request_id(&forbidden);

    let too_large = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/protected")
                .header(CONTENT_LENGTH, BODY_LIMIT + 1)
                .header("x-forwarded-for", "invalid,")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(too_large.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_request_id(&too_large);

    let not_found = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/missing")
                .header("x-forwarded-for", "invalid,")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(not_found.status(), StatusCode::NOT_FOUND);
    assert_request_id(&not_found);

    let method_not_allowed = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header("x-forwarded-for", "invalid,")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(method_not_allowed.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_request_id(&method_not_allowed);

    for (path, expected_status) in [
        ("/ops", StatusCode::NO_CONTENT),
        (HEALTH_PATH, StatusCode::OK),
        (READINESS_PATH, StatusCode::OK),
    ] {
        let operations = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(ORIGIN, ALLOWED_ORIGIN)
                    .header("x-forwarded-for", "invalid,")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(operations.status(), expected_status);
        assert!(
            operations
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
        assert!(
            operations
                .headers()
                .get(ACCESS_CONTROL_EXPOSE_HEADERS)
                .is_none()
        );
        assert_request_id(&operations);
    }

    assert_eq!(authenticator.calls(), 0);
    assert!(authenticator.digests().is_empty());
    assert!(authenticator.client_ips().is_empty());
}
