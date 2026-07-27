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

use crate::{
    chat_completions::HttpState,
    management_channels::{
        AdminChannelResponse, no_store_empty, no_store_json, parse_channel_id, status_json,
    },
    management_error::ManagementError,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
/// 管理端创建或完整更新渠道时使用的配置正文。
pub(crate) struct AdminChannelWriteRequest {
    name: String,
    #[serde(rename = "type")]
    channel_type: af_domain::ChannelType,
    protocol: af_domain::Protocol,
    base_url: Option<String>,
    status: AdminRoutingWriteStatus,
    weight: i32,
    priority: i32,
    auto_ban: bool,
    model_mapping: serde_json::Value,
    param_override: serde_json::Value,
    header_override: serde_json::Value,
    settings: serde_json::Value,
    tag: Option<String>,
}

/// 创建渠道并返回不含敏感配置的管理快照。
pub(crate) async fn create_admin_channel(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminChannelWriteRequest>, JsonRejection>,
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
    request: Result<Json<AdminChannelWriteRequest>, JsonRejection>,
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

impl AdminChannelWriteRequest {
    fn into_create_command(self) -> Result<AdminChannelCreateCommand, ManagementError> {
        AdminChannelCreateCommand::new(
            self.name,
            self.channel_type,
            self.protocol,
            self.base_url,
            self.status,
            self.weight,
            self.priority,
            self.auto_ban,
            self.model_mapping,
            self.param_override,
            self.header_override,
            self.settings,
            self.tag,
        )
        .map_err(map_write_error)
    }

    fn into_update_command(self) -> Result<AdminChannelUpdateCommand, ManagementError> {
        AdminChannelUpdateCommand::new(
            self.name,
            self.channel_type,
            self.protocol,
            self.base_url,
            self.status,
            self.weight,
            self.priority,
            self.auto_ban,
            self.model_mapping,
            self.param_override,
            self.header_override,
            self.settings,
            self.tag,
        )
        .map_err(map_write_error)
    }
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
