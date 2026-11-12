use std::sync::{Arc, Mutex};

use af_admin::{
    CreatePersonalAccessTokenCommand, IssuedPersonalAccessToken, IssuedStepUpProof,
    LoginCredentials, PersonalAccessToken, PersonalAccessTokenAccessRecord,
    PersonalAccessTokenAuthentication, PersonalAccessTokenDigest, PersonalAccessTokenError,
    PersonalAccessTokenFuture, PersonalAccessTokenLookup, PersonalAccessTokenMutationContext,
    PersonalAccessTokenPage, PersonalAccessTokenPolicy, PersonalAccessTokenService,
    PersonalAccessTokenStepUpEvidence, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole, UpdatePersonalAccessTokenCommand,
};
use af_domain::{GroupId, TrustedClientIp, UserId};
use axum::{
    Extension, Router,
    body::{Body, to_bytes},
    extract::Request,
    middleware,
    routing::{get, post},
};
use http::{
    StatusCode,
    header::{AUTHORIZATION, CACHE_CONTROL, WWW_AUTHENTICATE},
};
use tower::ServiceExt;

const TOKEN: &str = "af_pat_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

struct StubTokens {
    authentication: PersonalAccessTokenAuthentication,
    audit: Mutex<Vec<PersonalAccessTokenAccessRecord>>,
}

impl PersonalAccessTokenService for StubTokens {
    fn list(
        &self,
        _: SessionPrincipal,
        _: Option<i64>,
        _: usize,
    ) -> PersonalAccessTokenFuture<'_, PersonalAccessTokenPage> {
        Box::pin(async { Err(PersonalAccessTokenError::Forbidden) })
    }
    fn create(
        &self,
        _: SessionPrincipal,
        _: CreatePersonalAccessTokenCommand,
        _: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenFuture<'_, IssuedPersonalAccessToken> {
        Box::pin(async { Err(PersonalAccessTokenError::Forbidden) })
    }
    fn update(
        &self,
        _: SessionPrincipal,
        _: i64,
        _: UpdatePersonalAccessTokenCommand,
        _: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenFuture<'_, PersonalAccessToken> {
        Box::pin(async { Err(PersonalAccessTokenError::Forbidden) })
    }
    fn revoke(
        &self,
        _: SessionPrincipal,
        _: i64,
        _: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenFuture<'_, ()> {
        Box::pin(async { Err(PersonalAccessTokenError::Forbidden) })
    }
    fn revoke_all(
        &self,
        _: SessionPrincipal,
        _: UserId,
        _: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenFuture<'_, u64> {
        Box::pin(async { Err(PersonalAccessTokenError::Forbidden) })
    }
    fn policy(
        &self,
        _: SessionPrincipal,
    ) -> PersonalAccessTokenFuture<'_, PersonalAccessTokenPolicy> {
        Box::pin(async { Err(PersonalAccessTokenError::Forbidden) })
    }
    fn update_policy(
        &self,
        _: SessionPrincipal,
        _: PersonalAccessTokenPolicy,
        _: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenFuture<'_, PersonalAccessTokenPolicy> {
        Box::pin(async { Err(PersonalAccessTokenError::Forbidden) })
    }
    fn issue_step_up_proof(
        &self,
        _: SessionPrincipal,
        _: PersonalAccessTokenStepUpEvidence,
        _: String,
        _: Option<i64>,
        _: String,
        _: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenFuture<'_, IssuedStepUpProof> {
        Box::pin(async { Err(PersonalAccessTokenError::Forbidden) })
    }
    fn authenticate<'a>(
        &'a self,
        _: &'a PersonalAccessTokenDigest,
        _: TrustedClientIp,
    ) -> PersonalAccessTokenFuture<'a, PersonalAccessTokenLookup> {
        Box::pin(async {
            Ok(PersonalAccessTokenLookup::Authenticated(
                self.authentication.clone(),
            ))
        })
    }
    fn admit_request_ip(&self, _: TrustedClientIp) -> PersonalAccessTokenFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn admit_authenticated_request<'a>(
        &'a self,
        _: &'a PersonalAccessTokenAuthentication,
    ) -> PersonalAccessTokenFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn record_access(
        &self,
        record: PersonalAccessTokenAccessRecord,
    ) -> PersonalAccessTokenFuture<'_, ()> {
        Box::pin(async move {
            self.audit.lock().unwrap().push(record);
            Ok(())
        })
    }
}

struct StubSession(Arc<StubTokens>);
impl SessionAuthenticator for StubSession {
    fn login<'a>(&'a self, _: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }
    fn authenticate<'a>(&'a self, _: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidSession) })
    }
    fn personal_access_tokens(&self) -> Option<Arc<dyn PersonalAccessTokenService>> {
        Some(self.0.clone())
    }
}

fn router(role: SessionRole, scopes: &[&str]) -> (Router, Arc<StubTokens>) {
    let service = Arc::new(StubTokens {
        authentication: PersonalAccessTokenAuthentication {
            principal: SessionPrincipal::new(UserId::new(7).unwrap(), role),
            group_id: GroupId::new(1).unwrap(),
            token_id: 9,
            scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
            expires_at: Some(4_000_000_000),
        },
        audit: Mutex::new(Vec::new()),
    });
    let authentication = middleware::from_fn_with_state(
        crate::management_auth::ManagementAuthenticationState::new(Arc::new(StubSession(
            Arc::clone(&service),
        ))),
        crate::management_auth::authenticate_management_session,
    );
    let router = Router::new()
        .route("/api/tokens", get(inspect_request))
        .route("/api/admin/users", get(inspect_request))
        .route("/api/unregistered", get(inspect_request))
        .route("/api/account/access-tokens", post(inspect_request))
        .layer(authentication)
        .layer(Extension(TrustedClientIp::new(
            "192.0.2.10".parse().unwrap(),
        )))
        .layer(middleware::from_fn(crate::middleware::ensure_request_id));
    (router, service)
}

async fn inspect_request(
    Extension(session): Extension<SessionAuthentication>,
    Extension(token): Extension<PersonalAccessTokenAuthentication>,
    request: Request,
) -> axum::Json<serde_json::Value> {
    axum::Json(
        serde_json::json!({ "user":session.principal().user_id().get(),"token":token.token_id(),
        "authorization_visible":request.headers().contains_key(AUTHORIZATION),
        "interactive_evidence":request.extensions().get::<super::access_token_auth::InteractiveSessionEvidence>().is_some() }),
    )
}

#[tokio::test]
async fn personal_access_token_preserves_actor_removes_secret_and_audits_acceptance_and_result() {
    let (router, service) = router(SessionRole::User, &["api_keys:read"]);
    let response = router
        .oneshot(
            http::Request::builder()
                .uri("/api/tokens")
                .header(AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 2048).await.unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"user":7,"token":9,"authorization_visible":false,"interactive_evidence":false})
    );
    let audits = service.audit.lock().unwrap();
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[0].status_code, 102);
    assert_eq!(audits[1].status_code, 200);
    assert!(
        audits.iter().all(
            |audit| audit.user_id == Some(UserId::new(7).unwrap()) && audit.token_id == Some(9)
        )
    );
}

#[tokio::test]
async fn personal_access_token_cannot_cross_scope_role_or_interactive_boundaries() {
    for (role, scopes, method, path) in [
        (
            SessionRole::User,
            vec!["account:read"],
            "GET",
            "/api/tokens",
        ),
        (
            SessionRole::User,
            vec!["admin:list_admin_users:execute"],
            "GET",
            "/api/admin/users",
        ),
        (
            SessionRole::Admin,
            vec!["api_keys:read"],
            "GET",
            "/api/admin/users",
        ),
        (
            SessionRole::Admin,
            vec!["admin:list_admin_users:execute"],
            "POST",
            "/api/account/access-tokens",
        ),
        (
            SessionRole::Admin,
            vec!["admin:list_admin_users:execute"],
            "GET",
            "/api/unregistered",
        ),
    ] {
        let (router, _) = router(role, &scopes);
        let response = router
            .oneshot(
                http::Request::builder()
                    .method(method)
                    .uri(path)
                    .header(AUTHORIZATION, format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method} {path}");
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    }
}

#[tokio::test]
async fn admin_pat_uses_its_concrete_registered_operation_grant() {
    let (router, _) = router(SessionRole::Admin, &["admin:list_admin_users:execute"]);
    let response = router
        .oneshot(
            http::Request::builder()
                .uri("/api/admin/users")
                .header(AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn pat_rejects_query_credentials_and_model_key_carrier_conflicts() {
    for (path, extra_header) in [
        ("/api/tokens?access_token=leaked", false),
        ("/api/tokens", true),
    ] {
        let (router, service) = router(SessionRole::User, &["api_keys:read"]);
        let mut request = http::Request::builder()
            .uri(path)
            .header(AUTHORIZATION, format!("Bearer {TOKEN}"));
        if extra_header {
            request = request.header("x-api-key", "another-credential");
        }
        let response = router
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            response.headers()[WWW_AUTHENTICATE]
                .to_str()
                .unwrap()
                .contains("invalid_request")
        );
        assert!(service.audit.lock().unwrap().is_empty());
    }
}
