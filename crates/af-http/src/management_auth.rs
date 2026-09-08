use std::sync::Arc;

use af_admin::{SessionAuthenticationError, SessionAuthenticator, SessionToken};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use http::header::AUTHORIZATION;

use crate::management_error::ManagementError;

/// 管理 API JWT 中间件使用的认证器状态。
#[derive(Clone)]
pub(crate) struct ManagementAuthenticationState {
    authenticator: Arc<dyn SessionAuthenticator>,
}

impl ManagementAuthenticationState {
    /// 绑定启动期装配的会话认证器。
    pub(crate) fn new(authenticator: Arc<dyn SessionAuthenticator>) -> Self {
        Self { authenticator }
    }
}

/// 从唯一 Bearer Header 提取 JWT，校验后把当前主体注入请求扩展。
pub(crate) async fn authenticate_management_session(
    State(state): State<ManagementAuthenticationState>,
    request: Request,
    next: Next,
) -> Response {
    authenticate_present_session(state, request, next).await
}

/// 没有 Authorization 时按游客放行，携带后仍执行完整会话认证。
pub(crate) async fn authenticate_optional_management_session(
    State(state): State<ManagementAuthenticationState>,
    request: Request,
    next: Next,
) -> Response {
    if !request.headers().contains_key(AUTHORIZATION) {
        return next.run(request).await;
    }
    authenticate_present_session(state, request, next).await
}

async fn authenticate_present_session(
    state: ManagementAuthenticationState,
    mut request: Request,
    next: Next,
) -> Response {
    let token = match extract_bearer_token(&mut request) {
        Ok(token) => token,
        Err(error) => return error.into_response(),
    };
    match state.authenticator.authenticate(token.as_str()).await {
        Ok(authentication) => {
            request.extensions_mut().insert(authentication);
            next.run(request).await
        }
        Err(SessionAuthenticationError::InvalidSession) => {
            ManagementError::InvalidSession.into_response()
        }
        Err(SessionAuthenticationError::InvalidCredentials) => {
            ManagementError::InvalidSession.into_response()
        }
        Err(SessionAuthenticationError::TwoFactorRequired)
        | Err(SessionAuthenticationError::TwoFactorInvalid) => {
            ManagementError::InvalidSession.into_response()
        }
        Err(SessionAuthenticationError::Internal) => ManagementError::Internal.into_response(),
    }
}

fn extract_bearer_token(request: &mut Request) -> Result<SessionToken, ManagementError> {
    let values = request.headers().get_all(AUTHORIZATION);
    let mut iter = values.iter();
    let Some(value) = iter.next() else {
        return Err(ManagementError::InvalidSession);
    };
    if iter.next().is_some() {
        return Err(ManagementError::InvalidSession);
    }
    let raw = value
        .to_str()
        .map_err(|_| ManagementError::InvalidSession)?;
    let Some((scheme, token)) = raw.split_once(' ') else {
        return Err(ManagementError::InvalidSession);
    };
    if !scheme.eq_ignore_ascii_case("Bearer")
        || token.is_empty()
        || token.chars().any(char::is_whitespace)
    {
        return Err(ManagementError::InvalidSession);
    }
    let token = token.to_owned();
    request.headers_mut().remove(AUTHORIZATION);
    Ok(SessionToken::from_string(token))
}

use axum::response::IntoResponse;

#[cfg(test)]
mod tests {
    use af_admin::{LoginCredentials, SessionAuthenticationFuture, SessionLoginFuture};
    use axum::{Router, body::Body, middleware, routing::get};
    use http::{HeaderValue, Request, StatusCode, header::AUTHORIZATION};
    use tower::ServiceExt;

    use super::*;

    struct RejectSessionAuthenticator;

    impl SessionAuthenticator for RejectSessionAuthenticator {
        fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
            Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
        }

        fn authenticate<'a>(&'a self, _token: &'a str) -> SessionAuthenticationFuture<'a> {
            Box::pin(async { Err(SessionAuthenticationError::InvalidSession) })
        }
    }

    #[test]
    fn bearer_extraction_requires_one_header_and_removes_it() {
        let mut request = Request::builder()
            .header(AUTHORIZATION, "bEaReR jwt-token")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            extract_bearer_token(&mut request).unwrap().as_str(),
            "jwt-token"
        );
        assert!(!request.headers().contains_key(AUTHORIZATION));

        for value in [
            "Bearer",
            "Basic jwt-token",
            "Bearer ",
            "Bearer jwt token",
            " Bearer jwt",
        ] {
            let mut request = Request::builder()
                .header(AUTHORIZATION, value)
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                extract_bearer_token(&mut request).unwrap_err(),
                ManagementError::InvalidSession
            );
        }

        let mut duplicate = Request::builder()
            .header(AUTHORIZATION, HeaderValue::from_static("Bearer first"))
            .header(AUTHORIZATION, HeaderValue::from_static("Bearer second"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            extract_bearer_token(&mut duplicate).unwrap_err(),
            ManagementError::InvalidSession
        );
    }

    #[tokio::test]
    async fn optional_authentication_allows_guests_but_rejects_present_invalid_tokens() {
        let layer = middleware::from_fn_with_state(
            ManagementAuthenticationState::new(Arc::new(RejectSessionAuthenticator)),
            authenticate_optional_management_session,
        );
        let router = Router::new()
            .route("/", get(|| async { StatusCode::NO_CONTENT }))
            .layer(layer);

        let guest = router
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        assert_eq!(guest.status(), StatusCode::NO_CONTENT);

        let invalid = router
            .oneshot(
                Request::builder()
                    .header(AUTHORIZATION, "Bearer invalid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);
    }
}
