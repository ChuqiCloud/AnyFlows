use utoipa::OpenApi;

use crate::{management_error::ManagementErrorBody, runtime_status::*};

#[utoipa::path(
    get,
    path = "/api/admin/runtime-status",
    operation_id = "getAdminRuntimeStatus",
    tag = "运行状态",
    summary = "读取节点资源、依赖健康和上下文缓存快照",
    responses(
        (status = 200, description = "后台采样的运行快照；未采集字段为 null，Redis 故障保留最后观测", body = RuntimeStatus),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要平台管理员权限", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_runtime_status() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_runtime_status),
    components(schemas(
        RuntimeStatus,
        RuntimeDependency,
        RuntimeNode,
        RuntimeResourcePoint,
        RuntimePromptCache
    ))
)]
struct RuntimeStatusApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    RuntimeStatusApi::openapi()
}
