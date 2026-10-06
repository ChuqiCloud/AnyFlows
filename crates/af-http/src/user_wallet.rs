use std::sync::Arc;

use af_admin::{
    SessionAuthentication, SessionAuthenticator, UserWalletEntry, UserWalletEntryType,
    UserWalletError, UserWalletListQuery, UserWalletPage, UserWalletService, UserWalletSummary,
};
use axum::{
    Router,
    extract::{Extension, RawQuery, State},
    middleware,
    response::Response,
    routing::get,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
    wallet_query::parse_wallet_list_query,
};

/// 当前用户钱包路由独立持有的只读应用服务。
#[derive(Clone)]
pub(crate) struct UserWalletHttpState {
    service: Arc<dyn UserWalletService>,
}

impl UserWalletHttpState {
    /// 绑定启动期装配的当前用户钱包服务。
    pub(crate) fn new(service: Arc<dyn UserWalletService>) -> Self {
        Self { service }
    }
}

/// 构建只接受有效登录会话的当前用户钱包路由。
pub(crate) fn build_user_wallet_router(
    service: Arc<dyn UserWalletService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/account/wallet", get(get_user_wallet))
        .route("/api/account/wallet/entries", get(list_user_wallet_entries))
        .layer(authentication)
        .with_state(UserWalletHttpState::new(service))
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserWalletSummary)]
pub(crate) struct UserWalletSummaryResponse {
    #[schema(minimum = 0)]
    balance: i64,
    #[schema(minimum = 0)]
    used_quota: i64,
    #[schema(minimum = 0)]
    frozen_quota: i64,
}

/// 当前用户钱包账本事件类型响应。
#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = UserWalletEntryType, rename_all = "snake_case")]
pub(crate) enum UserWalletEntryTypeResponse {
    OpeningBalance,
    AdminAdjustment,
    Topup,
    Redemption,
    InviteRebate,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserWalletEntry)]
pub(crate) struct UserWalletEntryResponse {
    #[schema(minimum = 1)]
    id: i64,
    entry_type: UserWalletEntryTypeResponse,
    quota_delta: i64,
    #[schema(minimum = 0)]
    balance_before: i64,
    #[schema(minimum = 0)]
    balance_after: i64,
    #[schema(min_length = 1, max_length = 500, required = true)]
    reason: Option<String>,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserWalletListResponse)]
pub(crate) struct UserWalletListResponse {
    #[schema(max_items = 100)]
    entries: Vec<UserWalletEntryResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

/// 返回当前会话用户自己的余额状态。
pub(crate) async fn get_user_wallet(
    State(state): State<UserWalletHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let summary = state
        .service
        .summary(authentication.principal())
        .await
        .map_err(map_wallet_error)?;
    Ok(no_store_json(UserWalletSummaryResponse::from_summary(
        summary,
    )))
}

/// 按账本主键倒序返回当前会话用户自己的余额变更事实。
pub(crate) async fn list_user_wallet_entries(
    State(state): State<UserWalletHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let (before, limit) = parse_wallet_list_query(
        raw_query.as_deref(),
        af_admin::DEFAULT_USER_WALLET_PAGE_SIZE,
    )?;
    let query = UserWalletListQuery::new(before, limit).map_err(map_wallet_error)?;
    let page = state
        .service
        .list(authentication.principal(), query)
        .await
        .map_err(map_wallet_error)?;
    Ok(no_store_json(UserWalletListResponse::from_page(&page)))
}

impl UserWalletSummaryResponse {
    fn from_summary(summary: UserWalletSummary) -> Self {
        Self {
            balance: summary.balance(),
            used_quota: summary.used_quota(),
            frozen_quota: summary.frozen_quota(),
        }
    }
}

impl UserWalletListResponse {
    fn from_page(page: &UserWalletPage) -> Self {
        Self {
            entries: page
                .entries()
                .iter()
                .map(UserWalletEntryResponse::from_entry)
                .collect(),
            next_cursor: page.next_cursor(),
        }
    }
}

impl UserWalletEntryResponse {
    fn from_entry(entry: &UserWalletEntry) -> Self {
        Self {
            id: entry.id(),
            entry_type: match entry.entry_type() {
                UserWalletEntryType::OpeningBalance => UserWalletEntryTypeResponse::OpeningBalance,
                UserWalletEntryType::AdminAdjustment => {
                    UserWalletEntryTypeResponse::AdminAdjustment
                }
                UserWalletEntryType::Topup => UserWalletEntryTypeResponse::Topup,
                UserWalletEntryType::Redemption => UserWalletEntryTypeResponse::Redemption,
                UserWalletEntryType::InviteRebate => UserWalletEntryTypeResponse::InviteRebate,
            },
            quota_delta: entry.quota_delta(),
            balance_before: entry.balance_before(),
            balance_after: entry.balance_after(),
            reason: entry.reason().map(str::to_owned),
            created_at: entry.created_at(),
        }
    }
}

fn map_wallet_error(error: UserWalletError) -> ManagementError {
    match error {
        UserWalletError::InvalidInput => ManagementError::InvalidRequest,
        UserWalletError::InvalidSession => ManagementError::InvalidSession,
        UserWalletError::Internal => ManagementError::Internal,
    }
}
