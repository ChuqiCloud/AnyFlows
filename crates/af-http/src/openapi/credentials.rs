//! 渠道凭据管理 OpenAPI 契约。

#![allow(dead_code, reason = "文档模型和端点仅由 utoipa 过程宏读取")]

use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

use crate::{
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
    #[schema(minimum = 0, required = true)]
    concurrency: Option<i32>,
    #[schema(minimum = 0, required = true)]
    load_factor_micros: Option<i64>,
    /// 账号侧成本倍率，不参与用户扣费。
    #[schema(minimum = 0, required = true)]
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    /// 同渠道且同类型的母凭据 ID；母凭据本身不得再有父级。
    #[schema(minimum = 1, required = true)]
    parent_id: Option<i64>,
    #[schema(value_type = AdminCredentialQuotaDimensionSchema)]
    quota_dimension: String,
    #[schema(minimum = 1, required = true)]
    proxy_id: Option<i64>,
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
    /// 凭据类型必须与 secret 对象的 kind 完全一致。
    #[schema(value_type = AdminCredentialWriteKindSchema, inline)]
    kind: String,
    /// 类型化明文；只用于本次加密，永不随响应返回。
    #[schema(write_only)]
    secret: AdminCredentialSecretSchema,
    #[schema(value_type = AdminRoutingWriteStatusSchema)]
    status: String,
    #[schema(value_type = Option<AdminCredentialMultiKeyModeSchema>, required = true)]
    multi_key_mode: Option<String>,
    priority: i32,
    #[schema(minimum = 0)]
    weight: i32,
    #[schema(minimum = 0, required = true)]
    concurrency: Option<i32>,
    #[schema(minimum = 0, required = true)]
    load_factor_micros: Option<i64>,
    /// 账号侧成本倍率，不参与用户扣费。
    #[schema(minimum = 0, required = true)]
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    /// 同渠道且同类型的母凭据 ID；母凭据本身不得再有父级。
    #[schema(minimum = 1, required = true)]
    parent_id: Option<i64>,
    #[schema(value_type = AdminCredentialQuotaDimensionSchema)]
    quota_dimension: String,
    #[schema(minimum = 1, required = true)]
    proxy_id: Option<i64>,
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
    /// 凭据类型必须与非空 secret 对象的 kind 完全一致。
    #[schema(value_type = AdminCredentialWriteKindSchema, inline)]
    kind: String,
    /// 新明文用于轮换；null 表示保留已有密文。
    #[schema(required = true, write_only)]
    secret: Option<AdminCredentialSecretSchema>,
    #[schema(value_type = AdminRoutingWriteStatusSchema)]
    status: String,
    #[schema(value_type = Option<AdminCredentialMultiKeyModeSchema>, required = true)]
    multi_key_mode: Option<String>,
    priority: i32,
    #[schema(minimum = 0)]
    weight: i32,
    #[schema(minimum = 0, required = true)]
    concurrency: Option<i32>,
    #[schema(minimum = 0, required = true)]
    load_factor_micros: Option<i64>,
    /// 账号侧成本倍率，不参与用户扣费。
    #[schema(minimum = 0, required = true)]
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    /// 同渠道且同类型的母凭据 ID；母凭据本身不得再有父级。
    #[schema(minimum = 1, required = true)]
    parent_id: Option<i64>,
    #[schema(value_type = AdminCredentialQuotaDimensionSchema)]
    quota_dimension: String,
    #[schema(minimum = 1, required = true)]
    proxy_id: Option<i64>,
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
        get_admin_credential,
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
        AdminCredentialResponse,
        AdminCredentialListResponse,
        ManagementErrorBody
    ))
)]
struct CredentialsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    CredentialsApi::openapi()
}
