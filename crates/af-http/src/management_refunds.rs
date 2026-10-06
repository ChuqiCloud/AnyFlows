use af_admin::{
    AdminRefundDecisionCommand, AdminRefundError, AdminRefundListQuery,
    AdminRefundManualCompletionCommand, AdminRefundPage, AdminRefundRequest,
    RefundReconciliationEntry, RefundReconciliationListQuery, RefundReconciliationPage,
};
use af_domain::{
    RefundApprovalStatus, RefundManualResult, RefundOrderKind, RefundRequestId, RefundRequestKey,
    RefundRequestStatus,
};
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminRefundOrderKind, rename_all = "snake_case")]
pub(crate) enum AdminRefundOrderKindResponse {
    Topup,
    Subscription,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminRefundStatus, rename_all = "snake_case")]
pub(crate) enum AdminRefundStatusResponse {
    Requested,
    Submitted,
    Succeeded,
    Failed,
    Canceled,
    ManuallySucceeded,
    ManuallyFailed,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminRefundApprovalStatus, rename_all = "snake_case")]
pub(crate) enum AdminRefundApprovalStatusResponse {
    Pending,
    Approved,
    Rejected,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRefundRequest)]
pub(crate) struct AdminRefundRequestResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    request_id: String,
    #[schema(minimum = 1)]
    user_id: i64,
    order_kind: AdminRefundOrderKindResponse,
    #[schema(min_length = 1, max_length = 32)]
    order_key: String,
    #[schema(min_length = 1, max_length = 64)]
    provider: String,
    #[schema(min_length = 3, max_length = 16)]
    currency: String,
    original_amount_minor: i64,
    refund_amount_minor: i64,
    #[schema(min_length = 1, max_length = 128, required = true)]
    provider_refund_id: Option<String>,
    status: AdminRefundStatusResponse,
    approval_status: AdminRefundApprovalStatusResponse,
    #[schema(minimum = 1, required = true)]
    approval_actor_id: Option<i64>,
    #[schema(min_length = 1, max_length = 512, required = true)]
    approval_reason: Option<String>,
    #[schema(minimum = 1)]
    version: u64,
    #[schema(minimum = 0)]
    created_at: i64,
    #[schema(minimum = 0)]
    updated_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRefundListResponse)]
pub(crate) struct AdminRefundListResponse {
    #[schema(max_items = 100)]
    entries: Vec<AdminRefundRequestResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = RefundReconciliationStatus, rename_all = "snake_case")]
pub(crate) enum RefundReconciliationStatusResponse {
    Succeeded,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = RefundReconciliationEntry)]
pub(crate) struct RefundReconciliationEntryResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    request_id: String,
    #[schema(minimum = 1)]
    user_id: i64,
    #[schema(minimum = 1, required = true)]
    organization_id: Option<i64>,
    #[schema(minimum = 1)]
    approval_actor_id: i64,
    order_kind: AdminRefundOrderKindResponse,
    #[schema(min_length = 1, max_length = 32)]
    order_key: String,
    #[schema(min_length = 1, max_length = 64)]
    provider: String,
    #[schema(maximum = -1)]
    amount_delta_minor: i64,
    #[schema(min_length = 3, max_length = 16)]
    currency: String,
    status: RefundReconciliationStatusResponse,
    #[schema(minimum = 0)]
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = RefundReconciliationListResponse)]
pub(crate) struct RefundReconciliationListResponse {
    #[schema(max_items = 100)]
    entries: Vec<RefundReconciliationEntryResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRefundDecisionRequest)]
pub(crate) struct AdminRefundDecisionRequest {
    #[schema(min_length = 1, max_length = 512, required = true)]
    reason: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRefundManualCompletionRequest)]
pub(crate) struct AdminRefundManualCompletionRequest {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    completion_key: String,
    #[schema(minimum = 1)]
    expected_version: u64,
    #[schema(pattern = "^(completed|failed)$")]
    result: String,
    #[schema(min_length = 1, max_length = 256)]
    reference: String,
}

/// 读取管理员退款审批队列，游标只使用数据库自增 ID。
pub(crate) async fn list_admin_refunds(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_refund_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = service
        .list(
            authentication.principal(),
            parse_list_query(raw_query.as_deref())?,
        )
        .await
        .map_err(map_refund_error)?;
    Ok(no_store_json(AdminRefundListResponse::from_page(&page)))
}

/// 读取当前登录用户自己的现金退款成功对账事实。
pub(crate) async fn list_account_refund_reconciliations(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_refund_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = service
        .list_user_reconciliations(
            authentication.principal(),
            parse_reconciliation_query(raw_query.as_deref())?,
        )
        .await
        .map_err(map_refund_error)?;
    Ok(no_store_json(RefundReconciliationListResponse::from_page(
        &page,
    )))
}

/// 读取平台管理员可见的全局现金退款成功对账事实。
pub(crate) async fn list_admin_refund_reconciliations(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_refund_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = service
        .list_admin_reconciliations(
            authentication.principal(),
            parse_reconciliation_query(raw_query.as_deref())?,
        )
        .await
        .map_err(map_refund_error)?;
    Ok(no_store_json(RefundReconciliationListResponse::from_page(
        &page,
    )))
}

/// 批准退款；自动提交开启时，批准成功后立即复用同一退款事实提交 Provider。
pub(crate) async fn approve_admin_refund(
    State(state): State<HttpState>,
    Path(request_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminRefundDecisionRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    decide_admin_refund(state, request_id, authentication, request, true).await
}

/// 拒绝尚未提交 Provider 的退款申请。
pub(crate) async fn reject_admin_refund(
    State(state): State<HttpState>,
    Path(request_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminRefundDecisionRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    decide_admin_refund(state, request_id, authentication, request, false).await
}

/// 在关闭自动提交或自动提交失败时，由管理员手动提交已批准退款。
pub(crate) async fn submit_admin_refund(
    State(state): State<HttpState>,
    Path(request_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_refund_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let result = service
        .submit(authentication.principal(), parse_request_id(&request_id)?)
        .await
        .map_err(map_refund_error)?;
    Ok(status_json(
        StatusCode::OK,
        AdminRefundListResponse::from_page(&result),
    ))
}

/// 登记易支付线下退款结果，不发送任何 Provider 请求。
pub(crate) async fn manual_complete_admin_refund(
    State(state): State<HttpState>,
    Path(request_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminRefundManualCompletionRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_refund_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let completion_key = RefundRequestKey::from_persistence_key(&request.completion_key)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let result = match request.result.as_str() {
        "completed" => RefundManualResult::Completed,
        "failed" => RefundManualResult::Failed,
        _ => return Err(ManagementError::InvalidRequest),
    };
    let command = AdminRefundManualCompletionCommand::new(
        completion_key,
        request.expected_version,
        result,
        request.reference,
    )
    .map_err(map_refund_error)?;
    let page = service
        .complete_manual(
            authentication.principal(),
            parse_request_id(&request_id)?,
            command,
        )
        .await
        .map_err(map_refund_error)?;
    Ok(status_json(
        StatusCode::OK,
        AdminRefundListResponse::from_page(&page),
    ))
}

async fn decide_admin_refund(
    state: HttpState,
    request_id: String,
    authentication: af_admin::SessionAuthentication,
    request: Result<Json<AdminRefundDecisionRequest>, JsonRejection>,
    approve: bool,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_refund_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminRefundDecisionCommand::new(request.reason).map_err(map_refund_error)?;
    let request_id = parse_request_id(&request_id)?;
    let page = if approve {
        service
            .approve(authentication.principal(), request_id, command)
            .await
    } else {
        service
            .reject(authentication.principal(), request_id, command)
            .await
    }
    .map_err(map_refund_error)?;
    Ok(status_json(
        StatusCode::OK,
        AdminRefundListResponse::from_page(&page),
    ))
}

impl AdminRefundListResponse {
    fn from_page(page: &AdminRefundPage) -> Self {
        Self {
            entries: page
                .entries()
                .iter()
                .map(AdminRefundRequestResponse::from_request)
                .collect(),
            next_cursor: page.next_cursor(),
        }
    }
}

impl AdminRefundRequestResponse {
    fn from_request(request: &AdminRefundRequest) -> Self {
        Self {
            request_id: request.request_id().persistence_key(),
            user_id: request.user_id().get(),
            order_kind: match request.order_kind() {
                RefundOrderKind::Topup => AdminRefundOrderKindResponse::Topup,
                RefundOrderKind::Subscription => AdminRefundOrderKindResponse::Subscription,
            },
            order_key: request.order_key().to_owned(),
            provider: request.provider().to_owned(),
            currency: request.currency().to_owned(),
            original_amount_minor: request.original_amount_minor(),
            refund_amount_minor: request.refund_amount_minor(),
            provider_refund_id: request.provider_refund_id().map(str::to_owned),
            status: match request.status() {
                RefundRequestStatus::Requested => AdminRefundStatusResponse::Requested,
                RefundRequestStatus::Submitted => AdminRefundStatusResponse::Submitted,
                RefundRequestStatus::Succeeded => AdminRefundStatusResponse::Succeeded,
                RefundRequestStatus::Failed => AdminRefundStatusResponse::Failed,
                RefundRequestStatus::Canceled => AdminRefundStatusResponse::Canceled,
                RefundRequestStatus::ManuallySucceeded => {
                    AdminRefundStatusResponse::ManuallySucceeded
                }
                RefundRequestStatus::ManuallyFailed => AdminRefundStatusResponse::ManuallyFailed,
            },
            approval_status: match request.approval_status() {
                RefundApprovalStatus::Pending => AdminRefundApprovalStatusResponse::Pending,
                RefundApprovalStatus::Approved => AdminRefundApprovalStatusResponse::Approved,
                RefundApprovalStatus::Rejected => AdminRefundApprovalStatusResponse::Rejected,
            },
            approval_actor_id: request.approval_actor_id().map(|id| id.get()),
            approval_reason: request.approval_reason().map(str::to_owned),
            version: request.version(),
            created_at: i64::try_from(request.created_at()).unwrap_or(i64::MAX),
            updated_at: i64::try_from(request.updated_at()).unwrap_or(i64::MAX),
        }
    }
}

impl RefundReconciliationListResponse {
    fn from_page(page: &RefundReconciliationPage) -> Self {
        Self {
            entries: page
                .entries()
                .iter()
                .map(RefundReconciliationEntryResponse::from_entry)
                .collect(),
            next_cursor: page.next_cursor(),
        }
    }
}

impl RefundReconciliationEntryResponse {
    fn from_entry(entry: &RefundReconciliationEntry) -> Self {
        Self {
            request_id: entry.request_id().persistence_key(),
            user_id: entry.user_id().get(),
            organization_id: entry.organization_id().map(|id| id.get()),
            approval_actor_id: entry.approval_actor_id().get(),
            order_kind: match entry.order_kind() {
                RefundOrderKind::Topup => AdminRefundOrderKindResponse::Topup,
                RefundOrderKind::Subscription => AdminRefundOrderKindResponse::Subscription,
            },
            order_key: entry.order_key().to_owned(),
            provider: entry.provider().to_owned(),
            amount_delta_minor: entry.amount_delta_minor(),
            currency: entry.currency().to_owned(),
            status: RefundReconciliationStatusResponse::Succeeded,
            created_at: i64::try_from(entry.created_at()).unwrap_or(i64::MAX),
        }
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<AdminRefundListQuery, ManagementError> {
    let mut after_id = None;
    let mut approval_status = None;
    let mut limit = af_admin::DEFAULT_ADMIN_REFUND_PAGE_SIZE;
    let mut seen_after = false;
    let mut seen_status = false;
    let mut seen_limit = false;
    if let Some(raw_query) = raw_query {
        for pair in raw_query.split('&') {
            let (key, value) = pair
                .split_once('=')
                .ok_or(ManagementError::InvalidRequest)?;
            match key {
                "after" if !seen_after => {
                    seen_after = true;
                    after_id = Some(parse_positive_i64(value)?);
                }
                "approval_status" if !seen_status => {
                    seen_status = true;
                    approval_status = Some(parse_approval_status(value)?);
                }
                "limit" if !seen_limit => {
                    seen_limit = true;
                    limit = value
                        .parse::<usize>()
                        .map_err(|_| ManagementError::InvalidRequest)?;
                }
                _ => return Err(ManagementError::InvalidRequest),
            }
        }
    }
    AdminRefundListQuery::new(after_id, approval_status, limit).map_err(map_refund_error)
}

fn parse_reconciliation_query(
    raw_query: Option<&str>,
) -> Result<RefundReconciliationListQuery, ManagementError> {
    let mut before_id = None;
    let mut limit = af_admin::DEFAULT_REFUND_RECONCILIATION_PAGE_SIZE;
    let mut seen_before = false;
    let mut seen_limit = false;
    if let Some(raw_query) = raw_query {
        for pair in raw_query.split('&') {
            let (key, value) = pair
                .split_once('=')
                .ok_or(ManagementError::InvalidRequest)?;
            match key {
                "before" if !seen_before => {
                    seen_before = true;
                    before_id = Some(parse_positive_i64(value)?);
                }
                "limit" if !seen_limit => {
                    seen_limit = true;
                    limit = value
                        .parse::<usize>()
                        .map_err(|_| ManagementError::InvalidRequest)?;
                }
                _ => return Err(ManagementError::InvalidRequest),
            }
        }
    }
    RefundReconciliationListQuery::new(before_id, limit).map_err(map_refund_error)
}

fn parse_approval_status(value: &str) -> Result<RefundApprovalStatus, ManagementError> {
    match value {
        "pending" => Ok(RefundApprovalStatus::Pending),
        "approved" => Ok(RefundApprovalStatus::Approved),
        "rejected" => Ok(RefundApprovalStatus::Rejected),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn parse_request_id(value: &str) -> Result<RefundRequestId, ManagementError> {
    RefundRequestId::from_persistence_key(value).map_err(|_| ManagementError::InvalidRequest)
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

fn map_refund_error(error: AdminRefundError) -> ManagementError {
    match error {
        AdminRefundError::InvalidInput => ManagementError::InvalidRequest,
        AdminRefundError::Forbidden => ManagementError::Forbidden,
        AdminRefundError::NotFound => ManagementError::RefundNotFound,
        AdminRefundError::Conflict => ManagementError::RefundConflict,
        AdminRefundError::Unavailable => ManagementError::RefundUnavailable,
        AdminRefundError::AutoSubmitFailed => ManagementError::RefundAutoSubmitFailed,
        AdminRefundError::OutcomeUnknown => ManagementError::RefundOutcomeUnknown,
        AdminRefundError::Internal => ManagementError::Internal,
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
    fn list_query_rejects_duplicate_unknown_and_invalid_inputs() {
        assert!(parse_list_query(None).is_ok());
        for query in [
            "after=0",
            "after=-1",
            "after=1&after=2",
            "approval_status=unknown",
            "approval_status=pending&approval_status=approved",
            "limit=0",
            "limit=101",
            "limit=1&limit=2",
            "unknown=1",
            "after=%",
        ] {
            assert_eq!(
                parse_list_query(Some(query)),
                Err(ManagementError::InvalidRequest),
                "{query}"
            );
        }
    }

    #[test]
    fn reconciliation_query_rejects_duplicate_unknown_and_invalid_inputs() {
        assert!(parse_reconciliation_query(None).is_ok());
        for query in [
            "before=0",
            "before=-1",
            "before=1&before=2",
            "limit=0",
            "limit=101",
            "limit=1&limit=2",
            "unknown=1",
            "before=%",
        ] {
            assert_eq!(
                parse_reconciliation_query(Some(query)),
                Err(ManagementError::InvalidRequest),
                "{query}"
            );
        }
    }
}
