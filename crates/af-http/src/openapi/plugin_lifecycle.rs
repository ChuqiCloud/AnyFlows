//! Plugin artifact and deployment lifecycle contract.
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    plugin_lifecycle::{
        PluginActionResponse, PluginArtifactApprovalResponse, PluginArtifactPageResponse,
        PluginArtifactRegistrationRequest, PluginArtifactResponse,
        PluginArtifactRevocationResponse, PluginArtifactRevokeRequest,
        PluginCapabilityGrantRequest, PluginCapabilityGrantResponse,
        PluginDeploymentDisableRequest, PluginDeploymentPageResponse, PluginDeploymentRequest,
        PluginDeploymentResponse, PluginDeploymentRollbackRequest,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/plugins/artifacts",
    operation_id = "listAdminPluginArtifacts",
    tag = "插件生命周期",
    summary = "列出已验证与待审批的插件制品",
    params(("before_id" = Option<i64>, Query, minimum = 1), ("limit" = Option<u32>, Query, minimum = 1, maximum = 100)),
    responses(
        (status = 200, description = "插件制品安全投影；不包含本地路径或签名", body = PluginArtifactPageResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_plugin_artifacts() {}

#[utoipa::path(
    get,
    path = "/api/admin/plugins/deployments",
    operation_id = "listAdminPluginDeployments",
    tag = "插件生命周期",
    summary = "列出当前插件部署状态",
    params(("cursor" = Option<String>, Query), ("limit" = Option<u32>, Query, minimum = 1, maximum = 100)),
    responses(
        (status = 200, description = "插件部署安全投影；不包含插件配置", body = PluginDeploymentPageResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_plugin_deployments() {}

#[utoipa::path(
    get,
    path = "/api/admin/plugins/{plugin_id}/deployment/history",
    operation_id = "listAdminPluginDeploymentHistory",
    tag = "插件生命周期",
    summary = "读取插件不可变部署历史",
    params(("plugin_id" = String, Path), ("limit" = Option<u32>, Query, minimum = 1, maximum = 100)),
    responses(
        (status = 200, description = "部署历史；不包含插件配置", body = [PluginDeploymentResponse]),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 400, description = "插件标识或分页参数无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_plugin_deployment_history() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/artifacts",
    operation_id = "registerAdminVerifiedPluginArtifact",
    tag = "插件生命周期",
    summary = "登记并验签 artifact root 下的插件制品",
    description = "只接受相对于服务器配置制品根目录的路径；信任公钥来自服务器配置。服务端会校验签名、包 digest 和清单身份。",
    request_body = PluginArtifactRegistrationRequest,
    responses(
        (status = 200, description = "已登记的制品安全投影", body = PluginArtifactResponse),
        (status = 400, description = "相对路径、清单、签名或 digest 无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 409, description = "同一制品身份冲突", body = ManagementErrorBody),
        (status = 503, description = "制品根目录或可信签名者未配置", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn register_admin_plugin_artifact() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/artifacts/{artifact_id}/approve",
    operation_id = "approveAdminPluginArtifact",
    tag = "插件生命周期",
    summary = "批准已登记的插件制品",
    params(("artifact_id" = i64, Path, minimum = 1)),
    responses(
        (status = 200, description = "审批完成", body = PluginActionResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "制品不存在", body = ManagementErrorBody),
        (status = 409, description = "制品状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn approve_admin_plugin_artifact() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/artifacts/{artifact_id}/revoke",
    operation_id = "revokeAdminPluginArtifact",
    tag = "插件生命周期",
    summary = "撤销插件制品审批并停用其当前部署",
    params(("artifact_id" = i64, Path, minimum = 1)),
    request_body = PluginArtifactRevokeRequest,
    responses(
        (status = 200, description = "撤销完成", body = PluginActionResponse),
        (status = 400, description = "撤销原因无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "制品不存在", body = ManagementErrorBody),
        (status = 409, description = "制品状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn revoke_admin_plugin_artifact() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/{plugin_id}/deployment",
    operation_id = "deployAdminPluginArtifact",
    tag = "插件生命周期",
    summary = "创建或推进插件部署 generation",
    params(("plugin_id" = String, Path)),
    request_body = PluginDeploymentRequest,
    responses(
        (status = 200, description = "新部署状态", body = PluginDeploymentResponse),
        (status = 400, description = "部署配置或授权能力无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "制品不存在", body = ManagementErrorBody),
        (status = 409, description = "generation 或审批状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn deploy_admin_plugin_artifact() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/{plugin_id}/disable",
    operation_id = "disableAdminPluginDeployment",
    tag = "插件生命周期",
    summary = "按预期 generation 停用插件部署",
    params(("plugin_id" = String, Path)),
    request_body = PluginDeploymentDisableRequest,
    responses(
        (status = 200, description = "停用后的部署状态", body = PluginDeploymentResponse),
        (status = 400, description = "请求无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "部署不存在", body = ManagementErrorBody),
        (status = 409, description = "generation 冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn disable_admin_plugin_deployment() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/{plugin_id}/rollback",
    operation_id = "rollbackAdminPluginDeployment",
    tag = "插件生命周期",
    summary = "将历史 generation 作为新的部署 generation 恢复",
    params(("plugin_id" = String, Path)),
    request_body = PluginDeploymentRollbackRequest,
    responses(
        (status = 200, description = "恢复后的部署状态", body = PluginDeploymentResponse),
        (status = 400, description = "generation 无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "历史部署不存在", body = ManagementErrorBody),
        (status = 409, description = "generation 或审批状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn rollback_admin_plugin_deployment() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_plugin_artifacts,
        list_admin_plugin_deployments,
        list_admin_plugin_deployment_history,
        register_admin_plugin_artifact,
        approve_admin_plugin_artifact,
        revoke_admin_plugin_artifact,
        deploy_admin_plugin_artifact,
        disable_admin_plugin_deployment,
        rollback_admin_plugin_deployment
    ),
    components(schemas(
        PluginActionResponse,
        PluginArtifactApprovalResponse,
        PluginArtifactPageResponse,
        PluginArtifactRegistrationRequest,
        PluginArtifactResponse,
        PluginArtifactRevocationResponse,
        PluginArtifactRevokeRequest,
        PluginCapabilityGrantRequest,
        PluginCapabilityGrantResponse,
        PluginDeploymentDisableRequest,
        PluginDeploymentPageResponse,
        PluginDeploymentRequest,
        PluginDeploymentResponse,
        PluginDeploymentRollbackRequest,
        ManagementErrorBody
    ))
)]
struct PluginLifecycleApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PluginLifecycleApi::openapi()
}
