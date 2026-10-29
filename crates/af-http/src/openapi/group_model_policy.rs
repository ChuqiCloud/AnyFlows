//! 分组可调用模型限制管理契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    group_model_policy::{AdminGroupModelPolicyResponse, AdminGroupModelPolicyWriteRequest},
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/admin/groups/{id}/model-policy",
    operation_id = "getAdminGroupModelPolicy",
    tag = "分组管理",
    summary = "读取分组可调用模型白名单",
    params(("id" = i64, Path, minimum = 1, description = "分组 ID")),
    responses(
        (status = 200, description = "分组当前模型限制；空值表示不限制", body = AdminGroupModelPolicyResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "分组不存在或已删除", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_group_model_policy() {}

#[utoipa::path(
    put,
    path = "/api/admin/groups/{id}/model-policy",
    operation_id = "updateAdminGroupModelPolicy",
    tag = "分组管理",
    summary = "整体替换分组可调用模型白名单",
    description = "白名单与 Key 自身白名单取交集。将 allowed_models 设为 null 清除限制；非空数组按 Canonical 模型名区分大小写精确匹配。",
    params(("id" = i64, Path, minimum = 1, description = "分组 ID")),
    request_body = AdminGroupModelPolicyWriteRequest,
    responses(
        (status = 200, description = "模型白名单已保存", body = AdminGroupModelPolicyResponse),
        (status = 400, description = "模型白名单或分组 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "分组不存在或已删除", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_group_model_policy() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_admin_group_model_policy, update_admin_group_model_policy),
    components(schemas(
        AdminGroupModelPolicyResponse,
        AdminGroupModelPolicyWriteRequest,
        ManagementErrorBody
    ))
)]
struct GroupModelPolicyApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    GroupModelPolicyApi::openapi()
}
