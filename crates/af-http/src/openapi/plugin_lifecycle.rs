//! Plugin artifact and deployment lifecycle contract.
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    plugin_lifecycle::{
        AdminPluginTaskListQuery, AdminPluginTaskReconciliationRequest, ModelPluginTaskRequest,
        PluginActionResponse, PluginArtifactApprovalResponse, PluginArtifactCleanupResponse,
        PluginArtifactPageResponse, PluginArtifactRegistrationRequest, PluginArtifactResponse,
        PluginArtifactRevocationResponse, PluginArtifactRevokeRequest,
        PluginCapabilityActionConfirmRequest, PluginCapabilityActionProposalRequest,
        PluginCapabilityActionProposalResponse, PluginCapabilityActionResultResponse,
        PluginCapabilityGrantRequest, PluginCapabilityGrantResponse,
        PluginCapabilityInvocationRequest, PluginCapabilityInvocationResponse,
        PluginCatalogModelPageResponse, PluginCatalogModelResponse,
        PluginCatalogSourcePageResponse, PluginCatalogSourceResponse,
        PluginCredentialInvocationRequest, PluginCredentialInvocationResponse,
        PluginDeploymentDisableRequest, PluginDeploymentPageResponse, PluginDeploymentRequest,
        PluginDeploymentResponse, PluginDeploymentRollbackRequest, PluginEventResultPageResponse,
        PluginEventResultResponse, PluginMarketEntryResponse, PluginMarketInstallHttpRequest,
        PluginMarketPageResponse, PluginMarketSourceRequest, PluginPackageInstallRequest,
        PluginProviderSelectionBody, PluginProviderSelectionRequest,
        PluginPublicConfigurationResponse, PluginPublicConfigurationUpdateRequest,
        PluginRouteInvocationResponse, PluginRuntimeNodeResponse, PluginRuntimeOverviewResponse,
        PluginRuntimeStateResponse, PluginTaskInvocationRequest, PluginTaskInvocationResponse,
        PluginUninstallResponse,
    },
};

#[utoipa::path(
    post,
    path = "/api/admin/plugins/install",
    operation_id = "installAdminPluginPackage",
    tag = "插件生命周期",
    summary = "上传插件包或从 GitHub 仓库安装",
    request_body(content = PluginPackageInstallRequest, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "已安装并启用", body = PluginDeploymentResponse),
        (status = 400, description = "安装来源、插件包、配置或签名无效", body = ManagementErrorBody),
        (status = 404, description = "仓库、正式 Release 或插件包不存在", body = ManagementErrorBody),
        (status = 422, description = "插件需要在高级部署中显式授权能力", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "缺少插件管理权限", body = ManagementErrorBody),
        (status = 409, description = "已安装或部署代次冲突", body = ManagementErrorBody),
        (status = 413, description = "请求或压缩包超过大小上限", body = ManagementErrorBody),
        (status = 503, description = "安装并发已满、GitHub 暂不可用或插件运行时未启用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn install_admin_plugin_package() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/market/install",
    operation_id = "installAdminMarketPlugin",
    tag = "插件市场",
    summary = "从已验证的市场来源下载并安装插件",
    request_body = PluginMarketInstallHttpRequest,
    responses(
        (status = 200, description = "已下载、校验并安装启用", body = PluginDeploymentResponse),
        (status = 400, description = "来源、摘要、插件身份或配置无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "缺少插件管理权限", body = ManagementErrorBody),
        (status = 409, description = "插件身份或部署代次冲突", body = ManagementErrorBody),
        (status = 413, description = "远程插件包超过大小上限", body = ManagementErrorBody),
        (status = 503, description = "市场下载器或插件运行时不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn install_admin_market_plugin() {}

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
    post,
    path = "/api/admin/plugins/{plugin_id}/capabilities/{capability_id}/invoke",
    operation_id = "invokeAdminPluginCapability",
    tag = "插件生命周期",
    summary = "调用插件自定义的已授权只读能力",
    description = "只允许调用清单中声明了输入输出 Schema、access=read、sideEffect=none 且已授予当前部署的能力。外部读取、写入、任务提交和凭据访问仍走专用宿主入口。",
    params(
        ("plugin_id" = String, Path),
        ("capability_id" = String, Path)
    ),
    request_body = PluginCapabilityInvocationRequest,
    responses(
        (status = 200, description = "插件能力输出", body = PluginCapabilityInvocationResponse),
        (status = 400, description = "能力标识、版本或输入无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户、插件部署或能力授权不满足要求", body = ManagementErrorBody),
        (status = 404, description = "插件部署不存在", body = ManagementErrorBody),
        (status = 409, description = "插件部署代次在调用期间发生变化", body = ManagementErrorBody),
        (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn invoke_admin_plugin_capability() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/{plugin_id}/credential-lifecycle",
    operation_id = "invokeAdminPluginCredentialLifecycle",
    tag = "插件生命周期",
    summary = "执行插件凭据登录、刷新、撤销、签名或余额操作",
    params(("plugin_id" = String, Path)),
    request_body = PluginCredentialInvocationRequest,
    responses((status = 200, description = "凭据生命周期结果", body = PluginCredentialInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "插件或能力未授权", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_admin_plugin_credential_lifecycle() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/{plugin_id}/tasks",
    operation_id = "invokeAdminPluginTask",
    tag = "插件任务",
    summary = "提交、轮询、取消或读取插件任务",
    params(("plugin_id" = String, Path)),
    request_body = PluginTaskInvocationRequest,
    responses((status = 200, description = "任务状态或结果", body = PluginTaskInvocationResponse), (status = 400, description = "任务请求无效", body = ManagementErrorBody), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "插件或能力未授权", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_admin_plugin_task() {}

#[utoipa::path(
    get,
    path = "/api/admin/plugins/tasks/unknown",
    operation_id = "listAdminUnknownPluginTasks",
    tag = "插件任务",
    summary = "读取需要人工核对上游状态的插件任务",
    params(("cursor" = Option<String>, Query), ("limit" = Option<u16>, Query, minimum = 1, maximum = 100)),
    responses(
        (status = 200, description = "未知任务及分页游标", body = serde_json::Value),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_unknown_plugin_tasks() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/tasks/{task_id}/reconcile",
    operation_id = "reconcileAdminUnknownPluginTask",
    tag = "插件任务",
    summary = "依据上游核对结果结算未知插件任务并释放剩余额度",
    description = "管理员必须先在上游确认本次任务的实际计费单位。填 0 表示上游确认未执行。该操作会写入管理员、原因和结算额度审计信息。",
    params(("task_id" = String, Path)),
    request_body = AdminPluginTaskReconciliationRequest,
    responses(
        (status = 200, description = "任务已结算", body = serde_json::Value),
        (status = 400, description = "实际单位或原因无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "插件任务不存在", body = ManagementErrorBody),
        (status = 409, description = "任务不再处于未知状态", body = ManagementErrorBody),
        (status = 500, description = "结算或审计记录失败", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn reconcile_admin_unknown_plugin_task() {}

#[utoipa::path(
    get,
    path = "/api/admin/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeAdminPluginRouteGet",
    tag = "插件生命周期",
    summary = "调用插件声明的管理员 GET 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    responses(
        (status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户或路由授权不满足要求", body = ManagementErrorBody),
        (status = 404, description = "插件或路由不存在", body = ManagementErrorBody),
        (status = 409, description = "插件部署代次已变更", body = ManagementErrorBody),
        (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn invoke_admin_plugin_route_get() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeAdminPluginRoutePost",
    tag = "插件生命周期",
    summary = "调用插件声明的管理员 POST 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 409, description = "插件部署代次已变更", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_admin_plugin_route_post() {}

#[utoipa::path(
    get,
    path = "/api/admin/plugins/{plugin_id}/configuration",
    operation_id = "getAdminPluginConfiguration",
    tag = "插件生命周期",
    summary = "读取插件公开配置供设置表单编辑",
    params(("plugin_id" = String, Path)),
    responses(
        (status = 200, description = "当前部署代次与敏感字段过滤后的插件公开配置", body = PluginPublicConfigurationResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "缺少插件管理权限", body = ManagementErrorBody),
        (status = 404, description = "插件部署不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []), ("personalAccessTokenBearer" = [])),
    extensions(("x-anyflows-pat-scopes" = json!( ["admin:get_admin_plugin_configuration:execute"] )))
)]
fn get_admin_plugin_configuration() {}

#[utoipa::path(
    put,
    path = "/api/admin/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeAdminPluginRoutePut",
    tag = "插件生命周期",
    summary = "调用插件声明的管理员 PUT 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 409, description = "插件部署代次已变更", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_admin_plugin_route_put() {}

#[utoipa::path(
    patch,
    path = "/api/admin/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeAdminPluginRoutePatch",
    tag = "插件生命周期",
    summary = "调用插件声明的管理员 PATCH 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 409, description = "插件部署代次已变更", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_admin_plugin_route_patch() {}

#[utoipa::path(
    delete,
    path = "/api/admin/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeAdminPluginRouteDelete",
    tag = "插件生命周期",
    summary = "调用插件声明的管理员 DELETE 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 409, description = "插件部署代次已变更", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_admin_plugin_route_delete() {}

#[utoipa::path(
    get,
    path = "/api/account/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeUserPluginRouteGet",
    tag = "插件路由",
    summary = "调用插件声明的普通用户 GET 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 404, description = "插件或路由不存在", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_user_plugin_route_get() {}

#[utoipa::path(
    post,
    path = "/api/account/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeUserPluginRoutePost",
    tag = "插件路由",
    summary = "调用插件声明的普通用户 POST 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    request_body = PluginCapabilityInvocationRequest,
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_user_plugin_route_post() {}

#[utoipa::path(
    put,
    path = "/api/account/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeUserPluginRoutePut",
    tag = "插件路由",
    summary = "调用插件声明的普通用户 PUT 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    request_body = PluginCapabilityInvocationRequest,
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_user_plugin_route_put() {}

#[utoipa::path(
    patch,
    path = "/api/account/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeUserPluginRoutePatch",
    tag = "插件路由",
    summary = "调用插件声明的普通用户 PATCH 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    request_body = PluginCapabilityInvocationRequest,
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_user_plugin_route_patch() {}

#[utoipa::path(
    delete,
    path = "/api/account/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeUserPluginRouteDelete",
    tag = "插件路由",
    summary = "调用插件声明的普通用户 DELETE 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    request_body = PluginCapabilityInvocationRequest,
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "登录会话无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn invoke_user_plugin_route_delete() {}

#[utoipa::path(
    get,
    path = "/api/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeModelPluginRouteGet",
    tag = "模型插件路由",
    summary = "使用模型 Key 调用插件 GET 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 401, description = "模型 Key 无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 404, description = "插件或路由不存在", body = ManagementErrorBody), (status = 409, description = "插件部署代次已变更", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("apiKeyAuth" = []))
)]
fn invoke_model_plugin_route_get() {}

#[utoipa::path(
    post,
    path = "/api/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeModelPluginRoutePost",
    tag = "模型插件路由",
    summary = "使用模型 Key 调用插件 POST 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    request_body = PluginCapabilityInvocationRequest,
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "模型 Key 无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 409, description = "插件部署代次已变更", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("apiKeyAuth" = []))
)]
fn invoke_model_plugin_route_post() {}

#[utoipa::path(
    put,
    path = "/api/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeModelPluginRoutePut",
    tag = "模型插件路由",
    summary = "使用模型 Key 调用插件 PUT 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    request_body = PluginCapabilityInvocationRequest,
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "模型 Key 无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 409, description = "插件部署代次已变更", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("apiKeyAuth" = []))
)]
fn invoke_model_plugin_route_put() {}

#[utoipa::path(
    patch,
    path = "/api/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeModelPluginRoutePatch",
    tag = "模型插件路由",
    summary = "使用模型 Key 调用插件 PATCH 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    request_body = PluginCapabilityInvocationRequest,
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "模型 Key 无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 409, description = "插件部署代次已变更", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("apiKeyAuth" = []))
)]
fn invoke_model_plugin_route_patch() {}

#[utoipa::path(
    delete,
    path = "/api/plugins/{plugin_id}/routes/{route_id}",
    operation_id = "invokeModelPluginRouteDelete",
    tag = "模型插件路由",
    summary = "使用模型 Key 调用插件 DELETE 路由",
    params(("plugin_id" = String, Path), ("route_id" = String, Path)),
    request_body = PluginCapabilityInvocationRequest,
    responses((status = 200, description = "插件路由输出", body = PluginRouteInvocationResponse), (status = 400, description = "请求无效", body = ManagementErrorBody), (status = 401, description = "模型 Key 无效", body = ManagementErrorBody), (status = 403, description = "路由授权不满足要求", body = ManagementErrorBody), (status = 409, description = "插件部署代次已变更", body = ManagementErrorBody), (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)),
    security(("apiKeyAuth" = []))
)]
fn invoke_model_plugin_route_delete() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/{plugin_id}/provider-selection",
    operation_id = "configureAdminPluginProviderSelection",
    tag = "插件生命周期",
    summary = "设置 Provider 插件适用渠道、模型、优先级和上游响应模式",
    params(("plugin_id" = String, Path)),
    request_body = PluginProviderSelectionRequest,
    responses((status = 200, description = "新的插件部署代次", body = PluginDeploymentResponse), (status = 400, description = "渠道绑定或插件配置无效", body = ManagementErrorBody), (status = 409, description = "部署代次不一致", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn configure_provider_selection() {}

#[utoipa::path(
    put,
    path = "/api/admin/plugins/{plugin_id}/configuration",
    operation_id = "configureAdminPluginConfiguration",
    tag = "插件生命周期",
    summary = "更新插件公开配置并保留当前能力授权",
    params(("plugin_id" = String, Path)),
    request_body = PluginPublicConfigurationUpdateRequest,
    responses((status = 200, description = "已更新插件配置并生成新部署代次", body = PluginDeploymentResponse), (status = 400, description = "配置不符合插件 Schema 或包含敏感字段", body = ManagementErrorBody), (status = 409, description = "部署代次已变化", body = ManagementErrorBody)),
    security(("bearerAuth" = []), ("personalAccessTokenBearer" = [])),
    extensions(("x-anyflows-pat-scopes" = json!( ["admin:configure_admin_plugin_configuration:execute"] )))
)]
fn configure_admin_plugin_configuration() {}

#[utoipa::path(
    post,
    path = "/v1/plugin-tasks",
    operation_id = "submitModelPluginTask",
    tag = "插件任务",
    summary = "使用模型 Key 幂等提交插件任务",
    request_body = ModelPluginTaskRequest,
    responses((status = 200, description = "任务状态和本地 ID", body = serde_json::Value), (status = 409, description = "幂等键或任务状态冲突", body = ManagementErrorBody)),
    security(("apiKeyAuth" = []))
)]
fn submit_model_plugin_task() {}

#[utoipa::path(
    get,
    path = "/v1/plugin-tasks/{task_id}",
    operation_id = "getModelPluginTask",
    tag = "插件任务",
    summary = "读取模型 Key 名下的插件任务、用量、报告和文件清单",
    params(("task_id" = String, Path)),
    responses((status = 200, description = "任务快照", body = serde_json::Value), (status = 404, description = "任务不存在", body = ManagementErrorBody)),
    security(("apiKeyAuth" = []))
)]
fn get_model_plugin_task() {}

#[utoipa::path(
    post,
    path = "/v1/plugin-tasks/{task_id}/cancel",
    operation_id = "cancelModelPluginTask",
    tag = "插件任务",
    summary = "请求取消模型 Key 名下的插件任务",
    params(("task_id" = String, Path)),
    responses((status = 200, description = "取消请求后的任务状态", body = serde_json::Value), (status = 404, description = "任务不存在", body = ManagementErrorBody), (status = 409, description = "任务当前无法取消", body = ManagementErrorBody)),
    security(("apiKeyAuth" = []))
)]
fn cancel_model_plugin_task() {}

#[utoipa::path(
    get,
    path = "/v1/plugin-tasks/{task_id}/files/{file_id}",
    operation_id = "downloadModelPluginTaskFile",
    tag = "插件任务",
    summary = "下载模型 Key 名下已完成任务的结果文件",
    params(("task_id" = String, Path), ("file_id" = usize, Path)),
    responses((status = 200, description = "文件内容", body = String, content_type = "application/octet-stream"), (status = 404, description = "任务或文件不存在", body = ManagementErrorBody)),
    security(("apiKeyAuth" = []))
)]
fn download_model_plugin_task_file() {}

#[utoipa::path(
    get,
    path = "/v1/responses/{response_id}",
    operation_id = "retrievePluginResponse",
    tag = "OpenAI Responses",
    summary = "读取由插件任务生成的 Response",
    params(("response_id" = String, Path)),
    responses((status = 200, description = "Responses 对象", body = serde_json::Value), (status = 404, description = "Response 不存在")),
    security(("apiKeyAuth" = []))
)]
fn retrieve_plugin_response() {}

#[utoipa::path(
    post,
    path = "/v1/videos",
    operation_id = "createPluginVideo",
    tag = "OpenAI Video",
    summary = "使用已绑定的插件提交 JSON 视频任务",
    params(("Idempotency-Key" = Option<String>, Header)),
    request_body = serde_json::Value,
    responses((status = 200, description = "插件 Video 对象", body = serde_json::Value), (status = 404, description = "当前模型无视频插件绑定")),
    security(("apiKeyAuth" = []))
)]
fn create_plugin_video() {}

#[utoipa::path(
    get,
    path = "/v1/videos/{task_id}/content",
    operation_id = "downloadPluginVideo",
    tag = "OpenAI Video",
    summary = "获取已完成插件视频的内容",
    params(("task_id" = String, Path)),
    responses(
        (status = 200, description = "MP4 文件", body = String, content_type = "video/mp4"),
        (status = 303, description = "跳转到插件返回的 HTTPS 视频地址", headers(("Location" = String, description = "视频结果地址"))),
        (status = 404, description = "视频或文件不存在")
    ),
    security(("apiKeyAuth" = []))
)]
fn download_plugin_video() {}

#[utoipa::path(
    get,
    path = "/api/account/plugins/market",
    operation_id = "listPluginMarket",
    tag = "插件市场",
    summary = "发现已审批插件的图标、版本和能力",
    params(("before_id" = Option<i64>, Query, minimum = 1), ("limit" = Option<u32>, Query, minimum = 1, maximum = 100)),
    responses((status = 200, description = "公开插件市场索引投影", body = PluginMarketPageResponse), (status = 401, description = "需要登录", body = ManagementErrorBody), (status = 400, description = "分页参数无效", body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_plugin_market() {}

#[utoipa::path(
    get,
    path = "/api/account/plugins/market/index",
    operation_id = "fetchPluginMarketIndex",
    tag = "插件市场",
    summary = "读取并校验远程插件市场索引",
    params(("url" = String, Query, max_length = 4096)),
    responses(
        (status = 200, description = "经过宿主校验的市场索引", body = serde_json::Value),
        (status = 400, description = "索引地址或内容无效", body = ManagementErrorBody),
        (status = 401, description = "需要登录", body = ManagementErrorBody),
        (status = 413, description = "索引超过大小上限", body = ManagementErrorBody),
        (status = 503, description = "市场下载器不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn fetch_plugin_market_index() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/{plugin_id}/capability-actions",
    operation_id = "proposeAdminPluginCapabilityAction",
    tag = "插件生命周期",
    summary = "创建插件写能力动作提案",
    description = "只接受已审批、已启用且明确授权的幂等 external_write capability。创建提案不会执行插件动作，输入原文不写入动作表。",
    params(("plugin_id" = String, Path)),
    request_body = PluginCapabilityActionProposalRequest,
    responses(
        (status = 200, description = "待确认动作提案", body = PluginCapabilityActionProposalResponse),
        (status = 400, description = "能力、幂等键或输入无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户、插件部署或写能力授权不满足要求", body = ManagementErrorBody),
        (status = 404, description = "插件部署不存在", body = ManagementErrorBody),
        (status = 409, description = "幂等键或部署状态冲突", body = ManagementErrorBody),
        (status = 503, description = "插件运行时不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn propose_admin_plugin_capability_action() {}

#[utoipa::path(
    post,
    path = "/api/admin/plugins/capability-actions/{proposal_key}/confirm",
    operation_id = "confirmAdminPluginCapabilityAction",
    tag = "插件生命周期",
    summary = "确认并执行插件写能力动作",
    description = "确认时再次校验输入摘要、部署 generation、artifact digest 和 capability grant；执行结果只返回宿主状态码。",
    params(("proposal_key" = String, Path)),
    request_body = PluginCapabilityActionConfirmRequest,
    responses(
        (status = 200, description = "插件动作执行状态", body = PluginCapabilityActionResultResponse),
        (status = 400, description = "确认密钥或输入无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户或部署状态不满足要求", body = ManagementErrorBody),
        (status = 404, description = "动作提案不存在", body = ManagementErrorBody),
        (status = 409, description = "提案已执行、过期或状态发生变化", body = ManagementErrorBody),
        (status = 503, description = "插件运行时不可用或结果未知", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn confirm_admin_plugin_capability_action() {}

#[utoipa::path(
    get,
    path = "/api/admin/plugins/events/results",
    operation_id = "listAdminPluginEventResults",
    tag = "插件生命周期",
    summary = "分页读取插件事件处理结果",
    description = "只返回已完成事件的插件输出和状态摘要；不包含事件输入、事件键、插件 URL 或凭据。输出由宿主限制为 16 KiB，并拒绝敏感字段名。结果只是只读数据，不会触发操作或通知。",
    params(("before_id" = Option<i64>, Query, minimum = 1), ("limit" = Option<u32>, Query, minimum = 1, maximum = 100)),
    responses(
        (status = 200, description = "按 delivery ID 倒序排列的事件结果页", body = PluginEventResultPageResponse),
        (status = 400, description = "游标或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody),
        (status = 503, description = "事件存储未配置", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_plugin_event_results() {}

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
        install_admin_market_plugin,
        list_admin_plugin_artifacts,
        list_admin_plugin_deployments,
        invoke_admin_plugin_capability,
        invoke_admin_plugin_credential_lifecycle,
        invoke_admin_plugin_task,
        list_admin_unknown_plugin_tasks,
        reconcile_admin_unknown_plugin_task,
        invoke_admin_plugin_route_get,
        invoke_admin_plugin_route_post,
        invoke_admin_plugin_route_put,
        invoke_admin_plugin_route_patch,
        invoke_admin_plugin_route_delete,
        invoke_user_plugin_route_get,
        invoke_user_plugin_route_post,
        invoke_user_plugin_route_put,
        invoke_user_plugin_route_patch,
        invoke_user_plugin_route_delete,
        invoke_model_plugin_route_get,
        invoke_model_plugin_route_post,
        invoke_model_plugin_route_put,
        invoke_model_plugin_route_patch,
        invoke_model_plugin_route_delete,
        configure_provider_selection,
        get_admin_plugin_configuration,
        configure_admin_plugin_configuration,
        submit_model_plugin_task,
        get_model_plugin_task,
        cancel_model_plugin_task,
        download_model_plugin_task_file,
        retrieve_plugin_response,
        create_plugin_video,
        download_plugin_video,
        list_plugin_market,
        fetch_plugin_market_index,
        propose_admin_plugin_capability_action,
        confirm_admin_plugin_capability_action,
        list_admin_plugin_event_results,
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
        PluginMarketInstallHttpRequest,
        PluginActionResponse,
        PluginArtifactApprovalResponse,
        PluginArtifactPageResponse,
        PluginArtifactRegistrationRequest,
        PluginArtifactResponse,
        PluginArtifactRevocationResponse,
        PluginArtifactRevokeRequest,
        PluginCapabilityGrantRequest,
        PluginCapabilityGrantResponse,
        PluginCapabilityInvocationRequest,
        PluginCapabilityInvocationResponse,
        PluginCredentialInvocationRequest,
        PluginCredentialInvocationResponse,
        PluginTaskInvocationRequest,
        PluginTaskInvocationResponse,
        AdminPluginTaskListQuery,
        AdminPluginTaskReconciliationRequest,
        PluginRouteInvocationResponse,
        PluginCapabilityActionProposalRequest,
        PluginCapabilityActionConfirmRequest,
        PluginCapabilityActionProposalResponse,
        PluginCapabilityActionResultResponse,
        PluginDeploymentDisableRequest,
        PluginDeploymentPageResponse,
        PluginDeploymentRequest,
        PluginDeploymentResponse,
        PluginDeploymentRollbackRequest,
        PluginPublicConfigurationResponse,
        PluginPublicConfigurationUpdateRequest,
        PluginEventResultResponse,
        PluginEventResultPageResponse,
        PluginRuntimeOverviewResponse,
        PluginRuntimeNodeResponse,
        PluginRuntimeStateResponse,
        PluginUninstallResponse,
        PluginArtifactCleanupResponse,
        PluginCatalogSourceResponse,
        PluginCatalogSourcePageResponse,
        PluginCatalogModelResponse,
        PluginCatalogModelPageResponse,
        PluginMarketEntryResponse,
        PluginMarketPageResponse,
        PluginMarketSourceRequest,
        ModelPluginTaskRequest,
        PluginProviderSelectionBody,
        PluginProviderSelectionRequest,
        ManagementErrorBody
    ))
)]
struct PluginLifecycleApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PluginLifecycleApi::openapi()
}
