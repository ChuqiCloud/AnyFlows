use std::sync::Arc;

use af_admin::{
    SessionAuthentication, SessionAuthenticator, UserTokenError, UserTokenListQuery,
    UserTokenService,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use tower_http::limit::RequestBodyLimitLayer;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
    management_tokens::parse_token_id,
};

mod dto;

pub(crate) use dto::{
    IssuedUserTokenResponse, UserTokenListResponse, UserTokenResponse, UserTokenWriteRequest,
};

/// 用户 API Key 路由独立持有的应用服务状态。
#[derive(Clone)]
pub(crate) struct UserTokenHttpState {
    service: Arc<dyn UserTokenService>,
}

impl UserTokenHttpState {
    /// 绑定启动期装配的用户 Key 应用服务。
    pub(crate) fn new(service: Arc<dyn UserTokenService>) -> Self {
        Self { service }
    }
}

/// 构建只要求有效登录会话的用户 API Key 路由。
pub(crate) fn build_user_token_router(
    service: Arc<dyn UserTokenService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/tokens", get(list_tokens).post(create_token))
        .route(
            "/api/tokens/{id}",
            get(get_token).put(update_token).delete(delete_token),
        )
        // 白名单 JSON 需要容纳 68 KiB 持久化边界，但不使用全局 32 MiB 预算。
        .layer(DefaultBodyLimit::max(96 * 1024))
        .layer(RequestBodyLimitLayer::new(96 * 1024))
        .layer(authentication)
        .with_state(UserTokenHttpState::new(service))
}

/// 列出当前登录用户拥有的一页 API Key。
pub(crate) async fn list_tokens(
    State(state): State<UserTokenHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query.as_deref())?;
    let page = state
        .service
        .list(authentication.principal(), query)
        .await
        .map_err(map_user_token_error)?;
    Ok(no_store_json(UserTokenListResponse::from_application(page)))
}

/// 读取当前登录用户拥有的一个 API Key。
pub(crate) async fn get_token(
    State(state): State<UserTokenHttpState>,
    Path(token_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let token = state
        .service
        .get(authentication.principal(), parse_token_id(&token_id)?)
        .await
        .map_err(map_user_token_error)?;
    Ok(no_store_json(UserTokenResponse::from_application(&token)))
}

/// 为当前登录用户签发 API Key，完整明文只返回一次。
pub(crate) async fn create_token(
    State(state): State<UserTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserTokenWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let request = extract_write_request(request)?;
    let issued = state
        .service
        .create(authentication.principal(), request.into_command()?)
        .await
        .map_err(map_user_token_error)?;
    let mut response = no_store_json(IssuedUserTokenResponse::from_application(&issued));
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}

/// 更新当前登录用户可控字段，不覆盖管理员配置。
pub(crate) async fn update_token(
    State(state): State<UserTokenHttpState>,
    Path(token_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserTokenWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let token_id = parse_token_id(&token_id)?;
    let request = extract_write_request(request)?;
    let token = state
        .service
        .update(
            authentication.principal(),
            token_id,
            request.into_command()?,
        )
        .await
        .map_err(map_user_token_error)?;
    Ok(no_store_json(UserTokenResponse::from_application(&token)))
}

/// 软删除当前登录用户拥有的 API Key。
pub(crate) async fn delete_token(
    State(state): State<UserTokenHttpState>,
    Path(token_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    state
        .service
        .delete(authentication.principal(), parse_token_id(&token_id)?)
        .await
        .map_err(map_user_token_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn extract_write_request(
    request: Result<Json<UserTokenWriteRequest>, JsonRejection>,
) -> Result<UserTokenWriteRequest, ManagementError> {
    match request {
        Ok(Json(request)) => Ok(request),
        Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            Err(ManagementError::InvalidRequest)
        }
        Err(_) => Err(ManagementError::InvalidRequest),
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<UserTokenListQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(UserTokenListQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(UserTokenListQuery::default());
    }
    validate_percent_encoding(raw_query)?;
    let mut seen_after = false;
    let mut seen_limit = false;
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "after" if !seen_after => {
                seen_after = true;
                after = Some(parse_token_id(&value)?);
            }
            "limit" if !seen_limit => {
                seen_limit = true;
                limit = Some(parse_limit(&value)?);
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    UserTokenListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_USER_TOKEN_PAGE_SIZE),
    )
    .map_err(map_user_token_error)
}

fn parse_limit(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty()
        || value.starts_with('+')
        || value.starts_with('-')
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<usize>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn validate_percent_encoding(raw_query: &str) -> Result<(), ManagementError> {
    let bytes = raw_query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ManagementError::InvalidRequest);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn map_user_token_error(error: UserTokenError) -> ManagementError {
    match error {
        UserTokenError::InvalidInput => ManagementError::InvalidRequest,
        UserTokenError::InvalidSession => ManagementError::InvalidSession,
        UserTokenError::LimitReached => ManagementError::TokenLimitReached,
        UserTokenError::NotFound => ManagementError::TokenNotFound,
        UserTokenError::Internal => ManagementError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_parser_rejects_duplicate_unknown_and_malformed_values() {
        assert_eq!(parse_list_query(None).unwrap().limit(), 50);
        assert_eq!(
            parse_list_query(Some("after=1&limit=100")).unwrap().limit(),
            100
        );
        for raw in [
            "after=0",
            "after=1&after=2",
            "limit=0",
            "limit=101",
            "unknown=1",
            "after=%",
            "after=%ff",
        ] {
            assert_eq!(
                parse_list_query(Some(raw)),
                Err(ManagementError::InvalidRequest),
                "{raw}"
            );
        }
    }
}
