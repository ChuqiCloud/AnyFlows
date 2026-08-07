use std::sync::Arc;

use af_admin::{
    AdminRedemptionAuditBatch, AdminRedemptionAuditQuery, AdminRedemptionAuditStatus,
    AdminRedemptionAuditSummary, AdminRedemptionBatch, AdminRedemptionBatchCreateCommand,
    AdminRedemptionBatchDisableCommand, AdminRedemptionBatchListQuery, IssuedAdminRedemptionBatch,
    RedemptionService, RedemptionServiceError, SessionAuthentication, SessionAuthenticator,
    UserRedemptionCommand, UserRedemptionResult,
};
use af_domain::{RedemptionBatchId, RedemptionBatchStatus};
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
    wallet_query::parse_wallet_list_query,
};

#[derive(Clone)]
struct RedemptionHttpState {
    service: Arc<dyn RedemptionService>,
}

/// 管理 API 使用的闭合兑换码批次状态。
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminRedemptionBatchStatus, rename_all = "snake_case")]
pub(crate) enum AdminRedemptionBatchStatusDto {
    Active,
    Disabled,
}

impl From<RedemptionBatchStatus> for AdminRedemptionBatchStatusDto {
    fn from(value: RedemptionBatchStatus) -> Self {
        match value {
            RedemptionBatchStatus::Active => Self::Active,
            RedemptionBatchStatus::Disabled => Self::Disabled,
        }
    }
}

/// 兑换码运营报表支持的闭合筛选状态。
#[allow(dead_code, reason = "筛选枚举仅由 OpenAPI 过程宏读取")]
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminRedemptionAuditStatus, rename_all = "snake_case")]
pub(crate) enum AdminRedemptionAuditStatusDto {
    Active,
    Expired,
    Disabled,
    Redeemed,
}

/// 管理员可见的兑换码批次汇总，不包含明文或摘要。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRedemptionBatch)]
pub(crate) struct AdminRedemptionBatchResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    batch_id: String,
    #[schema(min_length = 1, max_length = 80)]
    name: String,
    #[schema(minimum = 1)]
    created_by_user_id: i64,
    status: AdminRedemptionBatchStatusDto,
    #[schema(minimum = 1)]
    quota_amount: i64,
    #[schema(minimum = 1, maximum = 1000)]
    code_count: usize,
    #[schema(minimum = 0, maximum = 1000)]
    redeemed_count: usize,
    #[schema(minimum = 1)]
    version: i64,
    #[schema(minimum = 1, required = true)]
    expires_at: Option<i64>,
    #[schema(minimum = 1, required = true)]
    disabled_at: Option<i64>,
    #[schema(minimum = 1)]
    created_at: i64,
    #[schema(minimum = 1)]
    updated_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRedemptionBatchListResponse)]
pub(crate) struct AdminRedemptionBatchListResponse {
    #[schema(max_items = 100)]
    batches: Vec<AdminRedemptionBatchResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

/// 管理员可见的单批次兑换码运营统计，不包含明文码或摘要。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRedemptionAuditBatch)]
pub(crate) struct AdminRedemptionAuditBatchResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    batch_id: String,
    #[schema(min_length = 1, max_length = 80)]
    name: String,
    #[schema(minimum = 1)]
    created_by_user_id: i64,
    status: AdminRedemptionBatchStatusDto,
    #[schema(minimum = 1)]
    quota_amount: i64,
    #[schema(minimum = 1, maximum = 1000)]
    issued_count: usize,
    #[schema(minimum = 0, maximum = 1000)]
    redeemed_count: usize,
    #[schema(minimum = 0, maximum = 1000)]
    remaining_count: usize,
    #[schema(minimum = 0, maximum = 1000)]
    expired_count: usize,
    #[schema(minimum = 0, maximum = 1000)]
    disabled_count: usize,
    #[schema(minimum = 1, required = true)]
    expires_at: Option<i64>,
    #[schema(minimum = 1, required = true)]
    disabled_at: Option<i64>,
    #[schema(minimum = 1, required = true)]
    last_redeemed_at: Option<i64>,
    #[schema(minimum = 1)]
    created_at: i64,
    #[schema(minimum = 1)]
    updated_at: i64,
}

/// 当前返回页内各互斥兑换状态的闭合汇总。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRedemptionAuditSummary)]
pub(crate) struct AdminRedemptionAuditSummaryResponse {
    #[schema(minimum = 0)]
    issued_count: usize,
    #[schema(minimum = 0)]
    redeemed_count: usize,
    #[schema(minimum = 0)]
    remaining_count: usize,
    #[schema(minimum = 0)]
    expired_count: usize,
    #[schema(minimum = 0)]
    disabled_count: usize,
}

/// 管理员兑换码运营报表响应。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRedemptionAuditListResponse)]
pub(crate) struct AdminRedemptionAuditListResponse {
    #[schema(max_items = 100)]
    batches: Vec<AdminRedemptionAuditBatchResponse>,
    summary: AdminRedemptionAuditSummaryResponse,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

/// 管理员创建同面额兑换码批次的结构化请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRedemptionBatchCreateRequest)]
pub(crate) struct AdminRedemptionBatchCreateRequest {
    #[schema(min_length = 1, max_length = 80)]
    name: String,
    #[schema(minimum = 1)]
    quota_amount: i64,
    #[schema(minimum = 1, maximum = 1000)]
    code_count: usize,
    /// Unix 秒时间戳；null 表示永不过期。
    #[schema(minimum = 1, required = true)]
    expires_at: Option<i64>,
}

/// 批次创建响应；完整兑换码只在本次响应中出现。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = IssuedAdminRedemptionBatch)]
pub(crate) struct IssuedAdminRedemptionBatchResponse<'a> {
    batch: AdminRedemptionBatchResponse,
    #[schema(
        min_items = 1,
        max_items = 1000,
        value_type = Vec<String>,
        pattern = "^rc-af-[A-Za-z0-9_-]{43}$"
    )]
    codes: Vec<&'a str>,
}

/// 管理员以当前版本禁用批次的请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRedemptionBatchDisableRequest)]
pub(crate) struct AdminRedemptionBatchDisableRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
}

/// 批次禁用后的状态迁移结果。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRedemptionBatchDisableResponse)]
pub(crate) struct AdminRedemptionBatchDisableResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    batch_id: String,
    #[schema(minimum = 2)]
    version: i64,
    #[schema(minimum = 1)]
    disabled_at: i64,
}

/// 当前用户提交的单个兑换码。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserRedemptionRequest)]
pub(crate) struct UserRedemptionRequest {
    #[schema(
        min_length = 49,
        max_length = 49,
        pattern = "^rc-af-[A-Za-z0-9_-]{43}$",
        format = Password,
        write_only
    )]
    code: String,
}

/// 当前用户兑换成功后的余额结果。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserRedemptionResult)]
pub(crate) struct UserRedemptionResponse {
    #[schema(minimum = 1)]
    quota_amount: i64,
    #[schema(minimum = 0)]
    balance_after: i64,
    #[schema(minimum = 1)]
    redeemed_at: i64,
    replayed: bool,
}

/// 构建管理员批次管理与当前用户兑换路由。
pub(crate) fn build_redemption_router(
    service: Arc<dyn RedemptionService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let state = RedemptionHttpState { service };
    let admin_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let user_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    let admin_routes = Router::new()
        .route(
            "/api/admin/redemption-audit",
            get(list_admin_redemption_audit),
        )
        .route(
            "/api/admin/redemption-batches",
            get(list_admin_redemption_batches).post(create_admin_redemption_batch),
        )
        .route(
            "/api/admin/redemption-batches/{batch_id}/disable",
            post(disable_admin_redemption_batch),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(admin_authentication)
        .with_state(state.clone());
    let user_routes = Router::new()
        .route(
            "/api/account/wallet/redemptions",
            post(redeem_user_redemption_code),
        )
        .route_layer(user_authentication)
        .with_state(state);
    admin_routes.merge(user_routes)
}

/// 按结构化条件读取当前管理员可见的兑换码运营报表。
async fn list_admin_redemption_audit(
    State(state): State<RedemptionHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_audit_query(raw_query.as_deref())?;
    let page = state
        .service
        .audit(authentication.principal(), query)
        .await
        .map_err(map_service_error)?;
    let batches = page
        .batches()
        .iter()
        .map(AdminRedemptionAuditBatchResponse::from_batch)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(no_store_json(AdminRedemptionAuditListResponse {
        batches,
        summary: AdminRedemptionAuditSummaryResponse::from_summary(page.summary()),
        next_cursor: page.next_cursor(),
    }))
}

/// 按新到旧返回兑换码批次汇总。
async fn list_admin_redemption_batches(
    State(state): State<RedemptionHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let (before, limit) = parse_wallet_list_query(
        raw_query.as_deref(),
        af_admin::DEFAULT_ADMIN_REDEMPTION_BATCH_PAGE_SIZE,
    )?;
    let query = AdminRedemptionBatchListQuery::new(before, limit).map_err(map_service_error)?;
    let page = state
        .service
        .list(authentication.principal(), query)
        .await
        .map_err(map_service_error)?;
    let batches = page
        .batches()
        .iter()
        .map(AdminRedemptionBatchResponse::from_batch)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(no_store_json(AdminRedemptionBatchListResponse {
        batches,
        next_cursor: page.next_cursor(),
    }))
}

/// 创建批次并仅在本次响应返回完整兑换码。
async fn create_admin_redemption_batch(
    State(state): State<RedemptionHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AdminRedemptionBatchCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminRedemptionBatchCreateCommand::new(
        request.name,
        request.quota_amount,
        request.code_count,
        request.expires_at,
    )
    .map_err(map_service_error)?;
    let issued = state
        .service
        .create(authentication.principal(), command)
        .await
        .map_err(map_service_error)?;
    let response = IssuedAdminRedemptionBatchResponse::from_issued(&issued)?;
    Ok(status_json(StatusCode::CREATED, response))
}

/// 以当前版本整体禁用一个兑换码批次。
async fn disable_admin_redemption_batch(
    State(state): State<RedemptionHttpState>,
    Path(batch_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AdminRedemptionBatchDisableRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let batch_id = parse_batch_id(&batch_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminRedemptionBatchDisableCommand::new(request.expected_version)
        .map_err(map_service_error)?;
    let result = state
        .service
        .disable(authentication.principal(), batch_id, command)
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(AdminRedemptionBatchDisableResponse {
        batch_id: result.batch_id().persistence_key(),
        version: i64::try_from(result.version()).map_err(|_| ManagementError::Internal)?,
        disabled_at: i64::try_from(result.disabled_at()).map_err(|_| ManagementError::Internal)?,
    }))
}

/// 仅为当前会话用户原子消费一个兑换码。
async fn redeem_user_redemption_code(
    State(state): State<RedemptionHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserRedemptionRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = UserRedemptionCommand::new(request.code).map_err(map_service_error)?;
    let result = state
        .service
        .redeem(authentication.principal(), command)
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(UserRedemptionResponse::from_result(result)?))
}

impl AdminRedemptionBatchResponse {
    fn from_batch(batch: &AdminRedemptionBatch) -> Result<Self, ManagementError> {
        Ok(Self {
            batch_id: batch.batch_id().persistence_key(),
            name: batch.name().to_owned(),
            created_by_user_id: batch.created_by_user_id().get(),
            status: batch.status().into(),
            quota_amount: batch.quota_amount().units(),
            code_count: batch.code_count(),
            redeemed_count: batch.redeemed_count(),
            version: i64::try_from(batch.version()).map_err(|_| ManagementError::Internal)?,
            expires_at: batch
                .expires_at()
                .map(i64::try_from)
                .transpose()
                .map_err(|_| ManagementError::Internal)?,
            disabled_at: batch
                .disabled_at()
                .map(i64::try_from)
                .transpose()
                .map_err(|_| ManagementError::Internal)?,
            created_at: i64::try_from(batch.created_at()).map_err(|_| ManagementError::Internal)?,
            updated_at: i64::try_from(batch.updated_at()).map_err(|_| ManagementError::Internal)?,
        })
    }
}

impl AdminRedemptionAuditBatchResponse {
    fn from_batch(batch: &AdminRedemptionAuditBatch) -> Result<Self, ManagementError> {
        let facts = batch.batch();
        Ok(Self {
            batch_id: facts.batch_id().persistence_key(),
            name: facts.name().to_owned(),
            created_by_user_id: facts.created_by_user_id().get(),
            status: facts.status().into(),
            quota_amount: facts.quota_amount().units(),
            issued_count: batch.issued_count(),
            redeemed_count: facts.redeemed_count(),
            remaining_count: batch.remaining_count(),
            expired_count: batch.expired_count(),
            disabled_count: batch.disabled_count(),
            expires_at: optional_i64(facts.expires_at())?,
            disabled_at: optional_i64(facts.disabled_at())?,
            last_redeemed_at: optional_i64(batch.last_redeemed_at())?,
            created_at: i64::try_from(facts.created_at()).map_err(|_| ManagementError::Internal)?,
            updated_at: i64::try_from(facts.updated_at()).map_err(|_| ManagementError::Internal)?,
        })
    }
}

impl AdminRedemptionAuditSummaryResponse {
    const fn from_summary(summary: AdminRedemptionAuditSummary) -> Self {
        Self {
            issued_count: summary.issued_count(),
            redeemed_count: summary.redeemed_count(),
            remaining_count: summary.remaining_count(),
            expired_count: summary.expired_count(),
            disabled_count: summary.disabled_count(),
        }
    }
}

impl<'a> IssuedAdminRedemptionBatchResponse<'a> {
    fn from_issued(issued: &'a IssuedAdminRedemptionBatch) -> Result<Self, ManagementError> {
        Ok(Self {
            batch: AdminRedemptionBatchResponse::from_batch(issued.batch())?,
            codes: issued
                .codes()
                .iter()
                .map(|code| code.code().expose_secret())
                .collect(),
        })
    }
}

impl UserRedemptionResponse {
    fn from_result(result: UserRedemptionResult) -> Result<Self, ManagementError> {
        Ok(Self {
            quota_amount: result.quota_amount().units(),
            balance_after: result.balance_after().units(),
            redeemed_at: i64::try_from(result.redeemed_at())
                .map_err(|_| ManagementError::Internal)?,
            replayed: result.replayed(),
        })
    }
}

fn parse_batch_id(value: &str) -> Result<RedemptionBatchId, ManagementError> {
    RedemptionBatchId::from_persistence_key(value).map_err(|_| ManagementError::InvalidRequest)
}

fn parse_audit_query(
    raw_query: Option<&str>,
) -> Result<AdminRedemptionAuditQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(AdminRedemptionAuditQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(AdminRedemptionAuditQuery::default());
    }
    if raw_query.len() > 1_024 {
        return Err(ManagementError::InvalidRequest);
    }
    validate_percent_encoding(raw_query)?;

    let mut before = None;
    let mut limit = None;
    let mut batch_id = None;
    let mut status = None;
    let mut redeemed_after = None;
    let mut redeemed_before = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "before" if before.is_none() => before = Some(parse_positive_i64(&value)?),
            "limit" if limit.is_none() => limit = Some(parse_positive_usize(&value)?),
            "batch_id" if batch_id.is_none() => batch_id = Some(parse_batch_id(&value)?),
            "status" if status.is_none() => status = Some(parse_audit_status(&value)?),
            "redeemed_after" if redeemed_after.is_none() => {
                redeemed_after = Some(parse_positive_i64(&value)?);
            }
            "redeemed_before" if redeemed_before.is_none() => {
                redeemed_before = Some(parse_positive_i64(&value)?);
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    AdminRedemptionAuditQuery::new(
        before,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_REDEMPTION_AUDIT_PAGE_SIZE),
        batch_id,
        status,
        redeemed_after,
        redeemed_before,
    )
    .map_err(map_service_error)
}

fn parse_audit_status(value: &str) -> Result<AdminRedemptionAuditStatus, ManagementError> {
    match value {
        "active" => Ok(AdminRedemptionAuditStatus::Active),
        "expired" => Ok(AdminRedemptionAuditStatus::Expired),
        "disabled" => Ok(AdminRedemptionAuditStatus::Disabled),
        "redeemed" => Ok(AdminRedemptionAuditStatus::Redeemed),
        _ => Err(ManagementError::InvalidRequest),
    }
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

fn parse_positive_usize(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty() || value.chars().any(|character| !character.is_ascii_digit()) {
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

fn optional_i64(value: Option<u64>) -> Result<Option<i64>, ManagementError> {
    value
        .map(i64::try_from)
        .transpose()
        .map_err(|_| ManagementError::Internal)
}

fn map_service_error(error: RedemptionServiceError) -> ManagementError {
    match error {
        RedemptionServiceError::InvalidInput => ManagementError::InvalidRequest,
        RedemptionServiceError::Forbidden => ManagementError::Forbidden,
        RedemptionServiceError::InvalidSession => ManagementError::InvalidSession,
        RedemptionServiceError::BatchNotFound => ManagementError::RedemptionBatchNotFound,
        RedemptionServiceError::BatchConflict => ManagementError::RedemptionBatchConflict,
        RedemptionServiceError::CodeInvalid => ManagementError::RedemptionCodeInvalid,
        RedemptionServiceError::BatchDisabled => ManagementError::RedemptionBatchDisabled,
        RedemptionServiceError::CodeExpired => ManagementError::RedemptionCodeExpired,
        RedemptionServiceError::CodeAlreadyUsed => ManagementError::RedemptionCodeAlreadyUsed,
        RedemptionServiceError::BalanceOverflow => ManagementError::WalletOverflow,
        RedemptionServiceError::OutcomeUnknown => ManagementError::RedemptionOutcomeUnknown,
        RedemptionServiceError::Internal => ManagementError::Internal,
    }
}

fn status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_query_parser_accepts_closed_filters() {
        assert_eq!(
            parse_audit_query(None).unwrap(),
            AdminRedemptionAuditQuery::default()
        );
        let batch_id = "11111111111111111111111111111111";
        assert!(
            parse_audit_query(Some(&format!(
                "before=9&limit=50&batch_id={batch_id}&status=redeemed&redeemed_after=100&redeemed_before=200"
            )))
            .is_ok()
        );
    }

    #[test]
    fn audit_query_parser_rejects_ambiguous_inputs() {
        for raw_query in [
            "before=0",
            "limit=0",
            "limit=101",
            "status=all",
            "status=active&status=disabled",
            "batch_id=1111111111111111111111111111111g",
            "redeemed_after=200&redeemed_before=200",
            "redeemed_after=201&redeemed_before=200",
            "unknown=1",
            "before=%",
            "before=%ff",
        ] {
            assert_eq!(
                parse_audit_query(Some(raw_query)),
                Err(ManagementError::InvalidRequest),
                "{raw_query}"
            );
        }
    }
}
