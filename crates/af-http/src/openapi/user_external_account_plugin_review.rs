//! Session-scoped plugin analysis of stored external balance snapshots.

#![allow(dead_code, reason = "documentation endpoints are consumed by utoipa")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_external_account_plugin_review::{
        ReviewCheckResponse, ReviewRequest, ReviewResponse, ReviewStatusResponse,
        ReviewerListResponse, ReviewerResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/account/external-accounts/plugin-reviewers",
    operation_id = "listUserExternalAccountSnapshotReviewers",
    tag = "个人资料与安全",
    summary = "列出可用于余额快照分析的插件",
    responses(
        (status = 200, description = "当前启用并明确获授只读分析能力的插件", body = ReviewerListResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 503, description = "插件目录暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_external_account_snapshot_reviewers() {}

#[utoipa::path(
    post,
    path = "/api/account/external-accounts/{id}/plugin-review",
    operation_id = "reviewUserExternalAccountBalanceSnapshots",
    tag = "个人资料与安全",
    summary = "使用所选插件分析已存余额快照",
    params(("id" = i64, Path, minimum = 1, description = "当前用户外部账户连接 ID")),
    request_body = ReviewRequest,
    responses(
        (status = 200, description = "插件提供的只读分析建议", body = ReviewResponse),
        (status = 400, description = "请求无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "插件未获授该分析能力", body = ManagementErrorBody),
        (status = 404, description = "账户或余额快照不存在", body = ManagementErrorBody),
        (status = 409, description = "账户或插件部署在分析期间发生变化", body = ManagementErrorBody),
        (status = 503, description = "插件分析暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn review_user_external_account_balance_snapshots() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_user_external_account_snapshot_reviewers,
        review_user_external_account_balance_snapshots
    ),
    components(schemas(
        ReviewRequest,
        ReviewerResponse,
        ReviewerListResponse,
        ReviewStatusResponse,
        ReviewCheckResponse,
        ReviewResponse,
        ManagementErrorBody
    ))
)]
struct UserExternalAccountPluginReviewApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserExternalAccountPluginReviewApi::openapi()
}
