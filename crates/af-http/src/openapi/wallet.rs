//! 管理端钱包调账与追加账本 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_wallet::{
        AdminWalletAdjustmentRequest, AdminWalletEntryResponse, AdminWalletEntryTypeResponse,
        AdminWalletListResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/users/{id}/wallet/entries",
    operation_id = "listAdminWalletEntries",
    tag = "钱包账本",
    summary = "读取用户钱包账本",
    params(
        ("id" = i64, Path, minimum = 1, description = "用户 ID"),
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的账本 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "钱包账本", body = AdminWalletListResponse),
        (status = 400, description = "用户 ID 或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "用户不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_wallet_entries() {}

#[utoipa::path(
    post,
    path = "/api/admin/users/{id}/wallet/adjustments",
    operation_id = "adjustAdminWallet",
    tag = "钱包账本",
    summary = "执行管理员增量调账",
    params(("id" = i64, Path, minimum = 1, description = "用户 ID")),
    request_body = AdminWalletAdjustmentRequest,
    responses(
        (status = 201, description = "新调账已提交", body = AdminWalletEntryResponse),
        (status = 200, description = "相同调账已存在", body = AdminWalletEntryResponse),
        (status = 400, description = "用户 ID、事件键、增量或原因无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "用户不存在", body = ManagementErrorBody),
        (status = 409, description = "事件冲突、余额不足或溢出", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知，必须复用同一事件键确认", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn adjust_admin_wallet() {}

#[derive(OpenApi)]
#[openapi(
    paths(list_admin_wallet_entries, adjust_admin_wallet),
    components(schemas(
        AdminWalletEntryTypeResponse,
        AdminWalletEntryResponse,
        AdminWalletListResponse,
        AdminWalletAdjustmentRequest,
        ManagementErrorBody
    ))
)]
struct WalletApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    WalletApi::openapi()
}
