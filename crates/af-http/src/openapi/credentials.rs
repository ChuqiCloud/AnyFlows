//! 渠道凭据管理 OpenAPI 契约。

#![allow(dead_code, reason = "文档模型和端点仅由 utoipa 过程宏读取")]

use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

use crate::{
    credential_usage::{CredentialUsageSnapshot, CredentialUsageStatus, CredentialUsageWindow},
    management_credential_writes::{AdminCredentialImportRequest, AdminCredentialImportResponse},
    management_credentials::{AdminCredentialListResponse, AdminCredentialResponse},
    management_error::ManagementErrorBody,
    openapi::schema::{
        AdminCredentialKindSchema, AdminCredentialMultiKeyModeSchema,
        AdminCredentialQuotaDimensionSchema, AdminCredentialWriteKindSchema,
        AdminRoutingStatusSchema, AdminRoutingWriteStatusSchema,
    },
};

#[derive(Serialize, ToSchema)]
#[schema(as = AdminCredentialWriteFields)]
struct AdminCredentialWriteFieldsSchema {
    /// 当前写链路支持的闭合凭据类型。
    #[schema(value_type = AdminCredentialWriteKindSchema, inline)]
    kind: String,
    #[schema(value_type = AdminRoutingWriteStatusSchema)]
    status: String,
    #[schema(value_type = Option<AdminCredentialMultiKeyModeSchema>, required = true)]
    multi_key_mode: Option<String>,
    priority: i32,
    #[schema(minimum = 0)]
    weight: i32,
    /// 普通凭据的独立并发上限；Spark 影子必须为 null 并继承母凭据。
    #[schema(minimum = 0, required = true, example = 8)]
    concurrency: Option<i32>,
    #[schema(minimum = 0, required = true)]
    load_factor_micros: Option<i64>,
    /// 账号侧成本倍率，不参与用户扣费。
    #[schema(minimum = 0, required = true)]
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    /// 普通凭据必须为 null；Spark 影子填写同渠道、已授权 OAuth 根凭据 ID。
    #[schema(minimum = 1, required = true, example = 41)]
    parent_id: Option<i64>,
    /// 普通凭据固定为 global；Spark 影子固定为 spark，创建后不可修改。
    #[schema(value_type = AdminCredentialQuotaDimensionSchema, example = "global")]
    quota_dimension: String,
    /// 普通凭据可绑定专属代理；Spark 影子必须为 null 并继承母凭据。
    #[schema(minimum = 1, required = true, example = 7)]
    proxy_id: Option<i64>,
    /// 仅普通 OAuth 凭据可写；Spark 影子必须为 null。
    #[schema(min_length = 1, max_length = 64, required = true)]
    oauth_provider: Option<String>,
    /// OAuth 账号的非令牌业务标识。
    #[schema(min_length = 1, max_length = 255, required = true)]
    oauth_account_key: Option<String>,
    #[schema(min_length = 1, max_length = 255, required = true)]
    oauth_project_id: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[schema(as = AdminCredentialSecret)]
enum AdminCredentialSecretSchema {
    /// 通用 API Key。
    ApiKey {
        #[schema(min_length = 1, max_length = 16384, write_only)]
        api_key: String,
    },
    /// 仅含现有 access token 的 OAuth 凭据；完整授权材料由 OAuth 连接向导写入。
    Oauth {
        #[schema(min_length = 1, max_length = 16384, write_only)]
        access_token: String,
    },
    /// AWS Bedrock SigV4 长期或 STS 临时凭据。
    Bedrock {
        #[schema(min_length = 1, max_length = 128, write_only)]
        access_key_id: String,
        #[schema(min_length = 1, max_length = 4096, write_only)]
        secret_access_key: String,
        #[schema(min_length = 1, max_length = 16384, required = true, write_only)]
        session_token: Option<String>,
    },
    /// Google Service Account JWT 凭据；不接受可覆盖的 token URI。
    ServiceAccount {
        #[schema(min_length = 1, max_length = 320, write_only)]
        client_email: String,
        #[schema(min_length = 1, max_length = 128, required = true, write_only)]
        private_key_id: Option<String>,
        #[schema(min_length = 1, max_length = 16384, write_only)]
        private_key: String,
    },
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminCredentialCreateRequest)]
struct AdminCredentialCreateRequestSchema {
    /// 普通凭据与非空 secret 的 kind 必须一致；Spark 影子固定为 oauth。
    #[schema(value_type = AdminCredentialWriteKindSchema, inline)]
    kind: String,
    /// 普通 OAuth 可传 null 创建待授权凭据；Spark 影子也必须传 null 且不进入授权流程。
    #[schema(required = true, write_only)]
    secret: Option<AdminCredentialSecretSchema>,
    #[schema(value_type = AdminRoutingWriteStatusSchema)]
    status: String,
    #[schema(value_type = Option<AdminCredentialMultiKeyModeSchema>, required = true)]
    multi_key_mode: Option<String>,
    priority: i32,
    #[schema(minimum = 0)]
    weight: i32,
    /// 普通凭据的独立并发上限；Spark 影子必须为 null 并继承母凭据。
    #[schema(minimum = 0, required = true, example = 8)]
    concurrency: Option<i32>,
    #[schema(minimum = 0, required = true)]
    load_factor_micros: Option<i64>,
    /// 账号侧成本倍率，不参与用户扣费。
    #[schema(minimum = 0, required = true)]
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    /// 普通凭据必须为 null；Spark 影子填写同渠道、已授权 OAuth 根凭据 ID。
    #[schema(minimum = 1, required = true, example = 41)]
    parent_id: Option<i64>,
    /// 普通凭据固定为 global；Spark 影子固定为 spark。
    #[schema(value_type = AdminCredentialQuotaDimensionSchema, example = "global")]
    quota_dimension: String,
    /// 普通凭据可绑定专属代理；Spark 影子必须为 null 并继承母凭据。
    #[schema(minimum = 1, required = true, example = 7)]
    proxy_id: Option<i64>,
    /// 仅普通 OAuth 凭据可写；Spark 影子必须为 null。
    #[schema(min_length = 1, max_length = 64, required = true)]
    oauth_provider: Option<String>,
    /// OAuth 账号的非令牌业务标识。
    #[schema(min_length = 1, max_length = 255, required = true)]
    oauth_account_key: Option<String>,
    #[schema(min_length = 1, max_length = 255, required = true)]
    oauth_project_id: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminCredentialUpdateRequest)]
struct AdminCredentialUpdateRequestSchema {
    /// 类型创建后不可修改；Spark 影子始终为 oauth。
    #[schema(value_type = AdminCredentialWriteKindSchema, inline)]
    kind: String,
    /// 普通凭据用新明文轮换，null 表示保留；Spark 影子必须为 null。
    #[schema(required = true, write_only)]
    secret: Option<AdminCredentialSecretSchema>,
    #[schema(value_type = AdminRoutingWriteStatusSchema)]
    status: String,
    #[schema(value_type = Option<AdminCredentialMultiKeyModeSchema>, required = true)]
    multi_key_mode: Option<String>,
    priority: i32,
    #[schema(minimum = 0)]
    weight: i32,
    /// 普通凭据的独立并发上限；Spark 影子必须为 null 并继承母凭据。
    #[schema(minimum = 0, required = true, example = 8)]
    concurrency: Option<i32>,
    #[schema(minimum = 0, required = true)]
    load_factor_micros: Option<i64>,
    /// 账号侧成本倍率，不参与用户扣费。
    #[schema(minimum = 0, required = true)]
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    /// 父级创建后不可修改；普通凭据为 null，Spark 影子保留原母凭据 ID。
    #[schema(minimum = 1, required = true, example = 41)]
    parent_id: Option<i64>,
    /// 额度维度创建后不可修改；普通凭据为 global，Spark 影子为 spark。
    #[schema(value_type = AdminCredentialQuotaDimensionSchema, example = "global")]
    quota_dimension: String,
    /// 普通凭据可绑定专属代理；Spark 影子必须为 null 并继承母凭据。
    #[schema(minimum = 1, required = true, example = 7)]
    proxy_id: Option<i64>,
    /// 仅普通 OAuth 凭据可写；Spark 影子必须为 null。
    #[schema(min_length = 1, max_length = 64, required = true)]
    oauth_provider: Option<String>,
    /// OAuth 账号的非令牌业务标识。
    #[schema(min_length = 1, max_length = 255, required = true)]
    oauth_account_key: Option<String>,
    #[schema(min_length = 1, max_length = 255, required = true)]
    oauth_project_id: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/admin/channels/{channel_id}/credentials",
    operation_id = "listAdminCredentials",
    tag = "凭据管理",
    summary = "读取管理员渠道凭据列表",
    params(
        ("channel_id" = i64, Path, minimum = 1, description = "渠道 ID"),
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页末尾的凭据 ID"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 50")
    ),
    responses(
        (status = 200, description = "凭据列表", body = AdminCredentialListResponse),
        (status = 400, description = "渠道 ID 或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_credentials() {}

#[utoipa::path(
    post,
    path = "/api/admin/channels/{channel_id}/credentials",
    operation_id = "createAdminCredential",
    tag = "凭据管理",
    summary = "创建管理员渠道凭据",
    params(("channel_id" = i64, Path, minimum = 1, description = "渠道 ID")),
    request_body = AdminCredentialCreateRequestSchema,
    responses(
        (status = 201, description = "凭据已创建", body = AdminCredentialResponse),
        (status = 400, description = "渠道 ID 或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_credential() {}

#[utoipa::path(
    post,
    path = "/api/admin/channels/{channel_id}/credentials/import",
    operation_id = "importAdminCredentials",
    tag = "凭据管理",
    summary = "批量导入 Codex 令牌文件",
    params(("channel_id" = i64, Path, minimum = 1, description = "OpenAI 渠道 ID")),
    request_body = AdminCredentialImportRequest,
    responses(
        (status = 200, description = "逐项导入结果", body = AdminCredentialImportResponse),
        (status = 400, description = "渠道 ID、文件或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn import_admin_credentials() {}

#[utoipa::path(
    get,
    path = "/api/admin/channels/{channel_id}/credentials/export",
    operation_id = "exportAdminCredentials",
    tag = "凭据管理",
    summary = "导出兼容 sub2api 的 Codex 令牌",
    params(("channel_id" = i64, Path, minimum = 1, description = "OpenAI 渠道 ID")),
    responses(
        (status = 200, description = "令牌 JSON 文件", content_type = "application/json"),
        (status = 400, description = "渠道 ID 无效或不是 OpenAI 渠道", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn export_admin_credentials() {}

#[utoipa::path(
    get,
    path = "/api/admin/channels/{channel_id}/credentials/{credential_id}",
    operation_id = "getAdminCredential",
    tag = "凭据管理",
    summary = "读取管理员渠道凭据详情",
    params(
        ("channel_id" = i64, Path, minimum = 1, description = "渠道 ID"),
        ("credential_id" = i64, Path, minimum = 1, description = "凭据 ID")
    ),
    responses(
        (status = 200, description = "凭据详情", body = AdminCredentialResponse),
        (status = 400, description = "渠道 ID 或凭据 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道或凭据不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_credential() {}

#[utoipa::path(
    get,
    path = "/api/admin/channels/{channel_id}/credentials/{credential_id}/usage",
    operation_id = "getAdminCredentialUsage",
    tag = "凭据管理",
    summary = "查询 Codex OAuth 账号上游用量窗口",
    params(
        ("channel_id" = i64, Path, minimum = 1, description = "渠道 ID"),
        ("credential_id" = i64, Path, minimum = 1, description = "凭据 ID")
    ),
    responses(
        (status = 200, description = "用量快照或不可用状态", body = CredentialUsageSnapshot),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道或凭据不存在", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_credential_usage() {}

#[utoipa::path(
    put,
    path = "/api/admin/channels/{channel_id}/credentials/{credential_id}",
    operation_id = "updateAdminCredential",
    tag = "凭据管理",
    summary = "更新管理员渠道凭据",
    params(
        ("channel_id" = i64, Path, minimum = 1, description = "渠道 ID"),
        ("credential_id" = i64, Path, minimum = 1, description = "凭据 ID")
    ),
    request_body = AdminCredentialUpdateRequestSchema,
    responses(
        (status = 200, description = "凭据已更新", body = AdminCredentialResponse),
        (status = 400, description = "路径参数或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道或凭据不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_admin_credential() {}

#[utoipa::path(
    delete,
    path = "/api/admin/channels/{channel_id}/credentials/{credential_id}",
    operation_id = "deleteAdminCredential",
    tag = "凭据管理",
    summary = "软删除管理员渠道凭据",
    params(
        ("channel_id" = i64, Path, minimum = 1, description = "渠道 ID"),
        ("credential_id" = i64, Path, minimum = 1, description = "凭据 ID")
    ),
    responses(
        (status = 204, description = "凭据已删除"),
        (status = 400, description = "渠道 ID 或凭据 ID 无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "渠道或凭据不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_admin_credential() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_credentials,
        create_admin_credential,
        import_admin_credentials,
        export_admin_credentials,
        get_admin_credential,
        get_admin_credential_usage,
        update_admin_credential,
        delete_admin_credential
    ),
    components(schemas(
        AdminCredentialKindSchema,
        AdminCredentialMultiKeyModeSchema,
        AdminCredentialQuotaDimensionSchema,
        AdminRoutingStatusSchema,
        AdminRoutingWriteStatusSchema,
        AdminCredentialWriteFieldsSchema,
        AdminCredentialSecretSchema,
        AdminCredentialCreateRequestSchema,
        AdminCredentialUpdateRequestSchema,
        AdminCredentialImportRequest,
        AdminCredentialImportResponse,
        AdminCredentialResponse,
        CredentialUsageSnapshot,
        CredentialUsageStatus,
        CredentialUsageWindow,
        AdminCredentialListResponse,
        ManagementErrorBody
    ))
)]
struct CredentialsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    CredentialsApi::openapi()
}
