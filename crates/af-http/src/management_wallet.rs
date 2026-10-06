use af_admin::{
    AdminWalletAdjustmentCommand, AdminWalletAdjustmentResult, AdminWalletEntry,
    AdminWalletEntryType, AdminWalletError, AdminWalletListQuery, AdminWalletPage,
};
use af_domain::{UserId, WalletEventId};
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState, management_error::ManagementError,
    wallet_query::parse_wallet_list_query,
};

/// 管理端钱包账本事件类型响应。
#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminWalletEntryType, rename_all = "snake_case")]
pub(crate) enum AdminWalletEntryTypeResponse {
    OpeningBalance,
    AdminAdjustment,
    Topup,
    Redemption,
    InviteRebate,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminWalletEntry)]
pub(crate) struct AdminWalletEntryResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    event_id: String,
    #[schema(minimum = 1)]
    user_id: i64,
    #[schema(minimum = 1, required = true)]
    actor_user_id: Option<i64>,
    entry_type: AdminWalletEntryTypeResponse,
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
#[schema(as = AdminWalletListResponse)]
pub(crate) struct AdminWalletListResponse {
    #[schema(max_items = 100)]
    entries: Vec<AdminWalletEntryResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminWalletAdjustmentRequest)]
/// 管理员调账正文；请求结果不确定时必须原样复用 `event_id`。
pub(crate) struct AdminWalletAdjustmentRequest {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    event_id: String,
    quota_delta: i64,
    #[schema(min_length = 1, max_length = 500)]
    reason: String,
}

/// 读取指定有效用户的一页不可变钱包账本。
pub(crate) async fn list_admin_wallet_entries(
    State(state): State<HttpState>,
    Path(user_id): Path<String>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_wallet_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = service
        .list(
            authentication.principal(),
            parse_user_id(&user_id)?,
            parse_list_query(raw_query.as_deref())?,
        )
        .await
        .map_err(map_wallet_error)?;
    Ok(no_store_json(AdminWalletListResponse::from_page(&page)))
}

/// 原子追加一次管理员有符号调账，新提交返回 201，幂等重放返回 200。
pub(crate) async fn adjust_admin_wallet(
    State(state): State<HttpState>,
    Path(user_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminWalletAdjustmentRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_wallet_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = request.into_command()?;
    let result = service
        .adjust(
            authentication.principal(),
            parse_user_id(&user_id)?,
            command,
        )
        .await
        .map_err(map_wallet_error)?;
    let (status, entry) = match result {
        AdminWalletAdjustmentResult::Applied(entry) => (StatusCode::CREATED, entry),
        AdminWalletAdjustmentResult::Existing(entry) => (StatusCode::OK, entry),
    };
    Ok(status_json(
        status,
        AdminWalletEntryResponse::from_entry(&entry),
    ))
}

impl AdminWalletAdjustmentRequest {
    fn into_command(self) -> Result<AdminWalletAdjustmentCommand, ManagementError> {
        let event_id = WalletEventId::from_persistence_key(&self.event_id)
            .map_err(|_| ManagementError::InvalidRequest)?;
        AdminWalletAdjustmentCommand::new(event_id, self.quota_delta, self.reason)
            .map_err(map_wallet_error)
    }
}

impl AdminWalletListResponse {
    fn from_page(page: &AdminWalletPage) -> Self {
        Self {
            entries: page
                .entries()
                .iter()
                .map(AdminWalletEntryResponse::from_entry)
                .collect(),
            next_cursor: page.next_cursor(),
        }
    }
}

impl AdminWalletEntryResponse {
    fn from_entry(entry: &AdminWalletEntry) -> Self {
        Self {
            id: entry.id(),
            event_id: entry.event_id().persistence_key(),
            user_id: entry.user_id().get(),
            actor_user_id: entry.actor_user_id().map(UserId::get),
            entry_type: match entry.entry_type() {
                AdminWalletEntryType::OpeningBalance => {
                    AdminWalletEntryTypeResponse::OpeningBalance
                }
                AdminWalletEntryType::AdminAdjustment => {
                    AdminWalletEntryTypeResponse::AdminAdjustment
                }
                AdminWalletEntryType::Topup => AdminWalletEntryTypeResponse::Topup,
                AdminWalletEntryType::Redemption => AdminWalletEntryTypeResponse::Redemption,
                AdminWalletEntryType::InviteRebate => AdminWalletEntryTypeResponse::InviteRebate,
            },
            quota_delta: entry.quota_delta(),
            balance_before: entry.balance_before(),
            balance_after: entry.balance_after(),
            reason: entry.reason().map(str::to_owned),
            created_at: entry.created_at(),
        }
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<AdminWalletListQuery, ManagementError> {
    let (before, limit) =
        parse_wallet_list_query(raw_query, af_admin::DEFAULT_ADMIN_WALLET_PAGE_SIZE)?;
    AdminWalletListQuery::new(before, limit).map_err(map_wallet_error)
}

fn parse_user_id(value: &str) -> Result<UserId, ManagementError> {
    UserId::new(parse_positive_i64(value)?).map_err(|_| ManagementError::InvalidRequest)
}

fn parse_positive_i64(value: &str) -> Result<i64, ManagementError> {
    if value.is_empty() || value.chars().any(|character| !character.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    let value = value
        .parse::<i64>()
        .map_err(|_| ManagementError::InvalidRequest)?;
    (value > 0)
        .then_some(value)
        .ok_or(ManagementError::InvalidRequest)
}

fn map_wallet_error(error: AdminWalletError) -> ManagementError {
    match error {
        AdminWalletError::InvalidInput => ManagementError::InvalidRequest,
        AdminWalletError::Forbidden => ManagementError::Forbidden,
        AdminWalletError::NotFound => ManagementError::UserNotFound,
        AdminWalletError::Conflict => ManagementError::WalletEventConflict,
        AdminWalletError::InsufficientQuota => ManagementError::WalletInsufficientQuota,
        AdminWalletError::Overflow => ManagementError::WalletOverflow,
        AdminWalletError::OutcomeUnknown => ManagementError::WalletOutcomeUnknown,
        AdminWalletError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = no_store_json(value);
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_parser_rejects_duplicate_unknown_and_unstable_inputs() {
        assert!(parse_list_query(None).is_ok());
        for query in [
            "before=0",
            "before=-1",
            "before=1&before=2",
            "limit=0",
            "limit=101",
            "limit=1&limit=2",
            "unknown=1",
            "before=%",
            "before=%ff",
        ] {
            assert_eq!(
                parse_list_query(Some(query)),
                Err(ManagementError::InvalidRequest),
                "{query}"
            );
        }
    }
}
