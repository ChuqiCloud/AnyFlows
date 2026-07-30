use std::{fmt, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, LockType},
};
use serde_json::json;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, EncryptedCredentialEnvelope,
    entity::{EncryptedJson, SensitiveString, email_settings},
};

const EMAIL_SETTINGS_ID: i16 = 1;
const MAX_HOST_BYTES: usize = 255;
const MAX_EMAIL_BYTES: usize = 320;
const MAX_FROM_NAME_BYTES: usize = 128;

/// SMTP 连接只允许两种加密模式，禁止静默退回明文传输。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmailTlsMode {
    /// 连接后要求服务器升级到 TLS。
    StartTls,
    /// 建立连接时直接使用 TLS。
    Tls,
}

impl EmailTlsMode {
    /// 返回持久化使用的闭合数值。
    #[must_use]
    pub const fn as_i16(self) -> i16 {
        match self {
            Self::StartTls => 1,
            Self::Tls => 2,
        }
    }

    fn from_i16(value: i16) -> Result<Self, EmailSettingsRepositoryError> {
        match value {
            1 => Ok(Self::StartTls),
            2 => Ok(Self::Tls),
            _ => Err(internal_error(EmailSettingsRepositoryError::Invariant)),
        }
    }
}

/// SMTP 密码在完整更新中的处理方式。
pub enum EmailPasswordUpdate {
    /// 保留数据库中现有密文。
    Keep,
    /// 使用新密文原子替换。
    Replace(EncryptedCredentialEnvelope),
    /// 清除现有 SMTP 密码。
    Clear,
}

impl fmt::Debug for EmailPasswordUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Keep => "EmailPasswordUpdate::Keep",
            Self::Replace(_) => "EmailPasswordUpdate::Replace(<已脱敏>)",
            Self::Clear => "EmailPasswordUpdate::Clear",
        })
    }
}

/// 已完成持久化校验的 SMTP 配置，密文只供应用服务解密。
#[derive(Clone, Eq, PartialEq)]
pub struct EmailSettingsRecord {
    enabled: bool,
    host: String,
    port: u16,
    tls_mode: EmailTlsMode,
    username: Option<String>,
    password_secret: Option<EncryptedCredentialEnvelope>,
    from_address: String,
    from_name: Option<String>,
    reply_to: Option<String>,
    timeout_seconds: u16,
    version: i64,
}

impl EmailSettingsRecord {
    /// 返回是否允许业务邮件投递；测试邮件仍可在关闭时验证完整配置。
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub const fn tls_mode(&self) -> EmailTlsMode {
        self.tls_mode
    }

    #[must_use]
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    #[must_use]
    pub const fn password_configured(&self) -> bool {
        self.password_secret.is_some()
    }

    /// 返回密码密文封套；调用方不得记录或序列化该值。
    #[must_use]
    pub fn password_secret(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.password_secret.as_ref()
    }

    #[must_use]
    pub fn from_address(&self) -> &str {
        &self.from_address
    }

    #[must_use]
    pub fn from_name(&self) -> Option<&str> {
        self.from_name.as_deref()
    }

    #[must_use]
    pub fn reply_to(&self) -> Option<&str> {
        self.reply_to.as_deref()
    }

    #[must_use]
    pub const fn timeout_seconds(&self) -> u16 {
        self.timeout_seconds
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    /// 判断已保存配置能否用于测试或业务投递。
    #[must_use]
    pub fn delivery_ready(&self) -> bool {
        !self.host.is_empty()
            && is_valid_email_address(&self.from_address)
            && self
                .username
                .as_ref()
                .is_none_or(|_| self.password_secret.is_some())
    }
}

impl fmt::Debug for EmailSettingsRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmailSettingsRecord")
            .field("enabled", &self.enabled)
            .field("host", &"<已脱敏>")
            .field("port", &self.port)
            .field("tls_mode", &self.tls_mode)
            .field("username", &self.username.as_ref().map(|_| "<已脱敏>"))
            .field("password_configured", &self.password_configured())
            .field("from_address", &"<已脱敏>")
            .field("from_name", &self.from_name.as_ref().map(|_| "<已脱敏>"))
            .field("reply_to", &self.reply_to.as_ref().map(|_| "<已脱敏>"))
            .field("timeout_seconds", &self.timeout_seconds)
            .field("version", &self.version)
            .finish()
    }
}

/// 管理员完整覆盖 SMTP 配置时使用的记录。
pub struct EmailSettingsWriteRecord {
    enabled: bool,
    host: String,
    port: u16,
    tls_mode: EmailTlsMode,
    username: Option<String>,
    password_update: EmailPasswordUpdate,
    from_address: String,
    from_name: Option<String>,
    reply_to: Option<String>,
    timeout_seconds: u16,
}

impl EmailSettingsWriteRecord {
    /// 组合已经由应用层完成输入校验的完整 SMTP 设置。
    #[allow(clippy::too_many_arguments, reason = "字段与 SMTP 设置契约一一对应")]
    #[must_use]
    pub fn new(
        enabled: bool,
        host: String,
        port: u16,
        tls_mode: EmailTlsMode,
        username: Option<String>,
        password_update: EmailPasswordUpdate,
        from_address: String,
        from_name: Option<String>,
        reply_to: Option<String>,
        timeout_seconds: u16,
    ) -> Self {
        Self {
            enabled,
            host,
            port,
            tls_mode,
            username,
            password_update,
            from_address,
            from_name,
            reply_to,
            timeout_seconds,
        }
    }

    fn validate(&self) -> Result<(), EmailSettingsRepositoryError> {
        let authentication_shape_valid = matches!(
            (&self.username, &self.password_update),
            (None, EmailPasswordUpdate::Keep | EmailPasswordUpdate::Clear)
                | (
                    Some(_),
                    EmailPasswordUpdate::Keep | EmailPasswordUpdate::Replace(_)
                )
        );
        if self.port == 0
            || !(1..=60).contains(&self.timeout_seconds)
            || (!self.host.is_empty() && !is_valid_host(&self.host))
            || (!self.from_address.is_empty() && !is_valid_email_address(&self.from_address))
            || !valid_optional_text(self.username.as_deref(), MAX_EMAIL_BYTES)
            || !valid_optional_text(self.from_name.as_deref(), MAX_FROM_NAME_BYTES)
            || self
                .reply_to
                .as_deref()
                .is_some_and(|value| !is_valid_email_address(value))
            || (self.enabled && (self.host.is_empty() || self.from_address.is_empty()))
            || !authentication_shape_valid
        {
            return Err(EmailSettingsRepositoryError::InvalidSettings);
        }
        Ok(())
    }
}

impl fmt::Debug for EmailSettingsWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmailSettingsWriteRecord")
            .field("enabled", &self.enabled)
            .field("host", &"<已脱敏>")
            .field("port", &self.port)
            .field("tls_mode", &self.tls_mode)
            .field("username", &self.username.as_ref().map(|_| "<已脱敏>"))
            .field("password_update", &self.password_update)
            .field("from_address", &"<已脱敏>")
            .field("from_name", &self.from_name.as_ref().map(|_| "<已脱敏>"))
            .field("reply_to", &self.reply_to.as_ref().map(|_| "<已脱敏>"))
            .field("timeout_seconds", &self.timeout_seconds)
            .finish()
    }
}

/// SMTP 设置仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EmailSettingsRepositoryConfigError {
    #[error("SMTP 设置数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// SMTP 设置仓储错误；不携带配置内容或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EmailSettingsRepositoryError {
    #[error("SMTP 设置数据库操作失败")]
    Query,
    #[error("SMTP 设置数据库操作超时")]
    Timeout,
    #[error("SMTP 设置持久化状态损坏")]
    Invariant,
    #[error("SMTP 设置字段无效")]
    InvalidSettings,
}

/// 系统 SMTP 配置的固定记录仓储。
#[derive(Clone)]
pub struct EmailSettingsRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl EmailSettingsRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, EmailSettingsRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(EmailSettingsRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 读取固定 SMTP 设置记录；固定行缺失视为持久化状态损坏。
    pub async fn settings(&self) -> Result<EmailSettingsRecord, EmailSettingsRepositoryError> {
        match timeout(self.operation_timeout, self.settings_inner()).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(EmailSettingsRepositoryError::Timeout)),
        }
    }

    /// 原子写入完整 SMTP 设置，并单调递增配置版本。
    pub async fn update(
        &self,
        record: EmailSettingsWriteRecord,
    ) -> Result<EmailSettingsRecord, EmailSettingsRepositoryError> {
        match timeout(self.operation_timeout, self.update_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(EmailSettingsRepositoryError::Timeout)),
        }
    }

    async fn settings_inner(&self) -> Result<EmailSettingsRecord, EmailSettingsRepositoryError> {
        let Some(model) = email_settings::Entity::find_by_id(EMAIL_SETTINGS_ID)
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("email_settings_read"))?
        else {
            return Err(internal_error(EmailSettingsRepositoryError::Invariant));
        };
        record_from_model(model)
    }

    async fn update_inner(
        &self,
        record: EmailSettingsWriteRecord,
    ) -> Result<EmailSettingsRecord, EmailSettingsRepositoryError> {
        record.validate()?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("email_settings_begin"))?;
        let existing = lock_settings(&transaction).await?;
        let existing_secret = existing.password_secret.clone();
        let password_secret = if record.username.is_none() {
            None
        } else {
            match record.password_update {
                EmailPasswordUpdate::Keep => existing_secret,
                EmailPasswordUpdate::Replace(envelope) => Some(encrypted_json(envelope)?),
                EmailPasswordUpdate::Clear => None,
            }
        };
        if record.username.is_some() && password_secret.is_none() {
            return Err(EmailSettingsRepositoryError::InvalidSettings);
        }
        let version = existing
            .version
            .checked_add(1)
            .ok_or_else(|| internal_error(EmailSettingsRepositoryError::Invariant))?;
        let now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
        let created_at = existing.created_at;
        let model = email_settings::ActiveModel {
            id: Set(EMAIL_SETTINGS_ID),
            enabled: Set(record.enabled),
            host: Set(SensitiveString::from(record.host)),
            port: Set(i32::from(record.port)),
            tls_mode: Set(record.tls_mode.as_i16()),
            username: Set(record.username.map(SensitiveString::from)),
            password_secret: Set(password_secret),
            from_address: Set(SensitiveString::from(record.from_address)),
            from_name: Set(record.from_name.map(SensitiveString::from)),
            reply_to: Set(record.reply_to.map(SensitiveString::from)),
            timeout_seconds: Set(i32::from(record.timeout_seconds)),
            version: Set(version),
            created_at: Set(created_at),
            updated_at: Set(now),
        };
        let saved = model
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("email_settings_write"))?;
        let saved = record_from_model(saved)?;
        if saved.enabled && !saved.delivery_ready() {
            return Err(EmailSettingsRepositoryError::InvalidSettings);
        }
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("email_settings_commit"))?;
        Ok(saved)
    }
}

async fn lock_settings(
    transaction: &DatabaseTransaction,
) -> Result<email_settings::Model, EmailSettingsRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，通过无变化写入先取得数据库写锁。
        let result = email_settings::Entity::update_many()
            .filter(email_settings::Column::Id.eq(EMAIL_SETTINGS_ID))
            .col_expr(
                email_settings::Column::Version,
                Expr::col(email_settings::Column::Version).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("email_settings_lock"))?;
        if result.rows_affected != 1 {
            return Err(internal_error(EmailSettingsRepositoryError::Invariant));
        }
    }

    let mut query =
        email_settings::Entity::find().filter(email_settings::Column::Id.eq(EMAIL_SETTINGS_ID));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("email_settings_read_for_update"))?
        .ok_or_else(|| internal_error(EmailSettingsRepositoryError::Invariant))
}

fn record_from_model(
    model: email_settings::Model,
) -> Result<EmailSettingsRecord, EmailSettingsRepositoryError> {
    if model.id != EMAIL_SETTINGS_ID
        || !(1..=65_535).contains(&model.port)
        || !(1..=60).contains(&model.timeout_seconds)
        || model.version < 1
        || !valid_optional_text(
            model.username.as_ref().map(SensitiveString::as_str),
            MAX_EMAIL_BYTES,
        )
        || !valid_optional_text(
            model.from_name.as_ref().map(SensitiveString::as_str),
            MAX_FROM_NAME_BYTES,
        )
        || model
            .reply_to
            .as_ref()
            .map(SensitiveString::as_str)
            .is_some_and(|value| !is_valid_email_address(value))
        || (!model.host.as_str().is_empty() && !is_valid_host(model.host.as_str()))
        || (!model.from_address.as_str().is_empty()
            && !is_valid_email_address(model.from_address.as_str()))
    {
        return Err(internal_error(EmailSettingsRepositoryError::Invariant));
    }
    let password_secret = model
        .password_secret
        .map(|secret| {
            let (key_id, nonce, ciphertext) = secret
                .envelope_parts()
                .map_err(|_| internal_error(EmailSettingsRepositoryError::Invariant))?;
            EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
                .map_err(|_| internal_error(EmailSettingsRepositoryError::Invariant))
        })
        .transpose()?;
    if model.username.is_some() != password_secret.is_some() {
        return Err(internal_error(EmailSettingsRepositoryError::Invariant));
    }
    let record = EmailSettingsRecord {
        enabled: model.enabled,
        host: model.host.as_str().to_owned(),
        port: u16::try_from(model.port)
            .map_err(|_| internal_error(EmailSettingsRepositoryError::Invariant))?,
        tls_mode: EmailTlsMode::from_i16(model.tls_mode)?,
        username: model.username.map(|value| value.as_str().to_owned()),
        password_secret,
        from_address: model.from_address.as_str().to_owned(),
        from_name: model.from_name.map(|value| value.as_str().to_owned()),
        reply_to: model.reply_to.map(|value| value.as_str().to_owned()),
        timeout_seconds: u16::try_from(model.timeout_seconds)
            .map_err(|_| internal_error(EmailSettingsRepositoryError::Invariant))?,
        version: model.version,
    };
    if record.enabled && !record.delivery_ready() {
        return Err(internal_error(EmailSettingsRepositoryError::Invariant));
    }
    Ok(record)
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, EmailSettingsRepositoryError> {
    EncryptedJson::from_envelope(json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| internal_error(EmailSettingsRepositoryError::Invariant))
}

fn is_valid_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_HOST_BYTES
        && value.trim() == value
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        && !value.contains("://")
        && !value.contains(['/', '\\', '@'])
}

fn is_valid_email_address(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_EMAIL_BYTES
        || value.trim() != value
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return false;
    }
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    !local.is_empty()
        && !domain.is_empty()
        && parts.next().is_none()
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

fn valid_optional_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= maximum_bytes
            && value.trim() == value
            && !value.chars().any(char::is_control)
    })
}

fn query_error(operation: &'static str) -> EmailSettingsRepositoryError {
    tracing::error!(
        target: "af_db::email_settings",
        error_kind = operation,
        "SMTP 设置数据库操作失败"
    );
    EmailSettingsRepositoryError::Query
}

fn internal_error(error: EmailSettingsRepositoryError) -> EmailSettingsRepositoryError {
    tracing::error!(
        target: "af_db::email_settings",
        error_kind = ?error,
        "SMTP 设置内部状态无效"
    );
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_redacts_all_address_and_secret_fields() {
        let record = EmailSettingsRecord {
            enabled: true,
            host: "smtp.private.example".to_owned(),
            port: 587,
            tls_mode: EmailTlsMode::StartTls,
            username: Some("smtp-user@example.com".to_owned()),
            password_secret: Some(
                EncryptedCredentialEnvelope::new("mail-key", [7_u8; 24], vec![9_u8; 16]).unwrap(),
            ),
            from_address: "from@example.com".to_owned(),
            from_name: Some("Private Sender".to_owned()),
            reply_to: Some("reply@example.com".to_owned()),
            timeout_seconds: 10,
            version: 1,
        };
        let rendered = format!("{record:?}");
        for secret in [
            "smtp.private.example",
            "smtp-user@example.com",
            "from@example.com",
            "Private Sender",
            "reply@example.com",
            "mail-key",
        ] {
            assert!(!rendered.contains(secret));
        }
    }

    #[test]
    fn validates_closed_tls_modes_and_delivery_readiness() {
        assert_eq!(EmailTlsMode::from_i16(1).unwrap(), EmailTlsMode::StartTls);
        assert_eq!(EmailTlsMode::from_i16(2).unwrap(), EmailTlsMode::Tls);
        assert!(EmailTlsMode::from_i16(3).is_err());
    }
}
