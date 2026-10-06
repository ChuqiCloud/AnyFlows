//! 模型缺失检测、上游同步预览与原子应用 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_model_sync::{
        AdminMissingModelChannelResponse, AdminMissingModelImportItemRequest,
        AdminMissingModelImportRequest, AdminMissingModelImportResponse,
        AdminMissingModelListResponse, AdminMissingModelResponse, AdminModelSyncApplyItemRequest,
        AdminModelSyncApplyRequest, AdminModelSyncApplyResponse, AdminModelSyncPreviewItemResponse,
        AdminModelSyncPreviewRequest, AdminModelSyncPreviewResponse, AdminModelSyncRelationValue,
    },
    management_models::{AdminModelModalityValue, AdminModelResponse},
    openapi::schema::{AdminChannelProtocolSchema, AdminChannelTypeSchema},
};

#[utoipa::path(
    get,
    path = "/api/admin/models/missing",
    operation_id = "listMissingAdminModels",
    tag = "模型管理",
    summary = "列出渠道已引用但缺少商品元数据的模型",
    params(
        ("after" = Option<String>, Query, max_length = 256, description = "上一页末尾的 Canonical 模型标识"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "缺失模型列表", body = AdminMissingModelListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_missing_admin_models() {}

#[utoipa::path(
    post,
    path = "/api/admin/models/missing",
    operation_id = "importMissingAdminModels",
    tag = "模型管理",
    summary = "批量导入渠道已引用但缺少元数据的模型",
    request_body = AdminMissingModelImportRequest,
    responses(
        (status = 201, description = "所选模型已创建为隐藏草稿", body = AdminMissingModelImportResponse),
        (status = 400, description = "批次或结构化元数据无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 409, description = "模型已被创建或已不再被活动渠道引用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn import_missing_admin_models() {}

#[utoipa::path(
    post,
    path = "/api/admin/models/sync-previews",
    operation_id = "createAdminModelSyncPreview",
    tag = "模型管理",
    summary = "从单个渠道创建固定上游模型同步预览",
    request_body = AdminModelSyncPreviewRequest,
    responses(
        (status = 201, description = "同步预览已创建", body = AdminModelSyncPreviewResponse),
        (status = 400, description = "请求正文或渠道 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道不存在", body = ManagementErrorBody),
        (status = 409, description = "渠道或凭据当前不可用于同步", body = ManagementErrorBody),
        (status = 422, description = "渠道类型或协议不支持同步", body = ManagementErrorBody),
        (status = 502, description = "上游拒绝请求、响应无效或候选数量超限", body = ManagementErrorBody),
        (status = 504, description = "上游模型枚举超时", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_model_sync_preview() {}

#[utoipa::path(
    post,
    path = "/api/admin/models/sync-previews/{preview_id}/apply",
    operation_id = "applyAdminModelSyncPreview",
    tag = "模型管理",
    summary = "原子应用同步预览、创建隐藏草稿并加入渠道路由",
    params(
        ("preview_id" = String, Path, min_length = 36, max_length = 36, description = "同步预览 UUID")
    ),
    request_body = AdminModelSyncApplyRequest,
    responses(
        (status = 200, description = "所选模型已创建为隐藏草稿并加入来源渠道路由", body = AdminModelSyncApplyResponse),
        (status = 400, description = "预览 ID 或结构化元数据无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "预览不存在或不属于当前管理员", body = ManagementErrorBody),
        (status = 409, description = "预览已应用或发生并发冲突", body = ManagementErrorBody),
        (status = 410, description = "预览已过期", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn apply_admin_model_sync_preview() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_missing_admin_models,
        import_missing_admin_models,
        create_admin_model_sync_preview,
        apply_admin_model_sync_preview
    ),
    components(schemas(
        AdminMissingModelChannelResponse,
        AdminMissingModelResponse,
        AdminMissingModelListResponse,
        AdminMissingModelImportItemRequest,
        AdminMissingModelImportRequest,
        AdminMissingModelImportResponse,
        AdminModelSyncPreviewRequest,
        AdminModelSyncPreviewItemResponse,
        AdminModelSyncPreviewResponse,
        AdminModelSyncRelationValue,
        AdminModelSyncApplyItemRequest,
        AdminModelSyncApplyRequest,
        AdminModelSyncApplyResponse,
        AdminModelResponse,
        AdminModelModalityValue,
        AdminChannelTypeSchema,
        AdminChannelProtocolSchema,
        ManagementErrorBody
    ))
)]
struct ModelSyncApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ModelSyncApi::openapi()
}
