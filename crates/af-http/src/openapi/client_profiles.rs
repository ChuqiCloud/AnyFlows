//! 客户端档案管理的公开 API 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]
use crate::{client_profiles::*, management_error::ManagementErrorBody};
use utoipa::OpenApi;

#[utoipa::path(get, path = "/api/admin/client-profiles", operation_id = "listAdminClientProfiles", tag = "客户端档案", summary = "分页查询客户端档案",
params(("after" = Option<String>, Query, description = "上一页返回的游标"), ("search" = Option<String>, Query), ("enabled" = Option<bool>, Query), ("service" = Option<ClientProfileServiceSchema>, Query), ("protocol" = Option<crate::openapi::schema::AdminChannelProtocolSchema>, Query), ("credential_kind" = Option<crate::openapi::schema::AdminCredentialKindSchema>, Query), ("channel_type" = Option<crate::openapi::schema::AdminChannelTypeSchema>, Query), ("limit" = Option<u32>, Query, minimum = 1, maximum = 100)),
responses((status = 200, description = "操作成功", body = ClientProfilePageSchema),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn list_admin_client_profiles() {}

#[utoipa::path(get, path = "/api/admin/client-profiles/capabilities", operation_id = "getAdminClientProfileCapabilities", tag = "客户端档案", summary = "读取已注册的客户端档案能力",
responses((status = 200, description = "受控模板与身份字段", body = ClientProfileCapabilitiesSchema), (status = 401, description = "会话无效", body = ManagementErrorBody), (status = 403, description = "需要平台管理员权限", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn client_profile_capabilities() {}

#[utoipa::path(get, path = "/api/admin/client-profiles/opencode-settings/{channel_id}", operation_id = "getAdminClientProfileOpenCodeSettings", tag = "客户端档案", summary = "读取渠道的 OpenCode 服务入口", params(("channel_id" = i64, Path)),
responses((status = 200, description = "不含敏感设置的服务入口", body = ClientProfileOpenCodeSettingsSchema), (status = 400, description = "渠道类型无效", body = ManagementErrorBody), (status = 401, description = "会话无效", body = ManagementErrorBody), (status = 403, description = "需要平台管理员权限", body = ManagementErrorBody), (status = 404, description = "渠道不存在", body = ManagementErrorBody), (status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn get_opencode_settings() {}

#[utoipa::path(put, path = "/api/admin/client-profiles/opencode-settings/{channel_id}", operation_id = "setAdminClientProfileOpenCodeEntry", tag = "客户端档案", summary = "更新渠道的 OpenCode 服务入口", params(("channel_id" = i64, Path)), request_body = ClientProfileOpenCodeEntryRequest,
responses((status = 200, description = "入口已更新", body = ClientProfileOpenCodeSettingsSchema), (status = 400, description = "入口或渠道类型无效", body = ManagementErrorBody), (status = 401, description = "会话无效", body = ManagementErrorBody), (status = 403, description = "需要平台管理员权限", body = ManagementErrorBody), (status = 404, description = "渠道不存在", body = ManagementErrorBody), (status = 409, description = "渠道已更新，请重新读取", body = ManagementErrorBody), (status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn set_opencode_settings() {}

#[utoipa::path(post, path = "/api/admin/client-profiles/validate", operation_id = "validateAdminClientProfile", tag = "客户端档案", summary = "校验客户端档案",
request_body = ClientProfileSaveRequest,
responses((status = 200, description = "操作成功", body = serde_json::Value),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn validate_admin_client_profile() {}

#[utoipa::path(get, path = "/api/admin/client-profiles/{key}", operation_id = "getAdminClientProfile", tag = "客户端档案", summary = "读取客户端档案",
params(("key" = String, Path)),
responses((status = 200, description = "操作成功", body = ClientProfileRecordSchema),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn get_admin_client_profile() {}

#[utoipa::path(put, path = "/api/admin/client-profiles/{key}", operation_id = "saveAdminClientProfile", tag = "客户端档案", summary = "创建档案或发布新修订",
params(("key" = String, Path)),
request_body = ClientProfileSaveRequest,
responses((status = 200, description = "操作成功", body = ClientProfileSnapshotSchema),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn save_admin_client_profile() {}

#[utoipa::path(post, path = "/api/admin/client-profiles/{key}/enabled", operation_id = "setAdminClientProfileEnabled", tag = "客户端档案", summary = "启用或停用客户端档案",
params(("key" = String, Path)),
request_body = ClientProfileEnabledRequest,
responses((status = 200, description = "操作成功", body = ClientProfileRecordSchema),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn set_admin_client_profile_enabled() {}

#[utoipa::path(get, path = "/api/admin/client-profiles/{key}/revisions", operation_id = "listAdminClientProfileRevisions", tag = "客户端档案", summary = "分页查询档案修订",
params(("key" = String, Path), ("before" = Option<i64>, Query), ("limit" = Option<u32>, Query, minimum = 1, maximum = 100)),
responses((status = 200, description = "操作成功", body = ClientProfileRevisionPageSchema),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn list_admin_client_profile_revisions() {}

#[utoipa::path(get, path = "/api/admin/client-profiles/{key}/revisions/{revision}", operation_id = "getAdminClientProfileRevision", tag = "客户端档案", summary = "读取冻结修订",
params(("key" = String, Path), ("revision" = i64, Path)),
responses((status = 200, description = "操作成功", body = ClientProfileSnapshotSchema),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn get_admin_client_profile_revision() {}

#[utoipa::path(delete, path = "/api/admin/client-profiles/{key}/revisions/{revision}", operation_id = "deleteAdminClientProfileRevision", tag = "客户端档案", summary = "删除未引用的历史修订",
params(("key" = String, Path), ("revision" = i64, Path)),
request_body = ClientProfileVersionRequest,
responses((status = 200, description = "操作成功", body = serde_json::Value),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn delete_admin_client_profile_revision() {}

#[utoipa::path(get, path = "/api/admin/client-profiles/{key}/references", operation_id = "listAdminClientProfileReferences", tag = "客户端档案", summary = "分页查询档案的渠道引用",
params(("key" = String, Path), ("after" = Option<i64>, Query), ("limit" = Option<u32>, Query, minimum = 1, maximum = 100)),
responses((status = 200, description = "操作成功", body = ClientProfileReferencePageSchema),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn list_admin_client_profile_references() {}

#[utoipa::path(get, path = "/api/admin/client-profiles/channel-bindings/{channel_id}", operation_id = "getAdminChannelClientProfile", tag = "客户端档案", summary = "读取渠道的档案绑定",
params(("channel_id" = i64, Path)),
responses((status = 200, description = "操作成功", body = Option<ClientProfileReferenceSchema>),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn get_admin_channel_client_profile() {}

#[utoipa::path(put, path = "/api/admin/client-profiles/channel-bindings/{channel_id}", operation_id = "bindAdminChannelClientProfile", tag = "客户端档案", summary = "切换渠道的档案修订",
params(("channel_id" = i64, Path)),
request_body = ClientProfileBindRequest,
responses((status = 200, description = "操作成功", body = ClientProfileReferenceSchema),
(status = 400, description = "配置或分页参数无效", body = ManagementErrorBody),
(status = 401, description = "会话无效", body = ManagementErrorBody),
(status = 403, description = "需要平台管理员权限", body = ManagementErrorBody),
(status = 404, description = "档案、修订或渠道不存在", body = ManagementErrorBody),
(status = 409, description = "版本冲突、内置保护、已停用或被引用", body = ManagementErrorBody),
(status = 500, description = "内部错误", body = ManagementErrorBody)), security(("bearerAuth" = [])))]
fn bind_admin_channel_client_profile() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        client_profile_capabilities,
        get_opencode_settings,
        set_opencode_settings,
        list_admin_client_profiles,
        validate_admin_client_profile,
        get_admin_client_profile,
        save_admin_client_profile,
        set_admin_client_profile_enabled,
        list_admin_client_profile_revisions,
        get_admin_client_profile_revision,
        delete_admin_client_profile_revision,
        list_admin_client_profile_references,
        get_admin_channel_client_profile,
        bind_admin_channel_client_profile
    ),
    components(schemas(
        ClientProfileCapabilitiesSchema,
        ClientProfileOpenCodeSettingsSchema,
        ClientProfileOpenCodeEntrySchema,
        ClientProfileOpenCodeEntryRequest,
        ClientProfileSaveRequest,
        ClientProfileEnabledRequest,
        ClientProfileVersionRequest,
        ClientProfileBindRequest,
        ClientProfileDefinitionSchema,
        ClientProfileServiceSchema,
        ClientProfileSessionStrategySchema,
        ClientProfileBehaviorSchema,
        ClientProfileBindingSchema,
        ClientProfileSnapshotSchema,
        ClientProfileRecordSchema,
        ClientProfilePageSchema,
        ClientProfileRevisionPageSchema,
        ClientProfileReferenceSchema,
        ClientProfileReferencePageSchema
    ))
)]
struct ClientProfilesApi;
pub(super) fn document() -> utoipa::openapi::OpenApi {
    ClientProfilesApi::openapi()
}
