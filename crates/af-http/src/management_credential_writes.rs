use af_admin::{
    AdminCredentialCreateCommand, AdminCredentialMultiKeyMode, AdminCredentialQuotaDimension,
    AdminCredentialSecret, AdminCredentialUpdateCommand, AdminRoutingWriteStatus,
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
    Present(Option<AdminCredentialSecretValue>),
}

impl<'de> Deserialize<'de> for AdminCredentialSecretInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<AdminCredentialSecretValue>::deserialize(deserializer).map(Self::Present)
    }
}

/// 管理端允许写入的闭合凭据明文对象；未知字段由 Serde 直接拒绝。
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum AdminCredentialSecretValue {
    ApiKey {
        api_key: String,
    },
    Oauth {
        access_token: String,
    },
    Bedrock {
        access_key_id: String,
        secret_access_key: String,
        #[serde(default)]
        session_token: Option<String>,
    },
    ServiceAccount {
        client_email: String,
        #[serde(default)]
        private_key_id: Option<String>,
        private_key: String,
    },
}

impl AdminCredentialSecretValue {
    fn into_secret(self) -> Result<AdminCredentialSecret, ManagementError> {
        let result = match self {
            Self::ApiKey { api_key } => AdminCredentialSecret::api_key(api_key),
            Self::Oauth { access_token } => AdminCredentialSecret::oauth_access_token(access_token),
            Self::Bedrock {
                access_key_id,
                secret_access_key,
                session_token,
            } => AdminCredentialSecret::bedrock(access_key_id, secret_access_key, session_token),
            Self::ServiceAccount {
                client_email,
                private_key_id,
                private_key,
            } => AdminCredentialSecret::service_account(client_email, private_key_id, private_key),
        };
        result.map_err(|_| ManagementError::InvalidRequest)
    }
}

/// 创建凭据；OAuth 可不带明文进入待授权态，其余明文仅用于生成密文。
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
            AdminCredentialSecretInput::Present(Some(secret)) => Some(secret.into_secret()?),
            AdminCredentialSecretInput::Present(None) => None,
            AdminCredentialSecretInput::Missing => return Err(ManagementError::InvalidRequest),
        };
        let args = (
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
        );
        match secret {
            Some(secret) => AdminCredentialCreateCommand::new(
                self.kind, secret, args.0, args.1, args.2, args.3, args.4, args.5, args.6, args.7,
                args.8, args.9, args.10, args.11, args.12, args.13,
            )
            .map_err(map_write_error),
            None => AdminCredentialCreateCommand::new_pending_oauth(
                self.kind, args.0, args.1, args.2, args.3, args.4, args.5, args.6, args.7, args.8,
                args.9, args.10, args.11, args.12, args.13,
            )
            .map_err(map_write_error),
        }
    }

    fn into_update_command(self) -> Result<AdminCredentialUpdateCommand, ManagementError> {
        let secret = match self.secret {
            AdminCredentialSecretInput::Missing => return Err(ManagementError::InvalidRequest),
            AdminCredentialSecretInput::Present(secret) => secret
                .map(AdminCredentialSecretValue::into_secret)
                .transpose()?,
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const SERVICE_ACCOUNT_PRIVATE_KEY: &str =
        include_str!("../../af-adapter/src/vertex/fixtures/service_account_private.pem");

    #[test]
    fn typed_secret_requests_accept_all_supported_shapes() {
        for (kind, secret) in [
            (
                "api_key",
                json!({"kind": "api_key", "api_key": "sk-private"}),
            ),
            (
                "oauth",
                json!({"kind": "oauth", "access_token": "oauth-private"}),
            ),
            (
                "bedrock",
                json!({
                    "kind": "bedrock",
                    "access_key_id": "AKIDEXAMPLE00000001",
                    "secret_access_key": "aws-secret-access-key",
                    "session_token": "aws-session-token"
                }),
            ),
            (
                "service_account",
                json!({
                    "kind": "service_account",
                    "client_email": "runtime@example.iam.gserviceaccount.com",
                    "private_key_id": "private-key-id",
                    "private_key": SERVICE_ACCOUNT_PRIVATE_KEY
                }),
            ),
        ] {
            let request =
                serde_json::from_value::<AdminCredentialWriteRequest>(request(kind, secret))
                    .expect("类型化凭据请求应可解析");
            assert!(request.into_create_command().is_ok());
        }
    }

    #[test]
    fn typed_secret_requests_cover_pending_oauth_and_missing_field_boundaries() {
        let mismatch = serde_json::from_value::<AdminCredentialWriteRequest>(request(
            "bedrock",
            json!({"kind": "api_key", "api_key": "sk-private"}),
        ))
        .unwrap();
        assert!(mismatch.into_create_command().is_err());

        let pending =
            serde_json::from_value::<AdminCredentialWriteRequest>(request("oauth", Value::Null))
                .unwrap();
        assert!(pending.into_create_command().is_ok());

        let non_oauth_pending =
            serde_json::from_value::<AdminCredentialWriteRequest>(request("api_key", Value::Null))
                .unwrap();
        assert!(non_oauth_pending.into_create_command().is_err());

        let service_account_with_token_uri = request(
            "service_account",
            json!({
                "kind": "service_account",
                "client_email": "runtime@example.iam.gserviceaccount.com",
                "private_key": SERVICE_ACCOUNT_PRIVATE_KEY,
                "token_uri": "https://untrusted.example/token"
            }),
        );
        assert!(
            serde_json::from_value::<AdminCredentialWriteRequest>(service_account_with_token_uri)
                .is_err()
        );

        let mut missing = request(
            "api_key",
            json!({"kind": "api_key", "api_key": "sk-private"}),
        );
        missing.as_object_mut().unwrap().remove("secret");
        let missing = serde_json::from_value::<AdminCredentialWriteRequest>(missing).unwrap();
        assert!(missing.into_update_command().is_err());
    }

    fn request(kind: &str, secret: Value) -> Value {
        json!({
            "kind": kind,
            "secret": secret,
            "status": "enabled",
            "multi_key_mode": null,
            "priority": 0,
            "weight": 10,
            "concurrency": null,
            "load_factor_micros": null,
            "rate_multiplier_micros": null,
            "schedulable": true,
            "parent_id": null,
            "quota_dimension": "global",
            "proxy_id": null,
            "oauth_provider": if kind == "oauth" { Value::String("codex".to_owned()) } else { Value::Null },
            "oauth_account_key": null,
            "oauth_project_id": null
        })
    }
}
