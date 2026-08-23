use std::sync::Arc;

use af_admin::{
    AdminOAuthLoginProviderSettings, AdminOAuthLoginProviderSettingsCommand, OAuthLoginError,
    OAuthLoginService, SessionAuthenticator,
};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{
    HeaderMap, HeaderName, HeaderValue, StatusCode,
    header::{CACHE_CONTROL, COOKIE, LOCATION, SET_COOKIE},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use url::Url;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::{issue_session_for_user, no_store_json},
};

const GITHUB_PROVIDER: &str = "github";
const DISCORD_PROVIDER: &str = "discord";
const OIDC_PROVIDER: &str = "oidc";
const LINUXDO_PROVIDER: &str = "linuxdo";
const WECHAT_PROVIDER: &str = "wechat";
const TELEGRAM_PROVIDER: &str = "telegram";
const GOOGLE_PROVIDER: &str = "google";
const OAUTH_STATE_COOKIE: &str = "af_oauth_login_state";
const GITHUB_CALLBACK_PATH: &str = "/api/auth/oauth/github/callback";
const DISCORD_CALLBACK_PATH: &str = "/api/auth/oauth/discord/callback";
const OIDC_CALLBACK_PATH: &str = "/api/auth/oauth/oidc/callback";
const LINUXDO_CALLBACK_PATH: &str = "/api/auth/oauth/linuxdo/callback";
const WECHAT_CALLBACK_PATH: &str = "/api/auth/oauth/wechat/callback";
const TELEGRAM_CALLBACK_PATH: &str = "/api/auth/oauth/telegram/callback";
const GOOGLE_CALLBACK_PATH: &str = "/api/auth/oauth/google/callback";
const OAUTH_STATE_COOKIE_MAX_AGE_SECONDS: u64 = 10 * 60;
const OAUTH_STATE_COOKIE_DOMAIN: &str = "oauth-login-browser-state-v1";

#[derive(Clone)]
struct OAuthLoginHttpState {
    service: Arc<dyn OAuthLoginService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OAuthLoginStartResponse)]
pub(crate) struct OAuthLoginStartResponse {
    #[schema(format = "uri", max_length = 4096)]
    authorization_url: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OAuthLoginExchangeRequest)]
pub(crate) struct OAuthLoginExchangeRequest {
    #[schema(pattern = r"^[A-Za-z0-9_-]{43}$")]
    ticket: String,
}

struct OAuthLoginCallbackQuery {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminOAuthLoginProviderSettings)]
pub(crate) struct AdminOAuthLoginProviderSettingsResponse {
    enabled: bool,
    #[schema(max_length = 255)]
    client_id: Option<String>,
    #[schema(format = "uri", max_length = 2048)]
    issuer_url: Option<String>,
    client_secret_configured: bool,
    #[schema(format = "uri", max_length = 4096)]
    callback_url: Option<String>,
    #[schema(minimum = 1)]
    version: i64,
}

impl From<AdminOAuthLoginProviderSettings> for AdminOAuthLoginProviderSettingsResponse {
    fn from(settings: AdminOAuthLoginProviderSettings) -> Self {
        Self {
            enabled: settings.enabled(),
            client_id: settings.client_id().map(str::to_owned),
            issuer_url: settings.issuer_url().map(str::to_owned),
            client_secret_configured: settings.client_secret_configured(),
            callback_url: settings.callback_url().map(str::to_owned),
            version: settings.version(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminOAuthLoginProviderSettingsRequest)]
pub(crate) struct AdminOAuthLoginProviderSettingsRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
    enabled: bool,
    #[schema(max_length = 255)]
    client_id: Option<String>,
    #[schema(format = "uri", max_length = 2048)]
    issuer_url: Option<String>,
    #[schema(max_length = 4096, format = Password)]
    client_secret: Option<String>,
    #[serde(default)]
    clear_client_secret: bool,
}

/// 构建内置用户登录 OAuth 的公开、回调和管理员设置路由。
pub(crate) fn build_oauth_login_router(
    service: Arc<dyn OAuthLoginService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/auth/oauth/github/start", post(start_github_login))
        .route(
            "/api/auth/oauth/github/callback",
            get(complete_github_login),
        )
        .route("/api/auth/oauth/discord/start", post(start_discord_login))
        .route(
            "/api/auth/oauth/discord/callback",
            get(complete_discord_login),
        )
        .route("/api/auth/oauth/oidc/start", post(start_oidc_login))
        .route("/api/auth/oauth/oidc/callback", get(complete_oidc_login))
        .route("/api/auth/oauth/linuxdo/start", post(start_linuxdo_login))
        .route(
            "/api/auth/oauth/linuxdo/callback",
            get(complete_linuxdo_login),
        )
        .route("/api/auth/oauth/wechat/start", post(start_wechat_login))
        .route(
            "/api/auth/oauth/wechat/callback",
            get(complete_wechat_login),
        )
        .route("/api/auth/oauth/telegram/start", post(start_telegram_login))
        .route(
            "/api/auth/oauth/telegram/callback",
            get(complete_telegram_login),
        )
        .route("/api/auth/oauth/google/start", post(start_google_login))
        .route(
            "/api/auth/oauth/google/callback",
            get(complete_google_login),
        )
        .route(
            "/api/auth/oauth/custom/{provider_key}/start",
            post(start_custom_oauth2_login),
        )
        .route(
            "/api/auth/oauth/custom/{provider_key}/callback",
            get(complete_custom_oauth2_login),
        )
        .route("/api/auth/oauth/exchange", post(exchange_oauth_ticket))
        .route(
            "/api/admin/authentication-settings/oauth/github",
            get(get_admin_github_settings)
                .put(update_admin_github_settings)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(authentication),
        )
        .route(
            "/api/admin/authentication-settings/oauth/discord",
            get(get_admin_discord_settings)
                .put(update_admin_discord_settings)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(middleware::from_fn_with_state(
                    ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
                    authenticate_management_session,
                )),
        )
        .route(
            "/api/admin/authentication-settings/oauth/oidc",
            get(get_admin_oidc_settings)
                .put(update_admin_oidc_settings)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(middleware::from_fn_with_state(
                    ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
                    authenticate_management_session,
                )),
        )
        .route(
            "/api/admin/authentication-settings/oauth/linuxdo",
            get(get_admin_linuxdo_settings)
                .put(update_admin_linuxdo_settings)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(middleware::from_fn_with_state(
                    ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
                    authenticate_management_session,
                )),
        )
        .route(
            "/api/admin/authentication-settings/oauth/wechat",
            get(get_admin_wechat_settings)
                .put(update_admin_wechat_settings)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(middleware::from_fn_with_state(
                    ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
                    authenticate_management_session,
                )),
        )
        .route(
            "/api/admin/authentication-settings/oauth/telegram",
            get(get_admin_telegram_settings)
                .put(update_admin_telegram_settings)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(middleware::from_fn_with_state(
                    ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
                    authenticate_management_session,
                )),
        )
        .route(
            "/api/admin/authentication-settings/oauth/google",
            get(get_admin_google_settings)
                .put(update_admin_google_settings)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(middleware::from_fn_with_state(
                    ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
                    authenticate_management_session,
                )),
        )
        .with_state(OAuthLoginHttpState {
            service,
            session_authenticator,
        })
}

async fn start_github_login(
    State(state): State<OAuthLoginHttpState>,
) -> Result<Response, ManagementError> {
    start_login(state, GITHUB_PROVIDER, GITHUB_CALLBACK_PATH).await
}

async fn start_discord_login(
    State(state): State<OAuthLoginHttpState>,
) -> Result<Response, ManagementError> {
    start_login(state, DISCORD_PROVIDER, DISCORD_CALLBACK_PATH).await
}

async fn start_oidc_login(
    State(state): State<OAuthLoginHttpState>,
) -> Result<Response, ManagementError> {
    start_login(state, OIDC_PROVIDER, OIDC_CALLBACK_PATH).await
}

async fn start_linuxdo_login(
    State(state): State<OAuthLoginHttpState>,
) -> Result<Response, ManagementError> {
    start_login(state, LINUXDO_PROVIDER, LINUXDO_CALLBACK_PATH).await
}

async fn start_wechat_login(
    State(state): State<OAuthLoginHttpState>,
) -> Result<Response, ManagementError> {
    start_login(state, WECHAT_PROVIDER, WECHAT_CALLBACK_PATH).await
}

async fn start_telegram_login(
    State(state): State<OAuthLoginHttpState>,
) -> Result<Response, ManagementError> {
    start_login(state, TELEGRAM_PROVIDER, TELEGRAM_CALLBACK_PATH).await
}

async fn start_google_login(
    State(state): State<OAuthLoginHttpState>,
) -> Result<Response, ManagementError> {
    start_login(state, GOOGLE_PROVIDER, GOOGLE_CALLBACK_PATH).await
}

async fn start_custom_oauth2_login(
    State(state): State<OAuthLoginHttpState>,
    Path(provider_key): Path<String>,
) -> Result<Response, ManagementError> {
    let callback_path =
        custom_oauth2_callback_path(&provider_key).map_err(map_oauth_login_error)?;
    start_login(state, &provider_key, &callback_path).await
}

async fn start_login(
    state: OAuthLoginHttpState,
    provider: &str,
    callback_path: &str,
) -> Result<Response, ManagementError> {
    let started = state
        .service
        .begin(provider)
        .await
        .map_err(map_oauth_login_error)?;
    let (state_digest, secure) = oauth_state_cookie_material(started.authorization_url())?;
    let mut response = no_store_json(OAuthLoginStartResponse {
        authorization_url: started.authorization_url().to_owned(),
    });
    response.headers_mut().insert(
        SET_COOKIE,
        oauth_state_set_cookie(&state_digest, secure, callback_path)?,
    );
    Ok(response)
}

/// 外部回调始终使用 303 且禁止 Referer，避免 code/state 进入后续前端请求。
async fn complete_github_login(
    State(state): State<OAuthLoginHttpState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    complete_login(
        state,
        headers,
        raw_query,
        GITHUB_PROVIDER,
        GITHUB_CALLBACK_PATH,
    )
    .await
}

async fn complete_discord_login(
    State(state): State<OAuthLoginHttpState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    complete_login(
        state,
        headers,
        raw_query,
        DISCORD_PROVIDER,
        DISCORD_CALLBACK_PATH,
    )
    .await
}

async fn complete_oidc_login(
    State(state): State<OAuthLoginHttpState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    complete_login(state, headers, raw_query, OIDC_PROVIDER, OIDC_CALLBACK_PATH).await
}

async fn complete_linuxdo_login(
    State(state): State<OAuthLoginHttpState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    complete_login(
        state,
        headers,
        raw_query,
        LINUXDO_PROVIDER,
        LINUXDO_CALLBACK_PATH,
    )
    .await
}

async fn complete_wechat_login(
    State(state): State<OAuthLoginHttpState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    complete_login(
        state,
        headers,
        raw_query,
        WECHAT_PROVIDER,
        WECHAT_CALLBACK_PATH,
    )
    .await
}

async fn complete_telegram_login(
    State(state): State<OAuthLoginHttpState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    complete_login(
        state,
        headers,
        raw_query,
        TELEGRAM_PROVIDER,
        TELEGRAM_CALLBACK_PATH,
    )
    .await
}

async fn complete_google_login(
    State(state): State<OAuthLoginHttpState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    complete_login(
        state,
        headers,
        raw_query,
        GOOGLE_PROVIDER,
        GOOGLE_CALLBACK_PATH,
    )
    .await
}

async fn complete_custom_oauth2_login(
    State(state): State<OAuthLoginHttpState>,
    Path(provider_key): Path<String>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let callback_path = match custom_oauth2_callback_path(&provider_key) {
        Ok(callback_path) => callback_path,
        Err(error) => {
            let mut response = map_oauth_login_error(error).into_response();
            harden_oauth_callback_response(&mut response, "/api/auth/oauth/custom");
            return response;
        }
    };
    complete_login(state, headers, raw_query, &provider_key, &callback_path).await
}

async fn complete_login(
    state: OAuthLoginHttpState,
    headers: HeaderMap,
    raw_query: Option<String>,
    provider: &str,
    callback_path: &str,
) -> Response {
    let result = match parse_oauth_callback_query(raw_query.as_deref()) {
        Ok(query) => match (query.state, query.code, query.error) {
            (Some(state_value), Some(code), None)
                if oauth_state_cookie_matches(&headers, &state_value) =>
            {
                state.service.complete(provider, state_value, code).await
            }
            (Some(state_value), None, Some(error))
                if valid_provider_error(&error)
                    && oauth_state_cookie_matches(&headers, &state_value) =>
            {
                state.service.cancel(provider, state_value).await
            }
            _ => Err(OAuthLoginError::Rejected),
        },
        Err(_) => Err(OAuthLoginError::Rejected),
    };
    let callback = match result {
        Ok(callback) => Ok(callback),
        Err(_) => state.service.callback_failure().await,
    };
    let mut response = match callback {
        Ok(callback) => {
            redirect_no_store(callback.redirect_url()).unwrap_or_else(|error| error.into_response())
        }
        Err(error) => map_oauth_login_error(error).into_response(),
    };
    harden_oauth_callback_response(&mut response, callback_path);
    response
}

async fn exchange_oauth_ticket(
    State(state): State<OAuthLoginHttpState>,
    request: Result<Json<OAuthLoginExchangeRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let user_id = state
        .service
        .exchange_ticket(request.ticket)
        .await
        .map_err(map_oauth_login_error)?;
    issue_session_for_user(state.session_authenticator.as_ref(), user_id).await
}

async fn get_admin_github_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    get_admin_settings(state, authentication, GITHUB_PROVIDER).await
}

async fn get_admin_discord_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    get_admin_settings(state, authentication, DISCORD_PROVIDER).await
}

async fn get_admin_oidc_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    get_admin_settings(state, authentication, OIDC_PROVIDER).await
}

async fn get_admin_linuxdo_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    get_admin_settings(state, authentication, LINUXDO_PROVIDER).await
}

async fn get_admin_wechat_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    get_admin_settings(state, authentication, WECHAT_PROVIDER).await
}

async fn get_admin_telegram_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    get_admin_settings(state, authentication, TELEGRAM_PROVIDER).await
}

async fn get_admin_google_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    get_admin_settings(state, authentication, GOOGLE_PROVIDER).await
}

async fn get_admin_settings(
    state: OAuthLoginHttpState,
    authentication: af_admin::SessionAuthentication,
    provider: &str,
) -> Result<Response, ManagementError> {
    let settings = state
        .service
        .admin_settings(authentication.principal(), provider)
        .await
        .map_err(map_oauth_login_error)?;
    Ok(no_store_json(
        AdminOAuthLoginProviderSettingsResponse::from(settings),
    ))
}

async fn update_admin_github_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminOAuthLoginProviderSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    update_admin_settings(state, authentication, request, GITHUB_PROVIDER).await
}

async fn update_admin_discord_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminOAuthLoginProviderSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    update_admin_settings(state, authentication, request, DISCORD_PROVIDER).await
}

async fn update_admin_oidc_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminOAuthLoginProviderSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    update_admin_settings(state, authentication, request, OIDC_PROVIDER).await
}

async fn update_admin_linuxdo_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminOAuthLoginProviderSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    update_admin_settings(state, authentication, request, LINUXDO_PROVIDER).await
}

async fn update_admin_wechat_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminOAuthLoginProviderSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    update_admin_settings(state, authentication, request, WECHAT_PROVIDER).await
}

async fn update_admin_telegram_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminOAuthLoginProviderSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    update_admin_settings(state, authentication, request, TELEGRAM_PROVIDER).await
}

async fn update_admin_google_settings(
    State(state): State<OAuthLoginHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminOAuthLoginProviderSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    update_admin_settings(state, authentication, request, GOOGLE_PROVIDER).await
}

async fn update_admin_settings(
    state: OAuthLoginHttpState,
    authentication: af_admin::SessionAuthentication,
    request: Result<Json<AdminOAuthLoginProviderSettingsRequest>, JsonRejection>,
    provider: &str,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminOAuthLoginProviderSettingsCommand::new(
        request.expected_version,
        request.enabled,
        request.client_id,
        request.issuer_url,
        request.client_secret,
        request.clear_client_secret,
    )
    .map_err(map_oauth_login_error)?;
    let settings = state
        .service
        .update_admin_settings(authentication.principal(), provider, command)
        .await
        .map_err(map_oauth_login_error)?;
    Ok(no_store_json(
        AdminOAuthLoginProviderSettingsResponse::from(settings),
    ))
}

fn redirect_no_store(target: &str) -> Result<Response, ManagementError> {
    let location = HeaderValue::from_str(target).map_err(|_| ManagementError::Internal)?;
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::SEE_OTHER;
    response.headers_mut().insert(LOCATION, location);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    Ok(response)
}

/// 授权 URL 由受信服务构造；HTTP 边界仍闭合校验绑定 Cookie 所需字段。
fn oauth_state_cookie_material(authorization_url: &str) -> Result<(String, bool), ManagementError> {
    let authorization_url = Url::parse(authorization_url).map_err(|_| ManagementError::Internal)?;
    let mut state = None;
    let mut redirect_uri = None;
    for (name, value) in authorization_url.query_pairs() {
        let duplicate = match name.as_ref() {
            "state" => state.replace(value.into_owned()).is_some(),
            "redirect_uri" => redirect_uri.replace(value.into_owned()).is_some(),
            _ => false,
        };
        if duplicate {
            return Err(ManagementError::Internal);
        }
    }
    let state = state
        .filter(|value| !value.is_empty() && value.len() <= 4_096)
        .ok_or(ManagementError::Internal)?;
    let redirect_uri = Url::parse(&redirect_uri.ok_or(ManagementError::Internal)?)
        .map_err(|_| ManagementError::Internal)?;
    let secure = match redirect_uri.scheme() {
        "https" => true,
        "http" => false,
        _ => return Err(ManagementError::Internal),
    };
    Ok((oauth_state_cookie_digest(&state), secure))
}

/// 仅提取回调必需字段；重复字段拒绝，Provider 附加诊断字段忽略。
fn parse_oauth_callback_query(
    raw_query: Option<&str>,
) -> Result<OAuthLoginCallbackQuery, OAuthLoginError> {
    let mut state = None;
    let mut code = None;
    let mut error = None;
    for (name, value) in url::form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes()) {
        let slot = match name.as_ref() {
            "state" => &mut state,
            "code" => &mut code,
            "error" => &mut error,
            _ => continue,
        };
        if slot.replace(value.into_owned()).is_some() {
            return Err(OAuthLoginError::Rejected);
        }
    }
    Ok(OAuthLoginCallbackQuery { state, code, error })
}

fn oauth_state_set_cookie(
    digest: &str,
    secure: bool,
    callback_path: &str,
) -> Result<HeaderValue, ManagementError> {
    let secure_attribute = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{OAUTH_STATE_COOKIE}={digest}; Path={callback_path}; Max-Age={OAUTH_STATE_COOKIE_MAX_AGE_SECONDS}; HttpOnly; SameSite=Lax{secure_attribute}"
    ))
    .map_err(|_| ManagementError::Internal)
}

/// Cookie 只保存 state 摘要；重复同名 Cookie 一律拒绝，避免解析歧义。
fn oauth_state_cookie_matches(headers: &HeaderMap, state: &str) -> bool {
    let expected = oauth_state_cookie_digest(state);
    let mut actual = None;
    for header in headers.get_all(COOKIE) {
        let Ok(header) = header.to_str() else {
            return false;
        };
        for pair in header.split(';') {
            let Some((name, value)) = pair.trim().split_once('=') else {
                continue;
            };
            if name == OAUTH_STATE_COOKIE && actual.replace(value).is_some() {
                return false;
            }
        }
    }
    actual.is_some_and(|actual| constant_time_eq(actual.as_bytes(), expected.as_bytes()))
}

fn oauth_state_cookie_digest(state: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(OAUTH_STATE_COOKIE_DOMAIN.as_bytes());
    hasher.update([0]);
    hasher.update(state.as_bytes());
    let bytes = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn harden_oauth_callback_response(response: &mut Response, callback_path: &str) {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{OAUTH_STATE_COOKIE}=; Path={callback_path}; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; HttpOnly; SameSite=Lax"
        )).unwrap_or_else(|_| HeaderValue::from_static("af_oauth_login_state=; Max-Age=0")),
    );
}

fn valid_provider_error(error: &str) -> bool {
    !error.is_empty() && error.len() <= 256 && !error.chars().any(char::is_control)
}

fn custom_oauth2_callback_path(provider_key: &str) -> Result<String, OAuthLoginError> {
    if provider_key.len() <= "custom_".len()
        || provider_key.len() > 32
        || !provider_key.starts_with("custom_")
        || !provider_key.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(OAuthLoginError::InvalidInput);
    }
    Ok(format!("/api/auth/oauth/custom/{provider_key}/callback"))
}

fn map_oauth_login_error(error: OAuthLoginError) -> ManagementError {
    match error {
        OAuthLoginError::InvalidInput => ManagementError::InvalidRequest,
        OAuthLoginError::Forbidden => ManagementError::Forbidden,
        OAuthLoginError::Unavailable => ManagementError::OauthProviderNotConfigured,
        OAuthLoginError::Rejected => ManagementError::OauthAuthorizationDenied,
        OAuthLoginError::ConcurrentUpdate => ManagementError::OauthLoginSettingsConflict,
        OAuthLoginError::ProviderUnavailable => ManagementError::OauthUnavailable,
        OAuthLoginError::Internal => ManagementError::Internal,
    }
}
