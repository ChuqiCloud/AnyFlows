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
    entity::{EncryptedJson, SensitiveString, network_settings},
};

const NETWORK_SETTINGS_ID: i16 = 1;
const MAX_PROXY_HOST_BYTES: usize = 255;
const MAX_PROXY_USERNAME_BYTES: usize = 320;

/// 全局出站网络模式使用闭合集合持久化，未知值一律视为状态损坏。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkSettingsMode {
    /// 继承启动期 `[http_client]` 配置。
    Inherit,
    /// 显式忽略系统代理并直连公网目标。
    Direct,
    Http,
    Https,
    Socks5,
    Socks5h,
}

impl NetworkSettingsMode {
    #[must_use]
    pub const fn is_proxy(self) -> bool {
        matches!(
            self,
            Self::Http | Self::Https | Self::Socks5 | Self::Socks5h
        )
    }

    #[must_use]
    pub const fn scheme(self) -> Option<&'static str> {
        match self {
            Self::Inherit | Self::Direct => None,
            Self::Http => Some("http"),
            Self::Https => Some("https"),
            Self::Socks5 => Some("socks5"),
            Self::Socks5h => Some("socks5h"),
        }
    }

    const fn as_i16(self) -> i16 {
        match self {
            Self::Inherit => 1,
            Self::Direct => 2,
            Self::Http => 3,
            Self::Https => 4,
            Self::Socks5 => 5,
            Self::Socks5h => 6,
        }
    }

    fn from_i16(value: i16) -> Result<Self, NetworkSettingsRepositoryError> {
        match value {
            1 => Ok(Self::Inherit),
            2 => Ok(Self::Direct),
            3 => Ok(Self::Http),
            4 => Ok(Self::Https),
            5 => Ok(Self::Socks5),
            6 => Ok(Self::Socks5h),
            _ => Err(internal_error(NetworkSettingsRepositoryError::Invariant)),
        }
    }
}

/// 代理密码在完整更新中的处理方式。
pub enum ProxyPasswordUpdate {
    Keep,
    Replace(EncryptedCredentialEnvelope),
    Clear,
}

impl fmt::Debug for ProxyPasswordUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Keep => "ProxyPasswordUpdate::Keep",
            Self::Replace(_) => "ProxyPasswordUpdate::Replace(<已脱敏>)",
            Self::Clear => "ProxyPasswordUpdate::Clear",
        })
    }
}

/// 已完成持久化校验的全局出站网络设置。
#[derive(Clone, Eq, PartialEq)]
pub struct NetworkSettingsRecord {
    mode: NetworkSettingsMode,
    proxy_host: Option<String>,
    proxy_port: Option<u16>,
    username: Option<String>,
    password_secret: Option<EncryptedCredentialEnvelope>,
    trust_proxy_dns: bool,
    version: i64,
}

impl NetworkSettingsRecord {
    #[must_use]
    pub const fn mode(&self) -> NetworkSettingsMode {
        self.mode
    }

    #[must_use]
    pub fn proxy_host(&self) -> Option<&str> {
        self.proxy_host.as_deref()
    }

    #[must_use]
    pub const fn proxy_port(&self) -> Option<u16> {
        self.proxy_port
    }

    #[must_use]
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    #[must_use]
    pub const fn password_configured(&self) -> bool {
        self.password_secret.is_some()
    }

    /// 返回代理密码密文封套；调用方不得记录或序列化该值。
    #[must_use]
    pub fn password_secret(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.password_secret.as_ref()
    }

    #[must_use]
    pub const fn trust_proxy_dns(&self) -> bool {
        self.trust_proxy_dns
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

impl fmt::Debug for NetworkSettingsRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NetworkSettingsRecord")
            .field("mode", &self.mode)
            .field("proxy_host", &self.proxy_host.as_ref().map(|_| "<已脱敏>"))
            .field("proxy_port", &self.proxy_port)
            .field("username", &self.username.as_ref().map(|_| "<已脱敏>"))
            .field("password_configured", &self.password_configured())
            .field("trust_proxy_dns", &self.trust_proxy_dns)
            .field("version", &self.version)
            .finish()
    }
}

/// 管理员完整覆盖出站网络设置时使用的记录。
pub struct NetworkSettingsWriteRecord {
    mode: NetworkSettingsMode,
    proxy_host: Option<String>,
    proxy_port: Option<u16>,
    username: Option<String>,
    password_update: ProxyPasswordUpdate,
    trust_proxy_dns: bool,
}

impl NetworkSettingsWriteRecord {
    #[must_use]
    pub fn new(
        mode: NetworkSettingsMode,
        proxy_host: Option<String>,
        proxy_port: Option<u16>,
        username: Option<String>,
        password_update: ProxyPasswordUpdate,
        trust_proxy_dns: bool,
    ) -> Self {
        Self {
            mode,
            proxy_host,
            proxy_port,
            username,
            password_update,
            trust_proxy_dns,
        }
    }

    fn validate(&self) -> Result<(), NetworkSettingsRepositoryError> {
        let authentication_shape_valid = matches!(
            (&self.username, &self.password_update),
            (None, ProxyPasswordUpdate::Keep | ProxyPasswordUpdate::Clear)
                | (
                    Some(_),
                    ProxyPasswordUpdate::Keep | ProxyPasswordUpdate::Replace(_)
                )
        );
        let route_shape_valid = if self.mode.is_proxy() {
            self.proxy_host.as_deref().is_some_and(is_valid_proxy_host)
                && self.proxy_port.is_some_and(|port| port > 0)
        } else {
            self.proxy_host.is_none()
                && self.proxy_port.is_none()
                && self.username.is_none()
                && matches!(
                    self.password_update,
                    ProxyPasswordUpdate::Keep | ProxyPasswordUpdate::Clear
                )
                && !self.trust_proxy_dns
        };
        if !route_shape_valid
            || !valid_optional_text(self.username.as_deref(), MAX_PROXY_USERNAME_BYTES)
            || !authentication_shape_valid
        {
            return Err(NetworkSettingsRepositoryError::InvalidSettings);
        }
        Ok(())
    }
}

impl fmt::Debug for NetworkSettingsWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NetworkSettingsWriteRecord")
            .field("mode", &self.mode)
            .field("proxy_host", &self.proxy_host.as_ref().map(|_| "<已脱敏>"))
            .field("proxy_port", &self.proxy_port)
            .field("username", &self.username.as_ref().map(|_| "<已脱敏>"))
            .field("password_update", &self.password_update)
            .field("trust_proxy_dns", &self.trust_proxy_dns)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NetworkSettingsRepositoryConfigError {
    #[error("网络设置数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 网络设置仓储错误不携带代理地址、账号或底层数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NetworkSettingsRepositoryError {
    #[error("网络设置数据库操作失败")]
    Query,
    #[error("网络设置数据库操作超时")]
    Timeout,
    #[error("网络设置持久化状态损坏")]
    Invariant,
    #[error("网络设置字段无效")]
    InvalidSettings,
    #[error("网络设置版本已变化")]
    ConcurrentUpdate,
}

/// 系统全局出站网络设置的固定记录仓储。
#[derive(Clone)]
pub struct NetworkSettingsRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl NetworkSettingsRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, NetworkSettingsRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(NetworkSettingsRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 读取固定网络设置记录；固定行缺失时失败关闭。
    pub async fn settings(&self) -> Result<NetworkSettingsRecord, NetworkSettingsRepositoryError> {
        match timeout(self.operation_timeout, self.settings_inner()).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(NetworkSettingsRepositoryError::Timeout)),
        }
    }

    /// 原子覆盖完整网络设置，并单调递增配置版本。
    pub async fn update(
        &self,
        record: NetworkSettingsWriteRecord,
    ) -> Result<NetworkSettingsRecord, NetworkSettingsRepositoryError> {
        match timeout(self.operation_timeout, self.update_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(NetworkSettingsRepositoryError::Timeout)),
        }
    }

    /// 在运行时应用失败后按版本恢复上一份快照，避免数据库与进程基线分裂。
    pub async fn restore_if_version(
        &self,
        expected_version: i64,
        record: &NetworkSettingsRecord,
    ) -> Result<(), NetworkSettingsRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.restore_if_version_inner(expected_version, record),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal_error(NetworkSettingsRepositoryError::Timeout)),
        }
    }

    async fn settings_inner(
        &self,
    ) -> Result<NetworkSettingsRecord, NetworkSettingsRepositoryError> {
        let Some(model) = network_settings::Entity::find_by_id(NETWORK_SETTINGS_ID)
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("network_settings_read"))?
        else {
            return Err(internal_error(NetworkSettingsRepositoryError::Invariant));
        };
        record_from_model(model)
    }

    async fn update_inner(
        &self,
        record: NetworkSettingsWriteRecord,
    ) -> Result<NetworkSettingsRecord, NetworkSettingsRepositoryError> {
        record.validate()?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("network_settings_begin"))?;
        let existing = lock_settings(&transaction).await?;
        let existing_secret = existing.password_secret.clone();
        let password_secret = if record.username.is_none() {
            None
        } else {
            match record.password_update {
                ProxyPasswordUpdate::Keep => existing_secret,
                ProxyPasswordUpdate::Replace(envelope) => Some(encrypted_json(envelope)?),
                ProxyPasswordUpdate::Clear => None,
            }
        };
        if record.username.is_some() != password_secret.is_some() {
            return Err(NetworkSettingsRepositoryError::InvalidSettings);
        }
        let version = existing
            .version
            .checked_add(1)
            .ok_or_else(|| internal_error(NetworkSettingsRepositoryError::Invariant))?;
        let now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
        let model = network_settings::ActiveModel {
            id: Set(NETWORK_SETTINGS_ID),
            mode: Set(record.mode.as_i16()),
            proxy_host: Set(record.proxy_host.map(SensitiveString::from)),
            proxy_port: Set(record.proxy_port.map(i32::from)),
            username: Set(record.username.map(SensitiveString::from)),
            password_secret: Set(password_secret),
            trust_proxy_dns: Set(record.trust_proxy_dns),
            version: Set(version),
            created_at: Set(existing.created_at),
            updated_at: Set(now),
        };
        let saved = model
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("network_settings_write"))?;
        let saved = record_from_model(saved)?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("network_settings_commit"))?;
        Ok(saved)
    }

    async fn restore_if_version_inner(
        &self,
        expected_version: i64,
        record: &NetworkSettingsRecord,
    ) -> Result<(), NetworkSettingsRepositoryError> {
        if expected_version < 1 || record.version < 1 {
            return Err(internal_error(NetworkSettingsRepositoryError::Invariant));
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("network_settings_restore_begin"))?;
        let existing = lock_settings(&transaction).await?;
        if existing.version != expected_version {
            return Err(internal_error(
                NetworkSettingsRepositoryError::ConcurrentUpdate,
            ));
        }
        let version = expected_version
            .checked_add(1)
            .ok_or_else(|| internal_error(NetworkSettingsRepositoryError::Invariant))?;
        let password_secret = record
            .password_secret
            .clone()
            .map(encrypted_json)
            .transpose()?;
        let now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
        let model = network_settings::ActiveModel {
            id: Set(NETWORK_SETTINGS_ID),
            mode: Set(record.mode.as_i16()),
            proxy_host: Set(record.proxy_host.clone().map(SensitiveString::from)),
            proxy_port: Set(record.proxy_port.map(i32::from)),
            username: Set(record.username.clone().map(SensitiveString::from)),
            password_secret: Set(password_secret),
            trust_proxy_dns: Set(record.trust_proxy_dns),
            version: Set(version),
            created_at: Set(existing.created_at),
            updated_at: Set(now),
        };
        model
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("network_settings_restore_write"))?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("network_settings_restore_commit"))?;
        Ok(())
    }
}

async fn lock_settings(
    transaction: &DatabaseTransaction,
) -> Result<network_settings::Model, NetworkSettingsRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，通过无变化写入先取得数据库写锁。
        let result = network_settings::Entity::update_many()
            .filter(network_settings::Column::Id.eq(NETWORK_SETTINGS_ID))
            .col_expr(
                network_settings::Column::Version,
                Expr::col(network_settings::Column::Version).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("network_settings_lock"))?;
        if result.rows_affected != 1 {
            return Err(internal_error(NetworkSettingsRepositoryError::Invariant));
        }
    }

    let mut query = network_settings::Entity::find()
        .filter(network_settings::Column::Id.eq(NETWORK_SETTINGS_ID));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("network_settings_read_for_update"))?
        .ok_or_else(|| internal_error(NetworkSettingsRepositoryError::Invariant))
}

fn record_from_model(
    model: network_settings::Model,
) -> Result<NetworkSettingsRecord, NetworkSettingsRepositoryError> {
    let mode = NetworkSettingsMode::from_i16(model.mode)?;
    let proxy_host = model.proxy_host.map(|value| value.as_str().to_owned());
    let proxy_port = model
        .proxy_port
        .map(u16::try_from)
        .transpose()
        .map_err(|_| internal_error(NetworkSettingsRepositoryError::Invariant))?;
    let username = model.username.map(|value| value.as_str().to_owned());
    let password_secret = model
        .password_secret
        .map(|secret| {
            let (key_id, nonce, ciphertext) = secret
                .envelope_parts()
                .map_err(|_| internal_error(NetworkSettingsRepositoryError::Invariant))?;
            EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
                .map_err(|_| internal_error(NetworkSettingsRepositoryError::Invariant))
        })
        .transpose()?;
    let route_shape_valid = if mode.is_proxy() {
        proxy_host.as_deref().is_some_and(is_valid_proxy_host)
            && proxy_port.is_some_and(|port| port > 0)
    } else {
        proxy_host.is_none()
            && proxy_port.is_none()
            && username.is_none()
            && password_secret.is_none()
            && !model.trust_proxy_dns
    };
    if model.id != NETWORK_SETTINGS_ID
        || model.version < 1
        || !route_shape_valid
        || !valid_optional_text(username.as_deref(), MAX_PROXY_USERNAME_BYTES)
        || username.is_some() != password_secret.is_some()
    {
        return Err(internal_error(NetworkSettingsRepositoryError::Invariant));
    }
    Ok(NetworkSettingsRecord {
        mode,
        proxy_host,
        proxy_port,
        username,
        password_secret,
        trust_proxy_dns: model.trust_proxy_dns,
        version: model.version,
    })
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, NetworkSettingsRepositoryError> {
    EncryptedJson::from_envelope(json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| internal_error(NetworkSettingsRepositoryError::Invariant))
}

fn is_valid_proxy_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PROXY_HOST_BYTES
        && value.trim() == value
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        && !value.contains("://")
        && !value.contains(['/', '\\', '@'])
}

fn valid_optional_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= maximum_bytes
            && value.trim() == value
            && !value.chars().any(char::is_control)
    })
}

fn query_error(operation: &'static str) -> NetworkSettingsRepositoryError {
    tracing::error!(
        target: "af_db::network_settings",
        error_kind = operation,
        "网络设置数据库操作失败"
    );
    NetworkSettingsRepositoryError::Query
}

fn internal_error(error: NetworkSettingsRepositoryError) -> NetworkSettingsRepositoryError {
    tracing::error!(
        target: "af_db::network_settings",
        error_kind = ?error,
        "网络设置内部状态无效"
    );
    error
}
