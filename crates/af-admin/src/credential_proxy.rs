use std::{fmt, future::Future, pin::Pin};

use af_account::{PlainSystemSecret, SystemSecretCipher, SystemSecretKind};
use af_db::{
    CredentialProxyCreateRecord, CredentialProxyDeleteOutcome, CredentialProxyLookupOutcome,
    CredentialProxyMutationOutcome, CredentialProxyPageRecord, CredentialProxyPasswordUpdate,
    CredentialProxyRecord, CredentialProxyRepository, CredentialProxyRepositoryError,
    CredentialProxyScheme, CredentialProxyUpdateRecord, MAX_CREDENTIAL_PROXY_HOST_BYTES,
    MAX_CREDENTIAL_PROXY_NAME_BYTES, MAX_CREDENTIAL_PROXY_PAGE_SIZE,
    MAX_CREDENTIAL_PROXY_USERNAME_BYTES,
};
use af_domain::ProxyId;
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理员可见的专属代理投影，不包含密码密文或明文。
#[derive(Clone, Eq, PartialEq)]
pub struct AdminCredentialProxy {
    proxy_id: ProxyId,
    name: String,
    scheme: CredentialProxyScheme,
    host: String,
    port: u16,
    username: Option<String>,
    password_configured: bool,
    trust_proxy_dns: bool,
    enabled: bool,
    version: i64,
    created_at: i64,
    updated_at: i64,
}

impl AdminCredentialProxy {
    fn from_record(record: CredentialProxyRecord) -> Self {
        Self {
            proxy_id: record.proxy_id(),
            name: record.name().to_owned(),
            scheme: record.scheme(),
            host: record.host().to_owned(),
            port: record.port(),
            username: record.username().map(str::to_owned),
            password_configured: record.password_configured(),
            trust_proxy_dns: record.trust_proxy_dns(),
            enabled: record.enabled(),
            version: record.version(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
        }
    }

    #[must_use]
    pub const fn proxy_id(&self) -> ProxyId {
        self.proxy_id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn scheme(&self) -> CredentialProxyScheme {
        self.scheme
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
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

impl fmt::Debug for AdminCredentialProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialProxy(<已脱敏>)")
    }
}

/// 管理端专属代理目录分页结果。
pub struct AdminCredentialProxyPage {
    proxies: Vec<AdminCredentialProxy>,
    next_cursor: Option<ProxyId>,
}

impl AdminCredentialProxyPage {
    fn from_record(record: CredentialProxyPageRecord) -> Self {
        let (proxies, next_cursor) = record.into_parts();
        Self {
            proxies: proxies
                .into_iter()
                .map(AdminCredentialProxy::from_record)
                .collect(),
            next_cursor,
        }
    }
    #[must_use]
    pub fn proxies(&self) -> &[AdminCredentialProxy] {
        &self.proxies
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<ProxyId> {
        self.next_cursor
    }
}

/// 专属代理目录分页查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminCredentialProxyListQuery {
    after: Option<ProxyId>,
    limit: usize,
}

impl AdminCredentialProxyListQuery {
    pub fn new(after: Option<ProxyId>, limit: usize) -> Result<Self, AdminCredentialProxyError> {
        if !(1..=MAX_CREDENTIAL_PROXY_PAGE_SIZE).contains(&limit) {
            return Err(AdminCredentialProxyError::InvalidInput);
        }
        Ok(Self { after, limit })
    }
    #[must_use]
    pub const fn after(self) -> Option<ProxyId> {
        self.after
    }
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for AdminCredentialProxyListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: MAX_CREDENTIAL_PROXY_PAGE_SIZE,
        }
    }
}

/// 创建专属代理的结构化命令。
pub struct AdminCredentialProxyCreateCommand {
    name: String,
    scheme: CredentialProxyScheme,
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<PlainSystemSecret>,
    trust_proxy_dns: bool,
    enabled: bool,
}

impl AdminCredentialProxyCreateCommand {
    #[allow(clippy::too_many_arguments, reason = "字段与专属代理创建契约一一对应")]
    pub fn new(
        name: String,
        scheme: CredentialProxyScheme,
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
        trust_proxy_dns: bool,
        enabled: bool,
    ) -> Result<Self, AdminCredentialProxyError> {
        let username = normalize_optional(username);
        let password = password
            .map(PlainSystemSecret::new)
            .transpose()
            .map_err(|_| AdminCredentialProxyError::InvalidInput)?;
        validate_fields(&name, &host, port, username.as_deref())?;
        if username.is_some() != password.is_some() {
            return Err(AdminCredentialProxyError::InvalidInput);
        }
        Ok(Self {
            name,
            scheme,
            host,
            port,
            username,
            password,
            trust_proxy_dns,
            enabled,
        })
    }
}

impl fmt::Debug for AdminCredentialProxyCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialProxyCreateCommand(<已脱敏>)")
    }
}

/// 完整更新专属代理的命令；密码为空表示保留，取消用户名表示清除认证。
pub struct AdminCredentialProxyUpdateCommand {
    name: String,
    scheme: CredentialProxyScheme,
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<PlainSystemSecret>,
    trust_proxy_dns: bool,
    enabled: bool,
}

impl AdminCredentialProxyUpdateCommand {
    #[allow(clippy::too_many_arguments, reason = "字段与专属代理更新契约一一对应")]
    pub fn new(
        name: String,
        scheme: CredentialProxyScheme,
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
        trust_proxy_dns: bool,
        enabled: bool,
    ) -> Result<Self, AdminCredentialProxyError> {
        let username = normalize_optional(username);
        let password = password
            .map(PlainSystemSecret::new)
            .transpose()
            .map_err(|_| AdminCredentialProxyError::InvalidInput)?;
        validate_fields(&name, &host, port, username.as_deref())?;
        if username.is_none() && password.is_some() {
            return Err(AdminCredentialProxyError::InvalidInput);
        }
        Ok(Self {
            name,
            scheme,
            host,
            port,
            username,
            password,
            trust_proxy_dns,
            enabled,
        })
    }
}

impl fmt::Debug for AdminCredentialProxyUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialProxyUpdateCommand(<已脱敏>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminCredentialProxyError {
    #[error("专属代理输入无效")]
    InvalidInput,
    #[error("当前会话无权管理专属代理")]
    Forbidden,
    #[error("专属代理不存在")]
    NotFound,
    #[error("专属代理名称冲突")]
    Conflict,
    #[error("专属代理仍被凭据引用")]
    Referenced,
    #[error("专属代理服务内部错误")]
    Internal,
}

pub type AdminCredentialProxyPageFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminCredentialProxyPage, AdminCredentialProxyError>>
            + Send
            + 'a,
    >,
>;
pub type AdminCredentialProxyFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminCredentialProxy, AdminCredentialProxyError>> + Send + 'a>,
>;
pub type AdminCredentialProxyDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminCredentialProxyError>> + Send + 'a>>;

/// 管理员专属代理目录用例端口。
pub trait AdminCredentialProxyService: Send + Sync {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminCredentialProxyListQuery,
    ) -> AdminCredentialProxyPageFuture<'a>;
    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        proxy_id: ProxyId,
    ) -> AdminCredentialProxyFuture<'a>;
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminCredentialProxyCreateCommand,
    ) -> AdminCredentialProxyFuture<'a>;
    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        proxy_id: ProxyId,
        command: AdminCredentialProxyUpdateCommand,
    ) -> AdminCredentialProxyFuture<'a>;
    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        proxy_id: ProxyId,
    ) -> AdminCredentialProxyDeleteFuture<'a>;
}

/// 使用数据库目录和独立代理密码 AAD 实现管理用例。
pub struct DatabaseAdminCredentialProxyService {
    repository: CredentialProxyRepository,
    cipher: SystemSecretCipher,
}

impl DatabaseAdminCredentialProxyService {
    #[must_use]
    pub fn new(repository: CredentialProxyRepository, cipher: SystemSecretCipher) -> Self {
        Self { repository, cipher }
    }
}

impl AdminCredentialProxyService for DatabaseAdminCredentialProxyService {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminCredentialProxyListQuery,
    ) -> AdminCredentialProxyPageFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .list(query.after(), query.limit())
                .await
                .map(AdminCredentialProxyPage::from_record)
                .map_err(map_repository_error)
        })
    }

    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        proxy_id: ProxyId,
    ) -> AdminCredentialProxyFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .get(proxy_id)
                .await
                .map_err(map_repository_error)?
            {
                CredentialProxyLookupOutcome::Found(record) => {
                    Ok(AdminCredentialProxy::from_record(record))
                }
                CredentialProxyLookupOutcome::NotFound => Err(AdminCredentialProxyError::NotFound),
            }
        })
    }

    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminCredentialProxyCreateCommand,
    ) -> AdminCredentialProxyFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let AdminCredentialProxyCreateCommand {
                name,
                scheme,
                host,
                port,
                username,
                password,
                trust_proxy_dns,
                enabled,
            } = command;
            let record = CredentialProxyCreateRecord::new(
                name,
                scheme,
                host,
                port,
                username,
                trust_proxy_dns,
                enabled,
            );
            self.repository
                .create(record, |proxy_id| {
                    password
                        .as_ref()
                        .map(|password| {
                            self.cipher.encrypt(
                                SystemSecretKind::CredentialProxyPassword(proxy_id),
                                password,
                            )
                        })
                        .transpose()
                        .map_err(|_| ())
                })
                .await
                .map(AdminCredentialProxy::from_record)
                .map_err(map_repository_error)
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        proxy_id: ProxyId,
        command: AdminCredentialProxyUpdateCommand,
    ) -> AdminCredentialProxyFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let AdminCredentialProxyUpdateCommand {
                name,
                scheme,
                host,
                port,
                username,
                password,
                trust_proxy_dns,
                enabled,
            } = command;
            let password_update = if username.is_none() {
                CredentialProxyPasswordUpdate::Clear
            } else if let Some(password) = password.as_ref() {
                CredentialProxyPasswordUpdate::Replace(
                    self.cipher
                        .encrypt(
                            SystemSecretKind::CredentialProxyPassword(proxy_id),
                            password,
                        )
                        .map_err(|_| AdminCredentialProxyError::Internal)?,
                )
            } else {
                CredentialProxyPasswordUpdate::Keep
            };
            let record = CredentialProxyUpdateRecord::new(
                name,
                scheme,
                host,
                port,
                username,
                password_update,
                trust_proxy_dns,
                enabled,
            );
            match self
                .repository
                .update(proxy_id, record)
                .await
                .map_err(map_repository_error)?
            {
                CredentialProxyMutationOutcome::Mutated(record) => {
                    Ok(AdminCredentialProxy::from_record(record))
                }
                CredentialProxyMutationOutcome::NotFound => {
                    Err(AdminCredentialProxyError::NotFound)
                }
            }
        })
    }

    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        proxy_id: ProxyId,
    ) -> AdminCredentialProxyDeleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .delete(proxy_id)
                .await
                .map_err(map_repository_error)?
            {
                CredentialProxyDeleteOutcome::Deleted => Ok(()),
                CredentialProxyDeleteOutcome::NotFound => Err(AdminCredentialProxyError::NotFound),
                CredentialProxyDeleteOutcome::Referenced => {
                    Err(AdminCredentialProxyError::Referenced)
                }
            }
        })
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminCredentialProxyError> {
    (principal.role() == SessionRole::Admin)
        .then_some(())
        .ok_or(AdminCredentialProxyError::Forbidden)
}

fn map_repository_error(error: CredentialProxyRepositoryError) -> AdminCredentialProxyError {
    match error {
        CredentialProxyRepositoryError::InvalidInput => AdminCredentialProxyError::InvalidInput,
        CredentialProxyRepositoryError::Conflict => AdminCredentialProxyError::Conflict,
        CredentialProxyRepositoryError::Referenced => AdminCredentialProxyError::Referenced,
        CredentialProxyRepositoryError::Query
        | CredentialProxyRepositoryError::Timeout
        | CredentialProxyRepositoryError::Invariant
        | CredentialProxyRepositoryError::SecretPreparation => AdminCredentialProxyError::Internal,
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn validate_fields(
    name: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
) -> Result<(), AdminCredentialProxyError> {
    if !valid_text(name, MAX_CREDENTIAL_PROXY_NAME_BYTES)
        || port == 0
        || host.is_empty()
        || host.len() > MAX_CREDENTIAL_PROXY_HOST_BYTES
        || host.trim() != host
        || !host.is_ascii()
        || host
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        || host.contains("://")
        || host.contains(['/', '\\', '@'])
        || username.is_some_and(|value| !valid_text(value, MAX_CREDENTIAL_PROXY_USERNAME_BYTES))
    {
        return Err(AdminCredentialProxyError::InvalidInput);
    }
    Ok(())
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
