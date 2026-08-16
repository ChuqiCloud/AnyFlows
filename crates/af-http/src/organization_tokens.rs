use std::sync::Arc;

use af_admin::{
    IssuedOrganizationToken, OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationContext, OrganizationToken, OrganizationTokenError, OrganizationTokenListQuery,
    OrganizationTokenPage, OrganizationTokenService, OrganizationTokenStatus,
    OrganizationTokenWriteCommand, SessionAuthentication, SessionAuthenticator,
};
use af_domain::{OrganizationId, OrganizationPermission, TokenId};
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_tokens::parse_token_id,
};

#[derive(Clone)]
struct OrganizationTokenHttpState {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    token_service: Arc<dyn OrganizationTokenService>,
}

/// 构建企业 API Key 路由；组织范围与权限在每个请求中重新解析。
pub fn build_organization_token_router(
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    token_service: Arc<dyn OrganizationTokenService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/organizations/{organization_id}/tokens",
            get(list_self_tokens).post(create_token),
        )
        .route(
            "/api/organizations/{organization_id}/tokens/directory",
            get(list_all_tokens),
        )
        .route(
            "/api/organizations/{organization_id}/tokens/{token_id}",
            get(get_self_token)
                .put(update_self_token)
                .delete(disable_self_token),
        )
        .route(
            "/api/organizations/{organization_id}/tokens/{token_id}/disable",
            post(disable_any_token),
        )
        .layer(DefaultBodyLimit::max(96 * 1024))
        .layer(RequestBodyLimitLayer::new(96 * 1024))
        .route_layer(authentication)
        .with_state(OrganizationTokenHttpState {
            authorization_service,
            token_service,
        })
}

#[derive(Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OrganizationTokenStatusDto {
    Enabled,
    Disabled,
}

impl From<OrganizationTokenStatus> for OrganizationTokenStatusDto {
    fn from(value: OrganizationTokenStatus) -> Self {
        match value {
            OrganizationTokenStatus::Enabled => Self::Enabled,
            OrganizationTokenStatus::Disabled => Self::Disabled,
        }
    }
}

impl From<OrganizationTokenStatusDto> for OrganizationTokenStatus {
    fn from(value: OrganizationTokenStatusDto) -> Self {
        match value {
            OrganizationTokenStatusDto::Enabled => Self::Enabled,
            OrganizationTokenStatusDto::Disabled => Self::Disabled,
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationTokenWriteRequest {
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    status: OrganizationTokenStatusDto,
    #[schema(minimum = 0)]
    remain_quota: i64,
    unlimited_quota: bool,
    #[schema(minimum = 0, required = true)]
    expired_at: Option<i64>,
    #[schema(
        schema_with = crate::openapi::schema::token_model_limits_write_schema,
        required = true
    )]
    model_limits: Option<Vec<String>>,
    #[schema(
        schema_with = crate::openapi::schema::token_allow_ips_write_schema,
        required = true
    )]
    allow_ips: Option<Vec<String>>,
    #[schema(minimum = 0, required = true)]
    rate_limit_5h: Option<i64>,
    #[schema(minimum = 0, required = true)]
    rate_limit_1d: Option<i64>,
    #[schema(minimum = 0, required = true)]
    rate_limit_7d: Option<i64>,
    #[schema(minimum = 0, required = true)]
    max_requests: Option<i64>,
}

impl OrganizationTokenWriteRequest {
    fn into_command(self) -> Result<OrganizationTokenWriteCommand, ManagementError> {
        OrganizationTokenWriteCommand::new(
            self.name,
            self.status.into(),
            self.remain_quota,
            self.unlimited_quota,
            self.expired_at,
            self.model_limits,
            self.allow_ips,
            self.rate_limit_5h,
            self.rate_limit_1d,
            self.rate_limit_7d,
            self.max_requests,
        )
        .map_err(|_| ManagementError::InvalidRequest)
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationTokenResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(minimum = 1)]
    owner_user_id: i64,
    #[schema(required = true, minimum = 1)]
    team_id: Option<i64>,
    #[schema(
        min_length = 18,
        max_length = 18,
        pattern = "^sk-af-[A-Za-z0-9_-]{12}$"
    )]
    key_prefix: String,
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    status: OrganizationTokenStatusDto,
    #[schema(minimum = 0)]
    remain_quota: i64,
    unlimited_quota: bool,
    #[schema(minimum = 0)]
    used_quota: i64,
    #[schema(minimum = 0, required = true)]
    expired_at: Option<i64>,
    #[schema(schema_with = crate::openapi::schema::token_model_limits_schema, required = true)]
    model_limits: Option<Vec<String>>,
    #[schema(schema_with = crate::openapi::schema::token_allow_ips_schema, required = true)]
    allow_ips: Option<Vec<String>>,
    #[schema(minimum = 0, required = true)]
    rate_limit_5h: Option<i64>,
    #[schema(minimum = 0, required = true)]
    rate_limit_1d: Option<i64>,
    #[schema(minimum = 0, required = true)]
    rate_limit_7d: Option<i64>,
    #[schema(minimum = 0)]
    usage_5h: i64,
    #[schema(minimum = 0)]
    usage_1d: i64,
    #[schema(minimum = 0)]
    usage_7d: i64,
    #[schema(minimum = 0, required = true)]
    max_requests: Option<i64>,
    #[schema(minimum = 0)]
    used_requests: i64,
    #[schema(minimum = 0)]
    created_at: i64,
    #[schema(minimum = 0)]
    updated_at: i64,
}

impl OrganizationTokenResponse {
    fn from_application(token: &OrganizationToken) -> Self {
        Self {
            id: token.token_id().get(),
            owner_user_id: token.owner_user_id().get(),
            team_id: token.team_id().map(|team| team.get()),
            key_prefix: token.key_prefix().to_owned(),
            name: token.name().to_owned(),
            status: token.status().into(),
            remain_quota: token.remain_quota(),
            unlimited_quota: token.unlimited_quota(),
            used_quota: token.used_quota(),
            expired_at: token.expired_at(),
            model_limits: token.model_limits().map(<[String]>::to_vec),
            allow_ips: token.allow_ips().map(<[String]>::to_vec),
            rate_limit_5h: token.rate_limit_5h(),
            rate_limit_1d: token.rate_limit_1d(),
            rate_limit_7d: token.rate_limit_7d(),
            usage_5h: token.usage_5h(),
            usage_1d: token.usage_1d(),
            usage_7d: token.usage_7d(),
            max_requests: token.max_requests(),
            used_requests: token.used_requests(),
            created_at: token.created_at(),
            updated_at: token.updated_at(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationTokenListResponse {
    #[schema(max_items = 100)]
    tokens: Vec<OrganizationTokenResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

impl OrganizationTokenListResponse {
    fn from_application(page: OrganizationTokenPage) -> Self {
        Self {
            tokens: page
                .tokens()
                .iter()
                .map(OrganizationTokenResponse::from_application)
                .collect(),
            next_cursor: page.next_cursor().map(TokenId::get),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssuedOrganizationTokenResponse<'a> {
    #[schema(
        min_length = 49,
        max_length = 49,
        pattern = "^sk-af-[A-Za-z0-9_-]{43}$"
    )]
    api_key: &'a str,
    token: OrganizationTokenResponse,
}

impl<'a> IssuedOrganizationTokenResponse<'a> {
    fn from_application(issued: &'a IssuedOrganizationToken) -> Self {
        Self {
            api_key: issued.api_key().expose_secret(),
            token: OrganizationTokenResponse::from_application(issued.token()),
        }
    }
}

async fn list_self_tokens(
    State(state): State<OrganizationTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::KeysManageSelf,
    )
    .await?;
    let page = state
        .token_service
        .list_self(context, parse_list_query(raw_query.as_deref())?)
        .await
        .map_err(map_token_error)?;
    Ok(no_store_json(
        StatusCode::OK,
        OrganizationTokenListResponse::from_application(page),
    ))
}

async fn list_all_tokens(
    State(state): State<OrganizationTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::KeysReadAll,
    )
    .await?;
    let page = state
        .token_service
        .list_all(context, parse_list_query(raw_query.as_deref())?)
        .await
        .map_err(map_token_error)?;
    Ok(no_store_json(
        StatusCode::OK,
        OrganizationTokenListResponse::from_application(page),
    ))
}

async fn get_self_token(
    State(state): State<OrganizationTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, token_id)): Path<(i64, String)>,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::KeysManageSelf,
    )
    .await?;
    let token = state
        .token_service
        .get_self(context, parse_token_id(&token_id)?)
        .await
        .map_err(map_token_error)?;
    Ok(no_store_json(
        StatusCode::OK,
        OrganizationTokenResponse::from_application(&token),
    ))
}

async fn create_token(
    State(state): State<OrganizationTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    request: Result<Json<OrganizationTokenWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::KeysManageSelf,
    )
    .await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let issued = state
        .token_service
        .create(context, request.into_command()?)
        .await
        .map_err(map_token_error)?;
    Ok(no_store_json(
        StatusCode::CREATED,
        IssuedOrganizationTokenResponse::from_application(&issued),
    ))
}

async fn update_self_token(
    State(state): State<OrganizationTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, token_id)): Path<(i64, String)>,
    request: Result<Json<OrganizationTokenWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::KeysManageSelf,
    )
    .await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let token = state
        .token_service
        .update_self(context, parse_token_id(&token_id)?, request.into_command()?)
        .await
        .map_err(map_token_error)?;
    Ok(no_store_json(
        StatusCode::OK,
        OrganizationTokenResponse::from_application(&token),
    ))
}

async fn disable_self_token(
    State(state): State<OrganizationTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, token_id)): Path<(i64, String)>,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::KeysManageSelf,
    )
    .await?;
    let token = state
        .token_service
        .disable_self(context, parse_token_id(&token_id)?)
        .await
        .map_err(map_token_error)?;
    Ok(no_store_json(
        StatusCode::OK,
        OrganizationTokenResponse::from_application(&token),
    ))
}

async fn disable_any_token(
    State(state): State<OrganizationTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, token_id)): Path<(i64, String)>,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::KeysReadAll,
    )
    .await?;
    let token = state
        .token_service
        .disable_any(context, parse_token_id(&token_id)?)
        .await
        .map_err(map_token_error)?;
    Ok(no_store_json(
        StatusCode::OK,
        OrganizationTokenResponse::from_application(&token),
    ))
}

async fn authorize(
    state: &OrganizationTokenHttpState,
    authentication: &SessionAuthentication,
    organization_id: i64,
    permission: OrganizationPermission,
) -> Result<OrganizationContext, ManagementError> {
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    state
        .authorization_service
        .context(authentication.principal(), organization_id, permission)
        .await
        .map_err(map_authorization_error)
}

fn parse_list_query(
    raw_query: Option<&str>,
) -> Result<OrganizationTokenListQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(OrganizationTokenListQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(OrganizationTokenListQuery::default());
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
    OrganizationTokenListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_ORGANIZATION_TOKEN_PAGE_SIZE),
    )
    .map_err(map_token_error)
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

fn map_authorization_error(error: OrganizationAuthorizationError) -> ManagementError {
    match error {
        OrganizationAuthorizationError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationAuthorizationError::Forbidden => ManagementError::Forbidden,
        OrganizationAuthorizationError::Internal => ManagementError::Internal,
    }
}

fn map_token_error(error: OrganizationTokenError) -> ManagementError {
    match error {
        OrganizationTokenError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationTokenError::Forbidden => ManagementError::Forbidden,
        OrganizationTokenError::NotFound => ManagementError::TokenNotFound,
        OrganizationTokenError::LimitReached => ManagementError::TokenLimitReached,
        OrganizationTokenError::OutcomeUnknown => ManagementError::TokenOutcomeUnknown,
        OrganizationTokenError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
