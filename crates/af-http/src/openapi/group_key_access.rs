//! 用户 Key 可选分组与管理员授权策略契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    group_key_access::{
        AdminGroupKeyPolicyResponse, AdminGroupKeyPolicyWriteRequest, GroupKeySelectionMode,
        UserTokenGroupListResponse, UserTokenGroupResponse,
    },
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/token-groups",
    operation_id = "listUserTokenGroups",
    tag = "我的 API Key",
    summary = "读取当前用户创建 Key 时可选的分组",
    description = "包含用户默认组及策略允许的分组。倍率是目标组基础倍率，实际费用还受来源组附加倍率、模型价格与高峰倍率影响。列表不是授权凭证，签发 Key 时服务端会再次校验。",
    params(
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页末尾的分组 ID"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "当前用户可选的分组", body = UserTokenGroupListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话或用户状态无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_user_token_groups() {}

#[utoipa::path(
    get,
    path = "/api/admin/groups/{id}/key-policy",
    operation_id = "getAdminGroupKeyPolicy",
    tag = "分组管理",
    summary = "读取分组的用户 Key 自助选择策略",
    params(("id" = i64, Path, minimum = 1, description = "分组 ID")),
    responses(
        (status = 200, description = "分组自助策略与指定用户授权", body = AdminGroupKeyPolicyResponse),
        (status = 400, description = "分组 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "分组不存在或已删除", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_group_key_policy() {}

#[utoipa::path(
    put,
    path = "/api/admin/groups/{id}/key-policy",
    operation_id = "updateAdminGroupKeyPolicy",
    tag = "分组管理",
    summary = "设置分组的用户 Key 自助选择策略",
    description = "整体替换策略及指定用户授权。用户默认组始终可用，策略变更不改写已有 Key 的分组。只有 selected_users 模式接受非空授权列表。",
    params(("id" = i64, Path, minimum = 1, description = "分组 ID")),
    request_body = AdminGroupKeyPolicyWriteRequest,
    responses(
        (status = 200, description = "策略与授权已保存", body = AdminGroupKeyPolicyResponse),
        (status = 400, description = "策略、用户授权或分组 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "分组不存在或已删除", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_group_key_policy() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_user_token_groups,
        get_admin_group_key_policy,
        update_admin_group_key_policy
    ),
    components(schemas(
        GroupKeySelectionMode,
        AdminGroupKeyPolicyResponse,
        AdminGroupKeyPolicyWriteRequest,
        UserTokenGroupResponse,
        UserTokenGroupListResponse,
        ManagementErrorBody
    ))
)]
struct GroupKeyAccessApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    GroupKeyAccessApi::openapi()
}
