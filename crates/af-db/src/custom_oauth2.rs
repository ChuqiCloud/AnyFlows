use std::{fmt, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use serde_json::json;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use url::{Host, Url};

use crate::{DatabasePool, EncryptedCredentialEnvelope, entity::custom_oauth2_providers};

const MAX_PROVIDER_KEY_BYTES: usize = 32;
const MAX_DISPLAY_NAME_BYTES: usize = 128;
const MAX_CLIENT_ID_BYTES: usize = 255;
const MAX_ENDPOINT_BYTES: usize = 2_048;
const MAX_SCOPE_BYTES: usize = 2 * 1_024;
const MAX_SUBJECT_FIELD_BYTES: usize = 64;
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

/// 已通过持久化边界校验的自定义 OAuth2 Provider 记录。
#[derive(Clone, Eq, PartialEq)]
pub struct CustomOAuth2ProviderRecord {
    provider_key: String,
    display_name: String,
    client_id: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    scope: String,
    subject_field: String,
    enabled: bool,
    client_secret: Option<EncryptedCredentialEnvelope>,
    version: i64,
}

impl CustomOAuth2ProviderRecord {
    #[must_use]
    pub fn provider_key(&self) -> &str {
        &self.provider_key
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    #[must_use]
    pub fn authorization_endpoint(&self) -> &str {
        &self.authorization_endpoint
    }

    #[must_use]
    pub fn token_endpoint(&self) -> &str {
        &self.token_endpoint
    }

    #[must_use]
    pub fn userinfo_endpoint(&self) -> &str {
        &self.userinfo_endpoint
    }

    #[must_use]
    pub fn scope(&self) -> &str {
        &self.scope
    }

    #[must_use]
    pub fn subject_field(&self) -> &str {
        &self.subject_field
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn secret_configured(&self) -> bool {
        self.client_secret.is_some()
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    /// 只有启用且存在 Client Secret 时，后续运行时才可使用该 Provider。
    #[must_use]
    pub const fn available(&self) -> bool {
        self.enabled && self.client_secret.is_some()
    }

    #[must_use]
    pub fn client_secret(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.client_secret.as_ref()
    }
}

impl fmt::Debug for CustomOAuth2ProviderRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomOAuth2ProviderRecord")
            .field("provider_key", &self.provider_key)
            .field("display_name", &"<已脱敏>")
            .field("client_id", &"<已脱敏>")
            .field("endpoints", &"<已脱敏>")
            .field("scope", &"<已脱敏>")
            .field("subject_field", &"<已脱敏>")
            .field("enabled", &self.enabled)
            .field("secret_configured", &self.client_secret.is_some())
            .field("version", &self.version)
            .finish()
    }
}

/// Client Secret 的 CAS 更新语义；明文只在 af-admin 加密后进入此类型。
pub enum CustomOAuth2ProviderSecretUpdate {
    Keep,
    Replace(EncryptedCredentialEnvelope),
    Clear,
}

impl fmt::Debug for CustomOAuth2ProviderSecretUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Keep => "CustomOAuth2ProviderSecretUpdate::Keep",
            Self::Replace(_) => "CustomOAuth2ProviderSecretUpdate::Replace(<已脱敏>)",
            Self::Clear => "CustomOAuth2ProviderSecretUpdate::Clear",
        })
    }
}

/// 管理员保存自定义 Provider 时使用的完整配置；不实现 Debug 明文回显。
pub struct CustomOAuth2ProviderWriteRecord {
    expected_version: i64,
    display_name: String,
    client_id: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    scope: String,
    subject_field: String,
    enabled: bool,
    client_secret: CustomOAuth2ProviderSecretUpdate,
}

impl CustomOAuth2ProviderWriteRecord {
    #[allow(
        clippy::too_many_arguments,
        reason = "Provider 配置是一个不可拆分的 CAS 快照"
    )]
    #[must_use]
    pub fn new(
        expected_version: i64,
        display_name: String,
        client_id: String,
        authorization_endpoint: String,
        token_endpoint: String,
        userinfo_endpoint: String,
        scope: String,
        subject_field: String,
        enabled: bool,
        client_secret: CustomOAuth2ProviderSecretUpdate,
    ) -> Self {
        Self {
            expected_version,
            display_name,
            client_id,
            authorization_endpoint,
            token_endpoint,
            userinfo_endpoint,
            scope,
            subject_field,
            enabled,
            client_secret,
        }
    }
}

impl fmt::Debug for CustomOAuth2ProviderWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CustomOAuth2ProviderWriteRecord(<已脱敏>)")
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CustomOAuth2ProviderRepositoryError {
    #[error("自定义 OAuth2 Provider 输入无效")]
    InvalidInput,
    #[error("自定义 OAuth2 Provider 发生并发更新")]
    ConcurrentUpdate,
    #[error("自定义 OAuth2 Provider 数据库查询失败")]
    Query,
    #[error("自定义 OAuth2 Provider 数据库操作超时")]
    Timeout,
    #[error("自定义 OAuth2 Provider 持久化状态无效")]
    Invariant,
}

#[derive(Clone)]
pub struct CustomOAuth2ProviderRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl CustomOAuth2ProviderRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, CustomOAuth2ProviderRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(CustomOAuth2ProviderRepositoryError::InvalidInput);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    #[must_use]
    pub fn with_default_timeout(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
        }
    }

    pub async fn providers(
        &self,
    ) -> Result<Vec<CustomOAuth2ProviderRecord>, CustomOAuth2ProviderRepositoryError> {
        match timeout(self.operation_timeout, self.providers_inner()).await {
            Ok(result) => result,
            Err(_) => Err(CustomOAuth2ProviderRepositoryError::Timeout),
        }
    }

    pub async fn provider(
        &self,
        provider_key: &str,
    ) -> Result<Option<CustomOAuth2ProviderRecord>, CustomOAuth2ProviderRepositoryError> {
        match timeout(self.operation_timeout, self.provider_inner(provider_key)).await {
            Ok(result) => result,
            Err(_) => Err(CustomOAuth2ProviderRepositoryError::Timeout),
        }
    }

    /// 以版本号执行创建或更新；`expected_version = 0` 只允许创建不存在的记录。
    pub async fn save_provider(
        &self,
        provider_key: &str,
        write: CustomOAuth2ProviderWriteRecord,
    ) -> Result<CustomOAuth2ProviderRecord, CustomOAuth2ProviderRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.save_provider_inner(provider_key, write),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(CustomOAuth2ProviderRepositoryError::Timeout),
        }
    }

    async fn providers_inner(
        &self,
    ) -> Result<Vec<CustomOAuth2ProviderRecord>, CustomOAuth2ProviderRepositoryError> {
        let models = custom_oauth2_providers::Entity::find()
            .order_by_asc(custom_oauth2_providers::Column::ProviderKey)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_provider_list"))?;
        models.into_iter().map(record_from_model).collect()
    }

    async fn provider_inner(
        &self,
        provider_key: &str,
    ) -> Result<Option<CustomOAuth2ProviderRecord>, CustomOAuth2ProviderRepositoryError> {
        validate_provider_key(provider_key)?;
        custom_oauth2_providers::Entity::find_by_id(provider_key.to_owned())
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_provider_read"))?
            .map(record_from_model)
            .transpose()
    }

    async fn save_provider_inner(
        &self,
        provider_key: &str,
        write: CustomOAuth2ProviderWriteRecord,
    ) -> Result<CustomOAuth2ProviderRecord, CustomOAuth2ProviderRepositoryError> {
        validate_provider_key(provider_key)?;
        let fields = ValidatedFields::from_write(write)?;
        let current = self.provider_inner(provider_key).await?;
        match (current, fields.expected_version) {
            (None, 0) => {
                if matches!(fields.secret_update, CustomOAuth2ProviderSecretUpdate::Keep)
                    || (fields.secret.is_none() && fields.enabled)
                {
                    return Err(CustomOAuth2ProviderRepositoryError::InvalidInput);
                }
                let now = TimeDateTimeWithTimeZone::now_utc();
                custom_oauth2_providers::ActiveModel {
                    provider_key: Set(provider_key.to_owned()),
                    display_name: Set(fields.display_name),
                    client_id: Set(fields.client_id),
                    authorization_endpoint: Set(fields.authorization_endpoint),
                    token_endpoint: Set(fields.token_endpoint),
                    userinfo_endpoint: Set(fields.userinfo_endpoint),
                    scope: Set(fields.scope),
                    subject_field: Set(fields.subject_field),
                    enabled: Set(fields.enabled),
                    client_secret: Set(fields.secret.map(encrypted_json).transpose()?),
                    version: Set(1),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| CustomOAuth2ProviderRepositoryError::ConcurrentUpdate)?;
                self.provider_inner(provider_key)
                    .await?
                    .ok_or_else(|| internal(CustomOAuth2ProviderRepositoryError::Invariant))
            }
            (Some(current), expected_version) if expected_version >= 1 => {
                let secret = match fields.secret_update {
                    CustomOAuth2ProviderSecretUpdate::Keep => current.client_secret,
                    CustomOAuth2ProviderSecretUpdate::Replace(secret) => Some(secret),
                    CustomOAuth2ProviderSecretUpdate::Clear => None,
                };
                if fields.enabled && secret.is_none() {
                    return Err(CustomOAuth2ProviderRepositoryError::InvalidInput);
                }
                let version = expected_version
                    .checked_add(1)
                    .ok_or_else(|| internal(CustomOAuth2ProviderRepositoryError::Invariant))?;
                let result = custom_oauth2_providers::Entity::update_many()
                    .filter(custom_oauth2_providers::Column::ProviderKey.eq(provider_key))
                    .filter(custom_oauth2_providers::Column::Version.eq(expected_version))
                    .col_expr(
                        custom_oauth2_providers::Column::DisplayName,
                        Expr::value(fields.display_name),
                    )
                    .col_expr(
                        custom_oauth2_providers::Column::ClientId,
                        Expr::value(fields.client_id),
                    )
                    .col_expr(
                        custom_oauth2_providers::Column::AuthorizationEndpoint,
                        Expr::value(fields.authorization_endpoint),
                    )
                    .col_expr(
                        custom_oauth2_providers::Column::TokenEndpoint,
                        Expr::value(fields.token_endpoint),
                    )
                    .col_expr(
                        custom_oauth2_providers::Column::UserinfoEndpoint,
                        Expr::value(fields.userinfo_endpoint),
                    )
                    .col_expr(
                        custom_oauth2_providers::Column::Scope,
                        Expr::value(fields.scope),
                    )
                    .col_expr(
                        custom_oauth2_providers::Column::SubjectField,
                        Expr::value(fields.subject_field),
                    )
                    .col_expr(
                        custom_oauth2_providers::Column::Enabled,
                        Expr::value(fields.enabled),
                    )
                    .col_expr(
                        custom_oauth2_providers::Column::ClientSecret,
                        Expr::value(secret.map(encrypted_json).transpose()?),
                    )
                    .col_expr(
                        custom_oauth2_providers::Column::Version,
                        Expr::value(version),
                    )
                    .col_expr(
                        custom_oauth2_providers::Column::UpdatedAt,
                        Expr::value(TimeDateTimeWithTimeZone::now_utc()),
                    )
                    .exec(self.pool.connection())
                    .with_subscriber(NoSubscriber::default())
                    .await
                    .map_err(|_| query("custom_oauth2_provider_update"))?;
                match result.rows_affected {
                    1 => self
                        .provider_inner(provider_key)
                        .await?
                        .ok_or_else(|| internal(CustomOAuth2ProviderRepositoryError::Invariant)),
                    0 => Err(CustomOAuth2ProviderRepositoryError::ConcurrentUpdate),
                    _ => Err(internal(CustomOAuth2ProviderRepositoryError::Invariant)),
                }
            }
            (None, _) | (Some(_), 0) => Err(CustomOAuth2ProviderRepositoryError::ConcurrentUpdate),
            (_, _) => Err(CustomOAuth2ProviderRepositoryError::InvalidInput),
        }
    }
}

struct ValidatedFields {
    expected_version: i64,
    display_name: String,
    client_id: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    scope: String,
    subject_field: String,
    enabled: bool,
    secret: Option<EncryptedCredentialEnvelope>,
    secret_update: CustomOAuth2ProviderSecretUpdate,
}

impl ValidatedFields {
    fn from_write(
        write: CustomOAuth2ProviderWriteRecord,
    ) -> Result<Self, CustomOAuth2ProviderRepositoryError> {
        if write.expected_version < 0
            || write.display_name.is_empty()
            || write.display_name.len() > MAX_DISPLAY_NAME_BYTES
            || write.display_name.chars().any(char::is_control)
            || write.client_id.is_empty()
            || write.client_id.len() > MAX_CLIENT_ID_BYTES
            || !write
                .client_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(CustomOAuth2ProviderRepositoryError::InvalidInput);
        }
        let authorization_endpoint = validate_endpoint(&write.authorization_endpoint)?;
        let token_endpoint = validate_endpoint(&write.token_endpoint)?;
        let userinfo_endpoint = validate_endpoint(&write.userinfo_endpoint)?;
        if !same_origin(&authorization_endpoint, &token_endpoint)
            || !same_origin(&authorization_endpoint, &userinfo_endpoint)
            || write.scope.is_empty()
            || write.scope.len() > MAX_SCOPE_BYTES
            || write.scope.trim() != write.scope
            || !write.scope.is_ascii()
            || write.scope.split(' ').any(|token| {
                token.is_empty()
                    || !token.bytes().all(|byte| {
                        byte == 0x21
                            || (0x23..=0x5b).contains(&byte)
                            || (0x5d..=0x7e).contains(&byte)
                    })
            })
            || write.subject_field.is_empty()
            || write.subject_field.len() > MAX_SUBJECT_FIELD_BYTES
            || !write
                .subject_field
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(CustomOAuth2ProviderRepositoryError::InvalidInput);
        }
        let (secret, secret_update) = match write.client_secret {
            CustomOAuth2ProviderSecretUpdate::Keep => {
                (None, CustomOAuth2ProviderSecretUpdate::Keep)
            }
            CustomOAuth2ProviderSecretUpdate::Replace(secret) => (
                Some(secret.clone()),
                CustomOAuth2ProviderSecretUpdate::Replace(secret),
            ),
            CustomOAuth2ProviderSecretUpdate::Clear => {
                (None, CustomOAuth2ProviderSecretUpdate::Clear)
            }
        };
        Ok(Self {
            expected_version: write.expected_version,
            display_name: write.display_name,
            client_id: write.client_id,
            authorization_endpoint: authorization_endpoint.into(),
            token_endpoint: token_endpoint.into(),
            userinfo_endpoint: userinfo_endpoint.into(),
            scope: write.scope,
            subject_field: write.subject_field,
            enabled: write.enabled,
            secret,
            secret_update,
        })
    }
}

fn record_from_model(
    model: custom_oauth2_providers::Model,
) -> Result<CustomOAuth2ProviderRecord, CustomOAuth2ProviderRepositoryError> {
    let secret = model
        .client_secret
        .map(envelope_from_encrypted_json)
        .transpose()?;
    if model.version < 1 || (model.enabled && secret.is_none()) {
        return Err(internal(CustomOAuth2ProviderRepositoryError::Invariant));
    }
    validate_provider_key(&model.provider_key)?;
    validate_endpoint(&model.authorization_endpoint)?;
    validate_endpoint(&model.token_endpoint)?;
    validate_endpoint(&model.userinfo_endpoint)?;
    Ok(CustomOAuth2ProviderRecord {
        provider_key: model.provider_key,
        display_name: model.display_name,
        client_id: model.client_id,
        authorization_endpoint: model.authorization_endpoint,
        token_endpoint: model.token_endpoint,
        userinfo_endpoint: model.userinfo_endpoint,
        scope: model.scope,
        subject_field: model.subject_field,
        enabled: model.enabled,
        client_secret: secret,
        version: model.version,
    })
}

fn validate_provider_key(value: &str) -> Result<(), CustomOAuth2ProviderRepositoryError> {
    if value.len() <= "custom_".len()
        || value.len() > MAX_PROVIDER_KEY_BYTES
        || !value.starts_with("custom_")
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
    {
        return Err(CustomOAuth2ProviderRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_endpoint(value: &str) -> Result<Url, CustomOAuth2ProviderRepositoryError> {
    if value.is_empty() || value.len() > MAX_ENDPOINT_BYTES {
        return Err(CustomOAuth2ProviderRepositoryError::InvalidInput);
    }
    let url = Url::parse(value).map_err(|_| CustomOAuth2ProviderRepositoryError::InvalidInput)?;
    let Some(Host::Domain(host)) = url.host() else {
        return Err(CustomOAuth2ProviderRepositoryError::InvalidInput);
    };
    if url.scheme() != "https"
        || host.is_empty()
        || !host.is_ascii()
        || host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
        || url.path().is_empty()
    {
        return Err(CustomOAuth2ProviderRepositoryError::InvalidInput);
    }
    Ok(url)
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left
            .host_str()
            .zip(right.host_str())
            .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        && left.port_or_known_default() == right.port_or_known_default()
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<crate::entity::EncryptedJson, CustomOAuth2ProviderRepositoryError> {
    crate::entity::EncryptedJson::from_envelope(json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| CustomOAuth2ProviderRepositoryError::InvalidInput)
}

fn envelope_from_encrypted_json(
    encrypted: crate::entity::EncryptedJson,
) -> Result<EncryptedCredentialEnvelope, CustomOAuth2ProviderRepositoryError> {
    let (key_id, nonce, ciphertext) = encrypted
        .envelope_parts()
        .map_err(|_| CustomOAuth2ProviderRepositoryError::Invariant)?;
    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
        .map_err(|_| CustomOAuth2ProviderRepositoryError::Invariant)
}

fn query(operation: &'static str) -> CustomOAuth2ProviderRepositoryError {
    tracing::error!(
        target: "af_db::custom_oauth2",
        error_kind = operation,
        "自定义 OAuth2 Provider 数据库查询失败"
    );
    CustomOAuth2ProviderRepositoryError::Query
}

fn internal(error: CustomOAuth2ProviderRepositoryError) -> CustomOAuth2ProviderRepositoryError {
    tracing::error!(
        target: "af_db::custom_oauth2",
        error_kind = ?error,
        "自定义 OAuth2 Provider 持久化状态无效"
    );
    error
}
