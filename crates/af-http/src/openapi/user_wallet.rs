//! 当前用户钱包摘要与账本 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_wallet::{
        UserWalletEntryResponse, UserWalletEntryTypeResponse, UserWalletListResponse,
        UserWalletSummaryResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/account/wallet",
    operation_id = "getUserWallet",
    tag = "我的钱包",
    summary = "读取当前用户钱包",
    responses(
        (status = 200, description = "当前余额与额度状态", body = UserWalletSummaryResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_user_wallet() {}

#[utoipa::path(
    get,
    path = "/api/account/wallet/entries",
    operation_id = "listUserWalletEntries",
    tag = "我的钱包",
    summary = "读取当前用户钱包账本",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的账本 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 25")
    ),
    responses(
        (status = 200, description = "当前用户钱包账本", body = UserWalletListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_wallet_entries() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_user_wallet, list_user_wallet_entries),
    components(schemas(
        UserWalletSummaryResponse,
        UserWalletEntryTypeResponse,
        UserWalletEntryResponse,
        UserWalletListResponse,
        ManagementErrorBody
    ))
)]
struct UserWalletApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserWalletApi::openapi()
}
