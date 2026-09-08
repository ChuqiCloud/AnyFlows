//! 兑换码批次管理与当前用户兑换 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    redemption::{
        AdminRedemptionAuditBatchResponse, AdminRedemptionAuditListResponse,
        AdminRedemptionAuditStatusDto, AdminRedemptionAuditSummaryResponse,
        AdminRedemptionBatchCreateRequest, AdminRedemptionBatchDisableRequest,
        AdminRedemptionBatchDisableResponse, AdminRedemptionBatchListResponse,
        AdminRedemptionBatchResponse, AdminRedemptionBatchStatusDto,
        IssuedAdminRedemptionBatchResponse, UserRedemptionRequest, UserRedemptionResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/redemption-audit",
    operation_id = "listAdminRedemptionAudit",
    tag = "兑换码",
    summary = "读取兑换码运营审计报表",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的批次主键游标"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页批次数量，默认 25"),
        ("batch_id" = Option<String>, Query, min_length = 32, max_length = 32, description = "精确批次标识"),
        ("status" = Option<AdminRedemptionAuditStatusDto>, Query, description = "有效批次状态或至少存在兑换事实"),
        ("redeemed_after" = Option<i64>, Query, minimum = 1, description = "兑换时间窗口起点（Unix 秒，包含）"),
        ("redeemed_before" = Option<i64>, Query, minimum = 1, description = "兑换时间窗口终点（Unix 秒，不包含）")
    ),
    responses(
        (status = 200, description = "当前结果页的批次审计统计", body = AdminRedemptionAuditListResponse),
        (status = 400, description = "筛选、时间窗口或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_redemption_audit() {}

#[utoipa::path(
    get,
    path = "/api/admin/redemption-batches",
    operation_id = "listAdminRedemptionBatches",
    tag = "兑换码",
    summary = "读取兑换码批次",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的批次游标"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 25")
    ),
    responses(
        (status = 200, description = "兑换码批次汇总", body = AdminRedemptionBatchListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_redemption_batches() {}

#[utoipa::path(
    post,
    path = "/api/admin/redemption-batches",
    operation_id = "createAdminRedemptionBatch",
    tag = "兑换码",
    summary = "创建兑换码批次",
    request_body = AdminRedemptionBatchCreateRequest,
    responses(
        (status = 201, description = "批次已创建，完整兑换码仅返回一次", body = IssuedAdminRedemptionBatchResponse<'static>),
        (status = 400, description = "批次名称、额度、数量或到期时间无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 409, description = "批次事实发生冲突", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_redemption_batch() {}

#[utoipa::path(
    post,
    path = "/api/admin/redemption-batches/{batch_id}/disable",
    operation_id = "disableAdminRedemptionBatch",
    tag = "兑换码",
    summary = "禁用兑换码批次",
    params(("batch_id" = String, Path, min_length = 32, max_length = 32, description = "批次标识")),
    request_body = AdminRedemptionBatchDisableRequest,
    responses(
        (status = 200, description = "批次已禁用或相同迁移已存在", body = AdminRedemptionBatchDisableResponse),
        (status = 400, description = "批次标识或版本无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "批次不存在", body = ManagementErrorBody),
        (status = 409, description = "批次版本冲突", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn disable_admin_redemption_batch() {}

#[utoipa::path(
    post,
    path = "/api/account/wallet/redemptions",
    operation_id = "redeemUserRedemptionCode",
    tag = "兑换码",
    summary = "兑换当前用户额度",
    request_body = UserRedemptionRequest,
    responses(
        (status = 200, description = "兑换成功或恢复本人已到账事实", body = UserRedemptionResponse),
        (status = 400, description = "兑换码格式无效或不存在", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "批次停用、兑换码已使用或余额溢出", body = ManagementErrorBody),
        (status = 410, description = "兑换码已过期", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知，可重试同一兑换码确认", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn redeem_user_redemption_code() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_redemption_audit,
        list_admin_redemption_batches,
        create_admin_redemption_batch,
        disable_admin_redemption_batch,
        redeem_user_redemption_code
    ),
    components(schemas(
        AdminRedemptionAuditStatusDto,
        AdminRedemptionAuditBatchResponse,
        AdminRedemptionAuditSummaryResponse,
        AdminRedemptionAuditListResponse,
        AdminRedemptionBatchStatusDto,
        AdminRedemptionBatchResponse,
        AdminRedemptionBatchListResponse,
        AdminRedemptionBatchCreateRequest,
        IssuedAdminRedemptionBatchResponse<'static>,
        AdminRedemptionBatchDisableRequest,
        AdminRedemptionBatchDisableResponse,
        UserRedemptionRequest,
        UserRedemptionResponse,
        ManagementErrorBody
    ))
)]
struct RedemptionsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    RedemptionsApi::openapi()
}
