use af_admin::{
    AdminCredentialCreateCommand, AdminCredentialMultiKeyMode, AdminCredentialQuotaDimension,
    AdminCredentialUpdateCommand, AdminRoutingWriteStatus,
};
use af_domain::{CredentialId, CredentialKind};
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    response::Response,
};
use http::StatusCode;
use serde::{Deserialize, Deserializer};

use crate::{
    chat_completions::HttpState,
    management_channel_writes::map_write_error,
    management_channels::{
        no_store_empty, no_store_json, parse_channel_id, parse_credential_id, status_json,
    },
    management_credentials::AdminCredentialResponse,
    management_error::ManagementError,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
/// 管理端创建或完整更新凭据时使用的配置正文。
pub(crate) struct AdminCredentialWriteRequest {
    kind: CredentialKind,
    #[serde(default)]
    secret: AdminCredentialSecretInput,
    status: AdminRoutingWriteStatus,
    multi_key_mode: Option<AdminCredentialMultiKeyMode>,
    priority: i32,
    weight: i32,
    concurrency: Option<i32>,
    load_factor_micros: Option<i64>,
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    parent_id: Option<i64>,
    quota_dimension: AdminCredentialQuotaDimension,
    proxy_id: Option<i64>,
    oauth_provider: Option<String>,
    oauth_account_key: Option<String>,
    oauth_project_id: Option<String>,
}

/// 区分字段缺失、显式 null 与有效明文，避免 Serde 将缺失字段折叠为 None。
#[derive(Default)]
enum AdminCredentialSecretInput {
    #[default]
    Missing,
    Present(Option<String>),
}

impl<'de> Deserialize<'de> for AdminCredentialSecretInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer).map(Self::Present)
    }
}

/// 创建凭据；明文仅用于生成密文，不随响应返回。
pub(crate) async fn create_admin_credential(
    State(state): State<HttpState>,
    Path(channel_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminCredentialWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let credential = writer
        .create_credential(
            authentication.principal(),
            channel_id,
            request.into_create_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminCredentialResponse::from_credential(&credential),
    ))
}

/// 完整更新凭据配置；secret 为空时保留已有密文。
pub(crate) async fn update_admin_credential(
    State(state): State<HttpState>,
    Path((channel_id, credential_id)): Path<(String, String)>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminCredentialWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let credential_id = parse_credential_id(&credential_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let credential = writer
        .update_credential(
            authentication.principal(),
            channel_id,
            credential_id,
            request.into_update_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(no_store_json(AdminCredentialResponse::from_credential(
        &credential,
    )))
}

/// 安全软删除凭据及其影子后代。
pub(crate) async fn delete_admin_credential(
    State(state): State<HttpState>,
    Path((channel_id, credential_id)): Path<(String, String)>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let credential_id = parse_credential_id(&credential_id)?;
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    writer
        .delete_credential(authentication.principal(), channel_id, credential_id)
        .await
        .map_err(map_write_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

impl AdminCredentialWriteRequest {
    fn into_create_command(self) -> Result<AdminCredentialCreateCommand, ManagementError> {
        let secret = match self.secret {
            AdminCredentialSecretInput::Present(Some(secret)) => secret,
            AdminCredentialSecretInput::Missing | AdminCredentialSecretInput::Present(None) => {
                return Err(ManagementError::InvalidRequest);
            }
        };
        AdminCredentialCreateCommand::new(
            self.kind,
            secret,
            self.status,
            self.multi_key_mode,
            self.priority,
            self.weight,
            self.concurrency,
            self.load_factor_micros,
            self.rate_multiplier_micros,
            self.schedulable,
            parse_parent_id(self.parent_id)?,
            self.quota_dimension,
            self.proxy_id,
            self.oauth_provider,
            self.oauth_account_key,
            self.oauth_project_id,
        )
        .map_err(map_write_error)
    }

    fn into_update_command(self) -> Result<AdminCredentialUpdateCommand, ManagementError> {
        let secret = match self.secret {
            AdminCredentialSecretInput::Missing => return Err(ManagementError::InvalidRequest),
            AdminCredentialSecretInput::Present(secret) => secret,
        };
        AdminCredentialUpdateCommand::new(
            self.kind,
            secret,
            self.status,
            self.multi_key_mode,
            self.priority,
            self.weight,
            self.concurrency,
            self.load_factor_micros,
            self.rate_multiplier_micros,
            self.schedulable,
            parse_parent_id(self.parent_id)?,
            self.quota_dimension,
            self.proxy_id,
            self.oauth_provider,
            self.oauth_account_key,
            self.oauth_project_id,
        )
        .map_err(map_write_error)
    }
}

fn parse_parent_id(value: Option<i64>) -> Result<Option<CredentialId>, ManagementError> {
    value
        .map(|value| CredentialId::new(value).map_err(|_| ManagementError::InvalidRequest))
        .transpose()
}
