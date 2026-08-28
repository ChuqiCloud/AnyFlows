use std::sync::Arc;

use af_admin::{
    OrganizationServiceAccountRuntimeAuthenticationError,
    OrganizationServiceAccountRuntimeAuthenticator,
};
use af_domain::{
    OrganizationId, OrganizationServiceAccountRuntimeIdentity, OrganizationServiceAccountSecret,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Request, State},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use http::{
    HeaderValue, StatusCode,
    header::{AUTHORIZATION, CACHE_CONTROL, WWW_AUTHENTICATE},
};
use serde::Serialize;
use utoipa::ToSchema;

const RUNTIME_AUTHENTICATION_PATH_PREFIX: &str = "/api/organizations/";
const RUNTIME_AUTHENTICATION_PATH_SUFFIX: &str = "/service-accounts/runtime/authenticate";

#[derive(Clone)]
struct OrganizationServiceAccountRuntimeHttpState {
    authenticator: Arc<dyn OrganizationServiceAccountRuntimeAuthenticator>,
}

/// 构建企业服务账号运行时鉴权入口；该入口不建立管理会话或自然人身份。
pub fn build_organization_service_account_runtime_router(
    authenticator: Arc<dyn OrganizationServiceAccountRuntimeAuthenticator>,
) -> Router {
    let state = OrganizationServiceAccountRuntimeHttpState { authenticator };
    let authentication =
        middleware::from_fn_with_state(state.clone(), authenticate_service_account);
    Router::new()
        .route(
            "/api/organizations/{organization_id}/service-accounts/runtime/authenticate",
            post(confirm_runtime_identity).route_layer(authentication),
        )
        .with_state(state)
}

async fn authenticate_service_account(
    State(state): State<OrganizationServiceAccountRuntimeHttpState>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(organization_id) = parse_organization_id(request.uri().path()) else {
        return RuntimeAuthenticationError::Unauthorized.into_response();
    };
    let secret = match extract_bearer_secret(&mut request).and_then(|value| {
        OrganizationServiceAccountSecret::new(value)
            .map_err(|_| RuntimeAuthenticationError::Unauthorized)
    }) {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    match state
        .authenticator
        .authenticate(organization_id, secret)
        .await
    {
        Ok(identity) => {
            request.extensions_mut().insert(identity);
            let mut response = next.run(request).await;
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(OrganizationServiceAccountRuntimeAuthenticationError::InvalidCredentials) => {
            RuntimeAuthenticationError::Unauthorized.into_response()
        }
        Err(OrganizationServiceAccountRuntimeAuthenticationError::Internal) => {
            RuntimeAuthenticationError::Unavailable.into_response()
        }
    }
}

async fn confirm_runtime_identity(
    Path(organization_id): Path<i64>,
    Extension(identity): Extension<OrganizationServiceAccountRuntimeIdentity>,
) -> Response {
    let Some(organization_id) = OrganizationId::new(organization_id).ok() else {
        return RuntimeAuthenticationError::Unauthorized.into_response();
    };
    if !identity.locator().belongs_to(organization_id) {
        return RuntimeAuthenticationError::Unauthorized.into_response();
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn extract_bearer_secret(request: &mut Request) -> Result<String, RuntimeAuthenticationError> {
    let values = request.headers().get_all(AUTHORIZATION);
    let mut iter = values.iter();
    let Some(value) = iter.next() else {
        return Err(RuntimeAuthenticationError::Unauthorized);
    };
    if iter.next().is_some() {
        return Err(RuntimeAuthenticationError::Unauthorized);
    }
    let raw = value
        .to_str()
        .map_err(|_| RuntimeAuthenticationError::Unauthorized)?;
    let Some((scheme, secret)) = raw.split_once(' ') else {
        return Err(RuntimeAuthenticationError::Unauthorized);
    };
    if !scheme.eq_ignore_ascii_case("Bearer")
        || secret.is_empty()
        || secret.chars().any(char::is_whitespace)
    {
        return Err(RuntimeAuthenticationError::Unauthorized);
    }
    let secret = secret.to_owned();
    request.headers_mut().remove(AUTHORIZATION);
    Ok(secret)
}

fn parse_organization_id(path: &str) -> Option<OrganizationId> {
    let organization_id = path
        .strip_prefix(RUNTIME_AUTHENTICATION_PATH_PREFIX)?
        .strip_suffix(RUNTIME_AUTHENTICATION_PATH_SUFFIX)?;
    if organization_id.is_empty() || organization_id.contains('/') {
        return None;
    }
    organization_id
        .parse()
        .ok()
        .and_then(|value| OrganizationId::new(value).ok())
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationServiceAccountRuntimeErrorResponse {
    code: &'static str,
    message: &'static str,
}

#[derive(Clone, Copy, Debug)]
enum RuntimeAuthenticationError {
    Unauthorized,
    Unavailable,
}

impl IntoResponse for RuntimeAuthenticationError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "invalid_service_account_credentials",
                "Invalid service account credentials",
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_account_authentication_unavailable",
                "Service account authentication unavailable",
            ),
        };
        let mut response =
            Json(OrganizationServiceAccountRuntimeErrorResponse { code, message }).into_response();
        *response.status_mut() = status;
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if matches!(self, Self::Unauthorized) {
            response.headers_mut().insert(
                WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"organization-service-account\""),
            );
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    use af_admin::OrganizationServiceAccountRuntimeAuthenticationFuture;
    use af_domain::{
        OrganizationServiceAccountId, OrganizationServiceAccountKey,
        OrganizationServiceAccountLocator,
    };
    use axum::body::{Body, to_bytes};
    use http::Request;
    use tower::ServiceExt;

    use super::*;

    const SECRET: &str = "sa_1111111111111111111111111111111111111111111111111111111111111111";

    #[derive(Clone, Copy)]
    enum FakeOutcome {
        Success,
        CrossOrganization,
        Invalid,
        Internal,
    }

    struct FakeAuthenticator {
        outcome: FakeOutcome,
        called: AtomicBool,
        observed_organization: Mutex<Option<OrganizationId>>,
    }

    impl FakeAuthenticator {
        fn new(outcome: FakeOutcome) -> Self {
            Self {
                outcome,
                called: AtomicBool::new(false),
                observed_organization: Mutex::new(None),
            }
        }
    }

    impl OrganizationServiceAccountRuntimeAuthenticator for FakeAuthenticator {
        fn authenticate<'a>(
            &'a self,
            organization_id: OrganizationId,
            _secret: OrganizationServiceAccountSecret,
        ) -> OrganizationServiceAccountRuntimeAuthenticationFuture<'a> {
            self.called.store(true, Ordering::SeqCst);
            *self.observed_organization.lock().unwrap() = Some(organization_id);
            Box::pin(async move {
                match self.outcome {
                    FakeOutcome::Success => Ok(runtime_identity(organization_id)),
                    FakeOutcome::CrossOrganization => Ok(runtime_identity(
                        OrganizationId::new(organization_id.get() + 1).unwrap(),
                    )),
                    FakeOutcome::Invalid => Err(
                        OrganizationServiceAccountRuntimeAuthenticationError::InvalidCredentials,
                    ),
                    FakeOutcome::Internal => {
                        Err(OrganizationServiceAccountRuntimeAuthenticationError::Internal)
                    }
                }
            })
        }
    }

    fn runtime_identity(
        organization_id: OrganizationId,
    ) -> OrganizationServiceAccountRuntimeIdentity {
        OrganizationServiceAccountRuntimeIdentity::new(
            OrganizationServiceAccountLocator::new(
                organization_id,
                OrganizationServiceAccountId::new(9).unwrap(),
            ),
            OrganizationServiceAccountKey::from_persistence_key("11111111111111111111111111111111")
                .unwrap(),
            OrganizationServiceAccountKey::from_persistence_key("22222222222222222222222222222222")
                .unwrap(),
        )
    }

    fn request(organization_id: i64, authorization: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().method("POST").uri(format!(
            "/api/organizations/{organization_id}/service-accounts/runtime/authenticate"
        ));
        if let Some(value) = authorization {
            builder = builder.header(AUTHORIZATION, value);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn bearer_extraction_requires_one_header_and_removes_the_secret() {
        let mut valid = request(7, Some(&format!("bEaReR {SECRET}")));
        assert_eq!(extract_bearer_secret(&mut valid).unwrap(), SECRET);
        assert!(!valid.headers().contains_key(AUTHORIZATION));

        let mut duplicate = request(7, Some(&format!("Bearer {SECRET}")));
        duplicate
            .headers_mut()
            .append(AUTHORIZATION, HeaderValue::from_static(SECRET));
        assert!(matches!(
            extract_bearer_secret(&mut duplicate),
            Err(RuntimeAuthenticationError::Unauthorized)
        ));
    }

    #[tokio::test]
    async fn authenticates_fixed_organization_and_returns_no_content() {
        let authenticator = Arc::new(FakeAuthenticator::new(FakeOutcome::Success));
        let response = build_organization_service_account_runtime_router(authenticator.clone())
            .oneshot(request(7, Some(&format!("Bearer {SECRET}"))))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        assert_eq!(
            *authenticator.observed_organization.lock().unwrap(),
            Some(OrganizationId::new(7).unwrap())
        );
    }

    #[tokio::test]
    async fn rejects_missing_or_malformed_credentials_before_repository_access() {
        for authorization in [None, Some("Basic value"), Some("Bearer malformed")] {
            let authenticator = Arc::new(FakeAuthenticator::new(FakeOutcome::Success));
            let response = build_organization_service_account_runtime_router(authenticator.clone())
                .oneshot(request(7, authorization))
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
            assert!(response.headers().contains_key(WWW_AUTHENTICATE));
            assert!(!authenticator.called.load(Ordering::SeqCst));
        }
    }

    #[tokio::test]
    async fn closes_cross_organization_and_internal_failures_without_secret_disclosure() {
        for (outcome, expected_status) in [
            (FakeOutcome::CrossOrganization, StatusCode::UNAUTHORIZED),
            (FakeOutcome::Invalid, StatusCode::UNAUTHORIZED),
            (FakeOutcome::Internal, StatusCode::SERVICE_UNAVAILABLE),
        ] {
            let response = build_organization_service_account_runtime_router(Arc::new(
                FakeAuthenticator::new(outcome),
            ))
            .oneshot(request(8, Some(&format!("Bearer {SECRET}"))))
            .await
            .unwrap();
            assert_eq!(response.status(), expected_status);
            assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
            let body = to_bytes(response.into_body(), 4096).await.unwrap();
            let body = String::from_utf8(body.to_vec()).unwrap();
            assert!(!body.contains(SECRET));
            assert!(!body.contains("digest"));
            assert!(!body.contains("service_account_id"));
        }
    }
}
