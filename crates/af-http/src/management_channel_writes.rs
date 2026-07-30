use af_admin::{
    AdminChannelCreateCommand, AdminChannelUpdateCommand, AdminChannelWriteError,
    AdminRoutingWriteStatus,
};
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    response::Response,
};
use http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_channels::{
        AdminChannelResponse, no_store_empty, no_store_json, parse_channel_id, status_json,
    },
    management_error::ManagementError,
};

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminChannelCreateRequest)]
/// 管理端创建渠道时使用的完整配置正文。
pub(crate) struct AdminChannelCreateRequest {
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[serde(rename = "type")]
    /// 当前生产写链路支持 OpenAI、标准 Anthropic 与 Gemini Developer API 适配器。
    #[schema(
        value_type = crate::openapi::schema::AdminChannelWriteTypeSchema,
        inline
    )]
    channel_type: af_domain::ChannelType,
    /// 当前生产写链路支持 OpenAI Chat、Responses、Anthropic Messages 与 Gemini 协议。
    #[schema(
        value_type = crate::openapi::schema::AdminChannelWriteProtocolSchema,
        inline
    )]
    protocol: af_domain::Protocol,
    /// 不含 userinfo、查询串或片段的 HTTP(S) 基础地址。
    #[schema(max_length = 2048, format = "uri", required = true)]
    base_url: Option<String>,
    /// 同时覆盖上游读取停顿与完整请求总时限；省略或 null 时使用服务默认值。
    #[schema(minimum = 1, maximum = 900)]
    timeout_secs: Option<u64>,
    #[schema(value_type = crate::openapi::schema::AdminRoutingWriteStatusSchema)]
    status: AdminRoutingWriteStatus,
    #[schema(minimum = 0)]
    weight: i32,
    priority: i32,
    auto_ban: bool,
    #[schema(schema_with = crate::openapi::schema::channel_write_models_schema)]
    models: Vec<String>,
    #[schema(schema_with = crate::openapi::schema::channel_write_group_ids_schema)]
    group_ids: Vec<i64>,
    #[schema(schema_with = crate::openapi::schema::model_mapping_write_schema)]
    model_mapping: serde_json::Value,
    /// 闭合的 Canonical 采样参数覆盖。
    #[schema(schema_with = crate::openapi::schema::channel_parameter_overrides_schema)]
    param_override: serde_json::Value,
    /// 敏感 Header 覆盖输入；拒绝认证、协议托管和 hop-by-hop 头，永不随响应返回。
    #[schema(schema_with = crate::openapi::schema::sensitive_string_map_schema)]
    header_override: serde_json::Value,
    /// 敏感适配器设置输入，序列化后最多 64 KiB，永不随响应返回。
    #[schema(schema_with = crate::openapi::schema::sensitive_free_form_object_schema)]
    settings: serde_json::Value,
    #[schema(min_length = 1, max_length = 64, required = true)]
    tag: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminChannelUpdateRequest)]
/// 管理端更新渠道时使用的配置正文；敏感对象省略或为 null 时保留原值。
pub(crate) struct AdminChannelUpdateRequest {
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[serde(rename = "type")]
    #[schema(
        value_type = crate::openapi::schema::AdminChannelWriteTypeSchema,
        inline
    )]
    channel_type: af_domain::ChannelType,
    #[schema(
        value_type = crate::openapi::schema::AdminChannelWriteProtocolSchema,
        inline
    )]
    protocol: af_domain::Protocol,
    #[schema(max_length = 2048, format = "uri", required = true)]
    base_url: Option<String>,
    /// 同时覆盖上游读取停顿与完整请求总时限；省略或 null 时使用服务默认值。
    #[schema(minimum = 1, maximum = 900)]
    timeout_secs: Option<u64>,
    #[schema(value_type = crate::openapi::schema::AdminRoutingWriteStatusSchema)]
    status: AdminRoutingWriteStatus,
    #[schema(minimum = 0)]
    weight: i32,
    priority: i32,
    auto_ban: bool,
    #[schema(schema_with = crate::openapi::schema::channel_write_models_schema)]
    models: Vec<String>,
    #[schema(schema_with = crate::openapi::schema::channel_write_group_ids_schema)]
    group_ids: Vec<i64>,
    #[schema(schema_with = crate::openapi::schema::model_mapping_write_schema)]
    model_mapping: serde_json::Value,
    #[schema(schema_with = crate::openapi::schema::channel_parameter_overrides_schema)]
    param_override: serde_json::Value,
    /// 省略或 null 表示保留，显式对象（包括空对象）表示替换。
    #[schema(schema_with = crate::openapi::schema::optional_sensitive_string_map_schema)]
    header_override: Option<serde_json::Value>,
    /// 省略或 null 表示保留，显式对象（包括空对象）表示替换。
    #[schema(schema_with = crate::openapi::schema::optional_sensitive_free_form_object_schema)]
    settings: Option<serde_json::Value>,
    #[schema(min_length = 1, max_length = 64, required = true)]
    tag: Option<String>,
}

/// 创建渠道并返回不含敏感配置的管理快照。
pub(crate) async fn create_admin_channel(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminChannelCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let channel = writer
        .create_channel(authentication.principal(), request.into_create_command()?)
        .await
        .map_err(map_write_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminChannelResponse::from_channel(&channel),
    ))
}

/// 完整更新渠道配置，保留余额与累计用量等运行字段。
pub(crate) async fn update_admin_channel(
    State(state): State<HttpState>,
    Path(channel_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminChannelUpdateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let channel = writer
        .update_channel(
            authentication.principal(),
            channel_id,
            request.into_update_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(no_store_json(AdminChannelResponse::from_channel(&channel)))
}

/// 安全软删除渠道、所属凭据和可重建的直接运行时关系。
pub(crate) async fn delete_admin_channel(
    State(state): State<HttpState>,
    Path(channel_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    writer
        .delete_channel(authentication.principal(), channel_id)
        .await
        .map_err(map_write_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

impl AdminChannelCreateRequest {
    fn into_create_command(self) -> Result<AdminChannelCreateCommand, ManagementError> {
        let group_ids = parse_group_ids(self.group_ids)?;
        AdminChannelCreateCommand::new(
            self.name,
            self.channel_type,
            self.protocol,
            self.base_url,
            self.timeout_secs,
            self.status,
            self.weight,
            self.priority,
            self.auto_ban,
            self.models,
            group_ids,
            self.model_mapping,
            self.param_override,
            self.header_override,
            self.settings,
            self.tag,
        )
        .map_err(map_write_error)
    }
}

impl AdminChannelUpdateRequest {
    fn into_update_command(self) -> Result<AdminChannelUpdateCommand, ManagementError> {
        let group_ids = parse_group_ids(self.group_ids)?;
        AdminChannelUpdateCommand::new(
            self.name,
            self.channel_type,
            self.protocol,
            self.base_url,
            self.timeout_secs,
            self.status,
            self.weight,
            self.priority,
            self.auto_ban,
            self.models,
            group_ids,
            self.model_mapping,
            self.param_override,
            self.header_override,
            self.settings,
            self.tag,
        )
        .map_err(map_write_error)
    }
}

fn parse_group_ids(values: Vec<i64>) -> Result<Vec<af_domain::GroupId>, ManagementError> {
    values
        .into_iter()
        .map(|value| af_domain::GroupId::new(value).map_err(|_| ManagementError::InvalidRequest))
        .collect()
}

pub(crate) fn map_write_error(error: AdminChannelWriteError) -> ManagementError {
    match error {
        AdminChannelWriteError::InvalidInput => ManagementError::InvalidRequest,
        AdminChannelWriteError::Forbidden => ManagementError::Forbidden,
        AdminChannelWriteError::ChannelNotFound => ManagementError::ChannelNotFound,
        AdminChannelWriteError::CredentialNotFound => ManagementError::CredentialNotFound,
        AdminChannelWriteError::Internal => ManagementError::Internal,
    }
}
