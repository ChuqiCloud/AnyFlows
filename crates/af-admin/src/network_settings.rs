use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_account::{PlainSystemSecret, SystemSecretCipher, SystemSecretKind};
use af_db::{
    NetworkSettingsMode, NetworkSettingsRecord, NetworkSettingsRepository,
    NetworkSettingsRepositoryError, NetworkSettingsWriteRecord, ProxyPasswordUpdate,
};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::{SessionPrincipal, SessionRole};

const MAX_PROXY_HOST_BYTES: usize = 255;
const MAX_PROXY_USERNAME_BYTES: usize = 320;

/// 管理员可见的全局出站网络设置投影，密码只返回配置状态。
#[derive(Clone, Eq, PartialEq)]
pub struct AdminNetworkSettings {
    mode: NetworkSettingsMode,
    proxy_host: Option<String>,
    proxy_port: Option<u16>,
    username: Option<String>,
    password_configured: bool,
    trust_proxy_dns: bool,
    version: i64,
}

impl AdminNetworkSettings {
    fn from_record(record: &NetworkSettingsRecord) -> Self {
        Self {
            mode: record.mode(),
            proxy_host: record.proxy_host().map(str::to_owned),
            proxy_port: record.proxy_port(),
            username: record.username().map(str::to_owned),
            password_configured: record.password_configured(),
            trust_proxy_dns: record.trust_proxy_dns(),
            version: record.version(),
        }
    }

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
        self.password_configured
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

impl fmt::Debug for AdminNetworkSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminNetworkSettings")
            .field("mode", &self.mode)
            .field("proxy_host", &self.proxy_host.as_ref().map(|_| "<已脱敏>"))
            .field("proxy_port", &self.proxy_port)
            .field("username", &self.username.as_ref().map(|_| "<已脱敏>"))
            .field("password_configured", &self.password_configured)
            .field("trust_proxy_dns", &self.trust_proxy_dns)
            .field("version", &self.version)
            .finish()
    }
}

/// 管理员完整覆盖网络设置的命令；密码缺失表示保留已有密文。
pub struct AdminNetworkSettingsCommand {
    mode: NetworkSettingsMode,
    proxy_host: Option<String>,
    proxy_port: Option<u16>,
    username: Option<String>,
    password: Option<PlainSystemSecret>,
    trust_proxy_dns: bool,
}

impl AdminNetworkSettingsCommand {
    pub fn new(
        mode: NetworkSettingsMode,
        proxy_host: Option<String>,
        proxy_port: Option<u16>,
        username: Option<String>,
        password: Option<String>,
        trust_proxy_dns: bool,
    ) -> Result<Self, AdminNetworkSettingsError> {
        let proxy_host = normalize_optional(proxy_host);
        let username = normalize_optional(username);
        let password = password
            .map(PlainSystemSecret::new)
            .transpose()
            .map_err(|_| AdminNetworkSettingsError::InvalidInput)?;
        let route_valid = if mode.is_proxy() {
            proxy_host.as_deref().is_some_and(is_valid_proxy_host)
                && proxy_port.is_some_and(|port| port > 0)
        } else {
            proxy_host.is_none() && proxy_port.is_none() && username.is_none() && !trust_proxy_dns
        };
        if !route_valid
            || !valid_optional_text(username.as_deref(), MAX_PROXY_USERNAME_BYTES)
            || (username.is_none() && password.is_some())
        {
            return Err(AdminNetworkSettingsError::InvalidInput);
        }
        Ok(Self {
            mode,
            proxy_host,
            proxy_port,
            username,
            password,
            trust_proxy_dns,
        })
    }

    fn into_record(
        self,
        cipher: &SystemSecretCipher,
    ) -> Result<NetworkSettingsWriteRecord, AdminNetworkSettingsError> {
        let password_update = if self.username.is_none() {
            ProxyPasswordUpdate::Clear
        } else if let Some(password) = self.password.as_ref() {
            ProxyPasswordUpdate::Replace(
                cipher
                    .encrypt(SystemSecretKind::ProxyPassword, password)
                    .map_err(|_| AdminNetworkSettingsError::Internal)?,
            )
        } else {
            ProxyPasswordUpdate::Keep
        };
        Ok(NetworkSettingsWriteRecord::new(
            self.mode,
            self.proxy_host,
            self.proxy_port,
            self.username,
            password_update,
            self.trust_proxy_dns,
        ))
    }
}

impl fmt::Debug for AdminNetworkSettingsCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminNetworkSettingsCommand(<已脱敏>)")
    }
}

/// 运行时应用端口；实现负责把数据库快照转换为受控 HTTP Client 基线。
pub trait NetworkSettingsRuntimeApplier: Send + Sync {
    fn apply<'a>(
        &'a self,
        record: &'a NetworkSettingsRecord,
        cipher: &'a SystemSecretCipher,
    ) -> NetworkSettingsApplyFuture<'a>;
}

pub type NetworkSettingsApplyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), NetworkSettingsRuntimeError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NetworkSettingsRuntimeError {
    #[error("运行时网络设置应用失败")]
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminNetworkSettingsError {
    #[error("网络设置输入无效")]
    InvalidInput,
    #[error("当前会话无权管理网络设置")]
    Forbidden,
    #[error("网络设置服务内部错误")]
    Internal,
}

pub type AdminNetworkSettingsReadFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminNetworkSettings, AdminNetworkSettingsError>> + Send + 'a>,
>;
pub type AdminNetworkSettingsUpdateFuture<'a> = AdminNetworkSettingsReadFuture<'a>;

/// 管理员网络设置用例端口。
pub trait AdminNetworkSettingsService: Send + Sync {
    fn settings<'a>(&'a self, principal: SessionPrincipal) -> AdminNetworkSettingsReadFuture<'a>;
    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminNetworkSettingsCommand,
    ) -> AdminNetworkSettingsUpdateFuture<'a>;
}

/// 使用数据库、系统密钥和运行时应用器实现网络设置管理用例。
pub struct DatabaseAdminNetworkSettingsService {
    repository: NetworkSettingsRepository,
    cipher: SystemSecretCipher,
    runtime: Arc<dyn NetworkSettingsRuntimeApplier>,
    update_lock: Arc<Mutex<()>>,
}

impl DatabaseAdminNetworkSettingsService {
    #[must_use]
    pub fn new(
        repository: NetworkSettingsRepository,
        cipher: SystemSecretCipher,
        runtime: Arc<dyn NetworkSettingsRuntimeApplier>,
    ) -> Self {
        Self {
            repository,
            cipher,
            runtime,
            update_lock: Arc::new(Mutex::new(())),
        }
    }

    /// 服务启动完成前应用数据库中的最新快照。
    pub async fn apply_current(&self) -> Result<(), AdminNetworkSettingsError> {
        let record = self
            .repository
            .settings()
            .await
            .map_err(map_repository_error)?;
        self.runtime
            .apply(&record, &self.cipher)
            .await
            .map_err(|_| AdminNetworkSettingsError::Internal)
    }
}

impl AdminNetworkSettingsService for DatabaseAdminNetworkSettingsService {
    fn settings<'a>(&'a self, principal: SessionPrincipal) -> AdminNetworkSettingsReadFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .settings()
                .await
                .map(|record| AdminNetworkSettings::from_record(&record))
                .map_err(map_repository_error)
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminNetworkSettingsCommand,
    ) -> AdminNetworkSettingsUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            // 串行化数据库提交、运行时切换和失败回滚，避免同一实例内交叉覆盖。
            let _update_guard = self.update_lock.lock().await;
            let previous = self
                .repository
                .settings()
                .await
                .map_err(map_repository_error)?;
            let record = command.into_record(&self.cipher)?;
            let saved = self
                .repository
                .update(record)
                .await
                .map_err(map_repository_error)?;
            if self.runtime.apply(&saved, &self.cipher).await.is_err() {
                // 运行时拒绝新基线时恢复持久化快照，并尽力把旧基线重新装回 Client。
                let restore_result = self
                    .repository
                    .restore_if_version(saved.version(), &previous)
                    .await;
                let reapply_result = self.runtime.apply(&previous, &self.cipher).await;
                if restore_result.is_err() || reapply_result.is_err() {
                    tracing::error!(
                        restore_failed = restore_result.is_err(),
                        runtime_reapply_failed = reapply_result.is_err(),
                        "网络设置运行时切换失败且回滚未完全确认"
                    );
                }
                return Err(AdminNetworkSettingsError::Internal);
            }
            Ok(AdminNetworkSettings::from_record(&saved))
        })
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminNetworkSettingsError> {
    if principal.role() != SessionRole::Admin {
        return Err(AdminNetworkSettingsError::Forbidden);
    }
    Ok(())
}

fn map_repository_error(error: NetworkSettingsRepositoryError) -> AdminNetworkSettingsError {
    match error {
        NetworkSettingsRepositoryError::InvalidSettings => AdminNetworkSettingsError::InvalidInput,
        NetworkSettingsRepositoryError::Query
        | NetworkSettingsRepositoryError::Timeout
        | NetworkSettingsRepositoryError::Invariant
        | NetworkSettingsRepositoryError::ConcurrentUpdate => AdminNetworkSettingsError::Internal,
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
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
