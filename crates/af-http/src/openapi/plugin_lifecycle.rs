//! Plugin artifact and deployment lifecycle contract.
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    plugin_lifecycle::{
        PluginActionResponse, PluginArtifactApprovalResponse, PluginArtifactCleanupResponse,
        PluginArtifactPageResponse, PluginArtifactRegistrationRequest, PluginArtifactResponse,
        PluginArtifactRevocationResponse, PluginArtifactRevokeRequest,
        PluginCapabilityGrantRequest, PluginCapabilityGrantResponse,
        PluginCatalogModelPageResponse, PluginCatalogModelResponse,
        PluginCatalogSourcePageResponse, PluginCatalogSourceResponse,
        PluginDeploymentDisableRequest, PluginDeploymentPageResponse, PluginDeploymentRequest,
        PluginDeploymentResponse, PluginDeploymentRollbackRequest, PluginPackageInstallRequest,
        PluginRuntimeNodeResponse, PluginRuntimeOverviewResponse, PluginRuntimeStateResponse,
        PluginUninstallResponse,
    },
};

#[utoipa::path(
    post,
    path = "/api/admin/plugins/install",
    operation_id = "installAdminPluginPackage",
    tag = "插件生命周期",
    summary = "上传插件包并安装启用；自有插件无需签名",
    request_body(content = PluginPackageInstallRequest, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "已安装并启用", body = PluginDeploymentResponse),
        (status = 400, description = "插件包、配置或签名无效", body = ManagementErrorBody),
        (status = 422, description = "插件需要在高级部署中显式授权能力", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "缺少插件管理权限", body = ManagementErrorBody),
        (status = 409, description = "已安装或部署代次冲突", body = ManagementErrorBody),
        (status = 413, description = "请求或压缩包超过大小上限", body = ManagementErrorBody),
        (status = 503, description = "安装并发已满或插件运行时未启用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn install_admin_plugin_package() {}

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

#[utoipa::path(
    get,
    path = "/api/admin/plugins/runtime",
    operation_id = "getAdminPluginRuntimeOverview",
    tag = "插件生命周期",
    summary = "读取插件分发与逐节点真实运行状态",
    responses(
        (status = 200, description = "节点心跳、代次与运行状态；启用状态不等于所有节点就绪", body = PluginRuntimeOverviewResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "缺少插件管理权限", body = ManagementErrorBody),
        (status = 500, description = "内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_plugin_runtime_overview() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/{plugin_id}/uninstall",
    operation_id = "uninstallAdminPlugin",
    tag = "插件生命周期",
    summary = "停止插件并安排各节点回收运行文件，保留部署历史",
    params(("plugin_id" = String, Path)),
    request_body = PluginDeploymentDisableRequest,
    responses(
        (status = 200, description = "卸载已受理；其他节点通过协调完成清理", body = PluginUninstallResponse),
        (status = 400, description = "请求无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "缺少插件管理权限", body = ManagementErrorBody),
        (status = 404, description = "插件不存在", body = ManagementErrorBody),
        (status = 409, description = "部署代次已变更", body = ManagementErrorBody),
        (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn uninstall_admin_plugin() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/artifacts/{artifact_id}/cleanup",
    operation_id = "cleanupAdminPluginArtifact",
    tag = "插件生命周期",
    summary = "回收已撤销且不再启用的插件归档与本机文件",
    params(("artifact_id" = i64, Path, minimum = 1)),
    responses(
        (status = 200, description = "制品内容清理结果；审计与历史元数据仍保留", body = PluginArtifactCleanupResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "缺少插件管理权限", body = ManagementErrorBody),
        (status = 404, description = "制品不存在", body = ManagementErrorBody),
        (status = 409, description = "制品未撤销或仍被启用部署引用", body = ManagementErrorBody),
        (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn cleanup_admin_plugin_artifact() {}

#[utoipa::path(
    get,
    path = "/api/account/plugin-models/sources",
    operation_id = "listPluginModelCatalogSources",
    tag = "模型目录",
    summary = "读取已启用并获得公开目录能力授权的插件来源",
    params(("cursor" = Option<String>, Query, max_length = 128)),
    responses(
        (status = 200, description = "公开模型资料来源；不含配置与凭据", body = PluginCatalogSourcePageResponse),
        (status = 400, description = "游标无效", body = ManagementErrorBody),
        (status = 401, description = "需要登录", body = ManagementErrorBody),
        (status = 500, description = "内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_plugin_model_catalog_sources() {}

#[utoipa::path(
    get,
    path = "/api/account/plugin-models/{plugin_id}",
    operation_id = "listPluginCatalogModels",
    tag = "模型目录",
    summary = "查询插件提供的模型资料目录",
    description = "仅调用已授权的 catalog.models.read/search v1。结果为参考资料，不创建平台模型、价格、调用权限或计费规则。",
    params(
        ("plugin_id" = String, Path),
        ("q" = Option<String>, Query, max_length = 128),
        ("cursor" = Option<String>, Query, max_length = 2048),
        ("limit" = Option<u32>, Query, minimum = 1, maximum = 50)
    ),
    responses(
        (status = 200, description = "经过宿主校验的模型资料页", body = PluginCatalogModelPageResponse),
        (status = 400, description = "查询参数无效", body = ManagementErrorBody),
        (status = 401, description = "需要登录", body = ManagementErrorBody),
        (status = 403, description = "插件未启用或目录能力未获授权", body = ManagementErrorBody),
        (status = 404, description = "来源不存在", body = ManagementErrorBody),
        (status = 409, description = "插件部署已变更，请重新查询", body = ManagementErrorBody),
        (status = 503, description = "查询繁忙或插件响应无效", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_plugin_catalog_models() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        install_admin_plugin_package,
        list_admin_plugin_artifacts,
        list_admin_plugin_deployments,
        list_admin_plugin_deployment_history,
        register_admin_plugin_artifact,
        approve_admin_plugin_artifact,
        revoke_admin_plugin_artifact,
        deploy_admin_plugin_artifact,
        disable_admin_plugin_deployment,
        rollback_admin_plugin_deployment,
        get_admin_plugin_runtime_overview,
        uninstall_admin_plugin,
        cleanup_admin_plugin_artifact,
        list_plugin_model_catalog_sources,
        list_plugin_catalog_models
    ),
    components(schemas(
        PluginPackageInstallRequest,
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
        PluginRuntimeOverviewResponse,
        PluginRuntimeNodeResponse,
        PluginRuntimeStateResponse,
        PluginUninstallResponse,
        PluginArtifactCleanupResponse,
        PluginCatalogSourceResponse,
        PluginCatalogSourcePageResponse,
        PluginCatalogModelResponse,
        PluginCatalogModelPageResponse,
        ManagementErrorBody
    ))
)]
struct PluginLifecycleApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PluginLifecycleApi::openapi()
}
