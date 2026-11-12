use axum::{
    Json,
    response::{IntoResponse, Response},
};
use http::{
    HeaderValue, StatusCode,
    header::{CACHE_CONTROL, RETRY_AFTER, WWW_AUTHENTICATE},
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub(crate) struct PersonalAccessTokenErrorBody {
    pub code: &'static str,
    pub message: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AccessTokenHttpError {
    InvalidRequest,
    InvalidToken,
    InsufficientScope,
    InteractiveSessionRequired,
    ProofRequired,
    RecentAuthenticationRequired,
    Forbidden,
    NotFound,
    Conflict,
    LimitReached,
    Disabled,
    IssuanceDisabled,
    RateLimited { retry_after: u32 },
    Unavailable,
}

impl IntoResponse for AccessTokenHttpError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::InvalidRequest => (
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "Invalid authentication request",
            ),
            Self::InvalidToken => (
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "Access token is invalid or expired",
            ),
            Self::InsufficientScope => (
                StatusCode::FORBIDDEN,
                "insufficient_scope",
                "Access token lacks a required permission",
            ),
            Self::InteractiveSessionRequired => (
                StatusCode::FORBIDDEN,
                "interactive_session_required",
                "An interactive login session is required",
            ),
            Self::ProofRequired => (
                StatusCode::FORBIDDEN,
                "access_token_step_up_required",
                "A fresh interactive verification is required",
            ),
            Self::RecentAuthenticationRequired => (
                StatusCode::FORBIDDEN,
                "recent_authentication_required",
                "Sign in again using an enabled authentication method",
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "access_token_forbidden",
                "Access denied",
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "access_token_not_found",
                "Access token not found",
            ),
            Self::Conflict => (
                StatusCode::CONFLICT,
                "access_token_conflict",
                "Access token was changed; refresh and retry",
            ),
            Self::LimitReached => (
                StatusCode::CONFLICT,
                "access_token_limit_reached",
                "Access token capacity has been reached",
            ),
            Self::Disabled => (
                StatusCode::SERVICE_UNAVAILABLE,
                "access_tokens_disabled",
                "Access token authentication is disabled",
            ),
            Self::IssuanceDisabled => (
                StatusCode::SERVICE_UNAVAILABLE,
                "access_token_issuance_disabled",
                "Access token issuance is disabled",
            ),
            Self::RateLimited { .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                "access_token_rate_limited",
                "Access token request rate limit exceeded",
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "access_token_unavailable",
                "Access token authentication is unavailable",
            ),
        };
        let mut response =
            (status, Json(PersonalAccessTokenErrorBody { code, message })).into_response();
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        let challenge = match self {
            Self::InvalidRequest => {
                Some("Bearer realm=\"anyflows-system-api\", error=\"invalid_request\"")
            }
            Self::InvalidToken => {
                Some("Bearer realm=\"anyflows-system-api\", error=\"invalid_token\"")
            }
            Self::InsufficientScope => {
                Some("Bearer realm=\"anyflows-system-api\", error=\"insufficient_scope\"")
            }
            _ => None,
        };
        if let Some(challenge) = challenge {
            response
                .headers_mut()
                .insert(WWW_AUTHENTICATE, HeaderValue::from_static(challenge));
        }
        if let Self::RateLimited { retry_after } = self
            && let Ok(header) = HeaderValue::from_str(&retry_after.to_string())
        {
            response.headers_mut().insert(RETRY_AFTER, header);
        }
        response
    }
}

impl From<af_admin::PersonalAccessTokenError> for AccessTokenHttpError {
    fn from(error: af_admin::PersonalAccessTokenError) -> Self {
        use af_admin::PersonalAccessTokenError as Error;
        match error {
            Error::InvalidInput => Self::InvalidRequest,
            Error::InvalidToken => Self::InvalidToken,
            Error::Forbidden => Self::Forbidden,
            Error::NotFound => Self::NotFound,
            Error::Conflict => Self::Conflict,
            Error::LimitReached => Self::LimitReached,
            Error::Disabled => Self::Disabled,
            Error::IssuanceDisabled => Self::IssuanceDisabled,
            Error::InvalidProof => Self::ProofRequired,
            Error::RateLimited { retry_after } => Self::RateLimited { retry_after },
            Error::Internal => Self::Unavailable,
        }
    }
}

pub(crate) fn insufficient_scope(scopes: &[String]) -> Response {
    let mut response = AccessTokenHttpError::InsufficientScope.into_response();
    let challenge = format!(
        "Bearer realm=\"anyflows-system-api\", error=\"insufficient_scope\", scope=\"{}\"",
        scopes.join(" ")
    );
    if let Ok(header) = HeaderValue::from_str(&challenge) {
        response.headers_mut().insert(WWW_AUTHENTICATE, header);
    }
    response
}
