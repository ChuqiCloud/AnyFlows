//! Session-scoped external account linking and balance observation contract.

#![allow(dead_code, reason = "documentation endpoints are consumed by utoipa")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_external_accounts::{
        AccountListResponse, AccountResponse, BalanceHistoryQuery, BalanceHistoryResponse,
        BalanceResponse, ConnectorResponse, LinkRequest,
    },
};

#[utoipa::path(
    get,
    path = "/api/account/external-connectors",
    operation_id = "listUserExternalConnectors",
    tag = "个人资料与安全",
    summary = "列出可绑定的外部账户连接器",
    responses(
        (status = 200, description = "内置只读连接器目录", body = [ConnectorResponse]),
        (status = 401, description = "会话无效", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_external_connectors() {}

#[utoipa::path(
    get,
    path = "/api/account/external-accounts",
    operation_id = "listUserExternalAccounts",
    tag = "个人资料与安全",
    summary = "读取当前用户绑定的外部账户",
    responses(
        (status = 200, description = "当前用户外部账户绑定", body = AccountListResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 503, description = "外部账户存储暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_external_accounts() {}

#[utoipa::path(
    post,
    path = "/api/account/external-accounts",
    operation_id = "linkUserExternalAccount",
    tag = "个人资料与安全",
    summary = "验证并绑定外部平台账户",
    request_body = LinkRequest,
    responses(
        (status = 200, description = "外部账户已验证并绑定", body = AccountResponse),
        (status = 400, description = "请求字段无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效或外部凭据被拒绝", body = ManagementErrorBody),
        (status = 409, description = "外部账户已绑定", body = ManagementErrorBody),
        (status = 422, description = "连接器或能力不受支持", body = ManagementErrorBody),
        (status = 429, description = "平台连接器繁忙或已达到绑定上限", body = ManagementErrorBody),
        (status = 503, description = "外部平台暂时不可用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn link_user_external_account() {}

#[utoipa::path(
    post,
    path = "/api/account/external-accounts/{id}/refresh",
    operation_id = "refreshUserExternalAccount",
    tag = "个人资料与安全",
    summary = "刷新外部账户资料和余额快照",
    params(("id" = i64, Path, minimum = 1, description = "当前用户外部账户连接 ID")),
    responses(
        (status = 200, description = "新的外部余额快照", body = AccountResponse),
        (status = 401, description = "会话无效或外部凭据已失效", body = ManagementErrorBody),
        (status = 404, description = "外部账户不存在", body = ManagementErrorBody),
        (status = 429, description = "平台连接器当前繁忙", body = ManagementErrorBody),
        (status = 503, description = "外部平台暂时不可用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn refresh_user_external_account() {}

#[utoipa::path(
    delete,
    path = "/api/account/external-accounts/{id}",
    operation_id = "unlinkUserExternalAccount",
    tag = "个人资料与安全",
    summary = "解除当前用户的外部账户绑定",
    params(("id" = i64, Path, minimum = 1, description = "当前用户外部账户连接 ID")),
    responses(
        (status = 204, description = "外部账户已解除绑定"),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 404, description = "外部账户不存在", body = ManagementErrorBody),
        (status = 503, description = "外部账户存储暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn unlink_user_external_account() {}

#[utoipa::path(
    get,
    path = "/api/account/external-accounts/{id}/balances",
    operation_id = "listUserExternalAccountBalances",
    tag = "个人资料与安全",
    summary = "读取外部平台原生单位余额历史",
    params(
        ("id" = i64, Path, minimum = 1, description = "当前用户外部账户连接 ID"),
        ("limit" = Option<u32>, Query, minimum = 1, maximum = 100, description = "返回记录数，默认 20，最大 100")
    ),
    responses(
        (status = 200, description = "余额观测历史，不影响 AnyFlows 钱包", body = BalanceHistoryResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 404, description = "外部账户不存在", body = ManagementErrorBody),
        (status = 503, description = "外部账户存储暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_external_account_balances() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_user_external_connectors,
        list_user_external_accounts,
        link_user_external_account,
        refresh_user_external_account,
        unlink_user_external_account,
        list_user_external_account_balances
    ),
    components(schemas(
        ConnectorResponse,
        AccountResponse,
        AccountListResponse,
        LinkRequest,
        BalanceResponse,
        BalanceHistoryQuery,
        BalanceHistoryResponse,
        ManagementErrorBody
    ))
)]
struct UserExternalAccountsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserExternalAccountsApi::openapi()
}
