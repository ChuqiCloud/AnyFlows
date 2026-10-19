//! 代理池管理契约。
#![allow(dead_code, reason = "文档端点由 utoipa 宏读取")]
use crate::{management_error::ManagementErrorBody, proxy_pools::*};
use utoipa::OpenApi;
#[utoipa::path(get,path="/api/admin/proxy-pools",operation_id="listAdminProxyPools",tag="代理池",summary="读取代理池",responses((status=200,description="成功",body=AdminProxyPoolList),(status=400,description="输入无效",body=ManagementErrorBody),(status=401,description="会话无效",body=ManagementErrorBody),(status=403,description="需要管理员权限",body=ManagementErrorBody),(status=404,description="代理池不存在",body=ManagementErrorBody),(status=409,description="冲突或仍被引用",body=ManagementErrorBody),(status=500,description="操作失败",body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn endpoint_0() {}
#[utoipa::path(post,path="/api/admin/proxy-pools",operation_id="createAdminProxyPool",tag="代理池",summary="创建代理池",request_body=AdminProxyPoolWriteRequest,responses((status=201,description="成功",body=AdminProxyPoolImportResult),(status=400,description="输入无效",body=ManagementErrorBody),(status=401,description="会话无效",body=ManagementErrorBody),(status=403,description="需要管理员权限",body=ManagementErrorBody),(status=404,description="代理池不存在",body=ManagementErrorBody),(status=409,description="冲突或仍被引用",body=ManagementErrorBody),(status=500,description="操作失败",body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn endpoint_1() {}
#[utoipa::path(put,path="/api/admin/proxy-pools/{id}",operation_id="updateAdminProxyPool",tag="代理池",summary="更新代理池并导入节点",params(("id"=i64,Path,minimum=1)),request_body=AdminProxyPoolWriteRequest,responses((status=200,description="成功",body=AdminProxyPoolImportResult),(status=400,description="输入无效",body=ManagementErrorBody),(status=401,description="会话无效",body=ManagementErrorBody),(status=403,description="需要管理员权限",body=ManagementErrorBody),(status=404,description="代理池不存在",body=ManagementErrorBody),(status=409,description="冲突或仍被引用",body=ManagementErrorBody),(status=500,description="操作失败",body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn endpoint_2() {}
#[utoipa::path(delete,path="/api/admin/proxy-pools/{id}",operation_id="deleteAdminProxyPool",tag="代理池",summary="删除未绑定的代理池",params(("id"=i64,Path,minimum=1)),responses((status=204,description="成功"),(status=400,description="输入无效",body=ManagementErrorBody),(status=401,description="会话无效",body=ManagementErrorBody),(status=403,description="需要管理员权限",body=ManagementErrorBody),(status=404,description="代理池不存在",body=ManagementErrorBody),(status=409,description="冲突或仍被引用",body=ManagementErrorBody),(status=500,description="操作失败",body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn endpoint_3() {}
#[utoipa::path(post,path="/api/admin/proxy-pools/{id}/refresh",operation_id="refreshAdminProxyPool",tag="代理池",summary="更新订阅并测活",params(("id"=i64,Path,minimum=1)),responses((status=200,description="成功",body=AdminProxyPool),(status=400,description="输入无效",body=ManagementErrorBody),(status=401,description="会话无效",body=ManagementErrorBody),(status=403,description="需要管理员权限",body=ManagementErrorBody),(status=404,description="代理池不存在",body=ManagementErrorBody),(status=409,description="冲突或仍被引用",body=ManagementErrorBody),(status=500,description="操作失败",body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn endpoint_4() {}
#[utoipa::path(post,path="/api/admin/proxy-pools/{id}/check",operation_id="checkAdminProxyPool",tag="代理池",summary="检测现有节点",params(("id"=i64,Path,minimum=1)),responses((status=200,description="成功",body=AdminProxyPool),(status=400,description="输入无效",body=ManagementErrorBody),(status=401,description="会话无效",body=ManagementErrorBody),(status=403,description="需要管理员权限",body=ManagementErrorBody),(status=404,description="代理池不存在",body=ManagementErrorBody),(status=409,description="冲突或仍被引用",body=ManagementErrorBody),(status=500,description="操作失败",body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn endpoint_5() {}
#[utoipa::path(put,path="/api/admin/proxy-pool-bindings",operation_id="bindAdminProxyPool",tag="代理池",summary="设置全局或渠道代理池",request_body=AdminProxyPoolBindingRequest,responses((status=204,description="成功"),(status=400,description="输入无效",body=ManagementErrorBody),(status=401,description="会话无效",body=ManagementErrorBody),(status=403,description="需要管理员权限",body=ManagementErrorBody),(status=404,description="代理池不存在",body=ManagementErrorBody),(status=409,description="冲突或仍被引用",body=ManagementErrorBody),(status=500,description="操作失败",body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn endpoint_6() {}
#[utoipa::path(post,path="/api/admin/proxy-pool-cleanup",operation_id="cleanupAdminProxyItems",tag="代理池",summary="预览或执行批量清理，保留被引用的项目",request_body=AdminProxyCleanupRequest,responses((status=200,description="清理结果",body=AdminProxyCleanupResult),(status=400,description="输入无效",body=ManagementErrorBody),(status=401,description="会话无效",body=ManagementErrorBody),(status=403,description="需要管理员权限",body=ManagementErrorBody),(status=404,description="代理池不存在",body=ManagementErrorBody),(status=409,description="版本冲突或任务正在进行",body=ManagementErrorBody),(status=500,description="操作失败",body=ManagementErrorBody)),security(("bearerAuth"=[])))]
fn endpoint_7() {}
#[derive(OpenApi)]
#[openapi(
    paths(
        endpoint_0, endpoint_1, endpoint_2, endpoint_3, endpoint_4, endpoint_5, endpoint_6,
        endpoint_7
    ),
    components(schemas(
        AdminProxyPool,
        AdminProxyPoolNode,
        AdminProxyPoolList,
        AdminProxyPoolWriteRequest,
        AdminProxyPoolImportResult,
        AdminProxyPoolBindingRequest,
        AdminProxyCleanupRequest,
        AdminProxyCleanupResult
    ))
)]
struct ProxyPoolsApi;
pub(super) fn document() -> utoipa::openapi::OpenApi {
    ProxyPoolsApi::openapi()
}

#[cfg(test)]
mod tests {
    #[test]
    fn proxy_pool_contract_registers_admin_operations_and_hides_import_secrets() {
        let document = serde_json::to_value(super::super::openapi_document()).unwrap();
        for (path, method, operation_id) in [
            ("/api/admin/proxy-pools", "get", "listAdminProxyPools"),
            (
                "/api/admin/proxy-pool-cleanup",
                "post",
                "cleanupAdminProxyItems",
            ),
            ("/api/admin/proxy-pools", "post", "createAdminProxyPool"),
            ("/api/admin/proxy-pools/{id}", "put", "updateAdminProxyPool"),
            (
                "/api/admin/proxy-pools/{id}",
                "delete",
                "deleteAdminProxyPool",
            ),
            (
                "/api/admin/proxy-pools/{id}/refresh",
                "post",
                "refreshAdminProxyPool",
            ),
            (
                "/api/admin/proxy-pools/{id}/check",
                "post",
                "checkAdminProxyPool",
            ),
            (
                "/api/admin/proxy-pool-bindings",
                "put",
                "bindAdminProxyPool",
            ),
        ] {
            let operation = &document["paths"][path][method];
            assert_eq!(operation["operationId"], operation_id);
            assert_eq!(
                operation["security"],
                serde_json::json!([{"bearerAuth": []}])
            );
            assert!(operation["responses"].get("401").is_some());
            assert!(operation["responses"].get("403").is_some());
        }

        let schemas = &document["components"]["schemas"];
        let request = &schemas["AdminProxyPoolWriteRequest"]["properties"];
        assert_eq!(request["interval_secs"]["minimum"], 60);
        assert_eq!(request["interval_secs"]["maximum"], 86400);
        assert!(
            schemas["AdminProxyPool"]["properties"]
                .get("last_refresh_attempt_at")
                .is_some()
        );
        assert!(
            schemas["AdminProxyCleanupRequest"]["properties"]
                .get("after_id")
                .is_some()
        );
        assert_eq!(
            schemas["AdminProxyCleanupRequest"]["properties"]["ids"]["maxItems"],
            256
        );
        assert!(
            schemas["AdminProxyCleanupResult"]["properties"]
                .get("next_after_id")
                .is_some()
        );
        for field in ["source_url", "content"] {
            assert_eq!(request[field]["writeOnly"], true);
        }
        for schema in ["AdminProxyPool", "AdminProxyPoolNode"] {
            let response = schemas[schema]["properties"].as_object().unwrap();
            for field in ["source_url", "content", "username", "password"] {
                assert!(!response.contains_key(field));
            }
        }
        assert!(
            schemas["AdminProxyPoolNode"]["properties"]
                .get("remark")
                .is_some()
        );
        for field in ["skipped", "duplicates"] {
            assert!(
                schemas["AdminProxyPoolImportResult"]["properties"]
                    .get(field)
                    .is_some()
            );
        }
    }
}
