use std::{fmt, time::Duration};

use af_domain::{ChannelId, ProxyId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use serde_json::json;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, EncryptedCredentialEnvelope, SchedulerCatalogSubject,
    entity::{EncryptedJson, SensitiveString, credentials, proxies},
    scheduler_outbox::enqueue_scheduler_catalog_change,
};

/// 管理端代理目录单页允许返回的最大记录数。
pub const MAX_CREDENTIAL_PROXY_PAGE_SIZE: usize = 100;
/// 代理展示名称允许的最大 UTF-8 字节数。
pub const MAX_CREDENTIAL_PROXY_NAME_BYTES: usize = 128;
/// 代理主机名或 IP 文本允许的最大字节数。
pub const MAX_CREDENTIAL_PROXY_HOST_BYTES: usize = 255;
/// 代理认证用户名允许的最大 UTF-8 字节数。
pub const MAX_CREDENTIAL_PROXY_USERNAME_BYTES: usize = 320;

/// 凭据专属出口代理使用的闭合协议集合。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialProxyScheme {
    Http,
    Https,
    Socks5,
    Socks5h,
}

impl CredentialProxyScheme {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks5 => "socks5",
            Self::Socks5h => "socks5h",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, CredentialProxyRepositoryError> {
        match value {
            "http" => Ok(Self::Http),
            "https" => Ok(Self::Https),
            "socks5" => Ok(Self::Socks5),
            "socks5h" => Ok(Self::Socks5h),
            _ => Err(internal_error(CredentialProxyRepositoryError::Invariant)),
        }
    }
}

/// 代理密码在完整更新中的处理语义。
pub enum CredentialProxyPasswordUpdate {
    Keep,
    Replace(EncryptedCredentialEnvelope),
    Clear,
}

impl fmt::Debug for CredentialProxyPasswordUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Keep => "CredentialProxyPasswordUpdate::Keep",
            Self::Replace(_) => "CredentialProxyPasswordUpdate::Replace(<已脱敏>)",
            Self::Clear => "CredentialProxyPasswordUpdate::Clear",
        })
    }
}

/// 已通过持久化不变量校验的专属代理快照。
#[derive(Clone, Eq, PartialEq)]
pub struct CredentialProxyRecord {
    proxy_id: ProxyId,
    name: String,
    scheme: CredentialProxyScheme,
    host: String,
    port: u16,
    username: Option<String>,
    password_secret: Option<EncryptedCredentialEnvelope>,
    trust_proxy_dns: bool,
    enabled: bool,
    version: i64,
    created_at: i64,
    updated_at: i64,
}

impl CredentialProxyRecord {
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
        self.password_secret.is_some()
    }
    /// 返回认证密码密文；仅运行时装配器可以解密，管理响应不得序列化。
    #[must_use]
    pub fn password_secret(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.password_secret.as_ref()
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

impl fmt::Debug for CredentialProxyRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialProxyRecord")
            .field("proxy_id", &self.proxy_id)
            .field("name", &self.name)
            .field("scheme", &self.scheme)
            .field("host", &"<已脱敏>")
            .field("port", &self.port)
            .field("username", &self.username.as_ref().map(|_| "<已脱敏>"))
            .field("password_configured", &self.password_configured())
            .field("trust_proxy_dns", &self.trust_proxy_dns)
            .field("enabled", &self.enabled)
            .field("version", &self.version)
            .finish()
    }
}

/// 一页专属代理目录快照。
pub struct CredentialProxyPageRecord {
    proxies: Vec<CredentialProxyRecord>,
    next_cursor: Option<ProxyId>,
}

impl CredentialProxyPageRecord {
    #[must_use]
    pub fn into_parts(self) -> (Vec<CredentialProxyRecord>, Option<ProxyId>) {
        (self.proxies, self.next_cursor)
    }
}

/// 单条代理读取结果。
pub enum CredentialProxyLookupOutcome {
    Found(CredentialProxyRecord),
    NotFound,
}

/// 单条代理更新结果。
#[derive(Debug)]
pub enum CredentialProxyMutationOutcome {
    Mutated(CredentialProxyRecord),
    NotFound,
}

/// 代理软删除结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialProxyDeleteOutcome {
    Deleted,
    NotFound,
    Referenced,
}

/// 创建代理时使用的非密文目录字段。
pub struct CredentialProxyCreateRecord {
    name: String,
    scheme: CredentialProxyScheme,
    host: String,
    port: u16,
    username: Option<String>,
    trust_proxy_dns: bool,
    enabled: bool,
}

impl CredentialProxyCreateRecord {
    #[must_use]
    pub fn new(
        name: String,
        scheme: CredentialProxyScheme,
        host: String,
        port: u16,
        username: Option<String>,
        trust_proxy_dns: bool,
        enabled: bool,
    ) -> Self {
        Self {
            name,
            scheme,
            host,
            port,
            username,
            trust_proxy_dns,
            enabled,
        }
    }
}

/// 完整更新代理目录时使用的字段。
pub struct CredentialProxyUpdateRecord {
    name: String,
    scheme: CredentialProxyScheme,
    host: String,
    port: u16,
    username: Option<String>,
    password_update: CredentialProxyPasswordUpdate,
    trust_proxy_dns: bool,
    enabled: bool,
}

impl CredentialProxyUpdateRecord {
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "字段与专属代理管理契约一一对应")]
    pub fn new(
        name: String,
        scheme: CredentialProxyScheme,
        host: String,
        port: u16,
        username: Option<String>,
        password_update: CredentialProxyPasswordUpdate,
        trust_proxy_dns: bool,
        enabled: bool,
    ) -> Self {
        Self {
            name,
            scheme,
            host,
            port,
            username,
            password_update,
            trust_proxy_dns,
            enabled,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CredentialProxyRepositoryConfigError {
    #[error("专属代理数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 专属代理仓储错误不携带地址、账号、密码或底层数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CredentialProxyRepositoryError {
    #[error("专属代理字段无效")]
    InvalidInput,
    #[error("专属代理名称冲突")]
    Conflict,
    #[error("专属代理仍被凭据引用")]
    Referenced,
    #[error("专属代理数据库操作失败")]
    Query,
    #[error("专属代理数据库操作超时")]
    Timeout,
    #[error("专属代理持久化状态损坏")]
    Invariant,
    #[error("专属代理密码准备失败")]
    SecretPreparation,
}

/// 凭据专属出口代理目录仓储。
#[derive(Clone)]
pub struct CredentialProxyRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl CredentialProxyRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, CredentialProxyRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(CredentialProxyRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 按稳定代理 ID 游标读取未软删除目录。
    pub async fn list(
        &self,
        after: Option<ProxyId>,
        limit: usize,
    ) -> Result<CredentialProxyPageRecord, CredentialProxyRepositoryError> {
        if !(1..=MAX_CREDENTIAL_PROXY_PAGE_SIZE).contains(&limit) {
            return Err(internal_error(CredentialProxyRepositoryError::Invariant));
        }
        let operation = async {
            let mut query = proxies::Entity::find()
                .filter(proxies::Column::DeletedAt.is_null())
                .order_by_asc(proxies::Column::Id)
                .limit((limit + 1) as u64);
            if let Some(after) = after {
                query = query.filter(proxies::Column::Id.gt(after.get()));
            }
            query
                .all(self.pool.connection())
                .await
                .map_err(|_| query_error("credential_proxy_list"))
        }
        .with_subscriber(NoSubscriber::default());
        let mut models = timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| internal_error(CredentialProxyRepositoryError::Timeout))??;
        let has_more = models.len() > limit;
        if has_more {
            models.pop();
        }
        let proxies = models
            .into_iter()
            .map(record_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| proxies.last().map(CredentialProxyRecord::proxy_id))
            .flatten();
        Ok(CredentialProxyPageRecord {
            proxies,
            next_cursor,
        })
    }

    /// 读取一个未软删除代理，供管理端与运行时投影复用。
    pub async fn get(
        &self,
        proxy_id: ProxyId,
    ) -> Result<CredentialProxyLookupOutcome, CredentialProxyRepositoryError> {
        let operation = proxies::Entity::find_by_id(proxy_id.get())
            .filter(proxies::Column::DeletedAt.is_null())
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default());
        let model = timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| internal_error(CredentialProxyRepositoryError::Timeout))?
            .map_err(|_| query_error("credential_proxy_get"))?;
        model.map(record_from_model).transpose().map(|record| {
            record.map_or(
                CredentialProxyLookupOutcome::NotFound,
                CredentialProxyLookupOutcome::Found,
            )
        })
    }

    /// 创建代理；密码回调只接收事务内生成的真实代理 ID。
    pub async fn create<F>(
        &self,
        record: CredentialProxyCreateRecord,
        prepare_password: F,
    ) -> Result<CredentialProxyRecord, CredentialProxyRepositoryError>
    where
        F: FnOnce(ProxyId) -> Result<Option<EncryptedCredentialEnvelope>, ()> + Send,
    {
        let operation = self.create_inner(record, prepare_password);
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| internal_error(CredentialProxyRepositoryError::Timeout))?
    }

    /// 完整更新代理，并为全部引用渠道追加运行时目录事件。
    pub async fn update(
        &self,
        proxy_id: ProxyId,
        record: CredentialProxyUpdateRecord,
    ) -> Result<CredentialProxyMutationOutcome, CredentialProxyRepositoryError> {
        timeout(self.operation_timeout, self.update_inner(proxy_id, record))
            .await
            .map_err(|_| internal_error(CredentialProxyRepositoryError::Timeout))?
    }

    /// 未被凭据引用时软删除代理；引用冲突必须显式返回。
    pub async fn delete(
        &self,
        proxy_id: ProxyId,
    ) -> Result<CredentialProxyDeleteOutcome, CredentialProxyRepositoryError> {
        timeout(self.operation_timeout, self.delete_inner(proxy_id))
            .await
            .map_err(|_| internal_error(CredentialProxyRepositoryError::Timeout))?
    }

    async fn create_inner<F>(
        &self,
        record: CredentialProxyCreateRecord,
        prepare_password: F,
    ) -> Result<CredentialProxyRecord, CredentialProxyRepositoryError>
    where
        F: FnOnce(ProxyId) -> Result<Option<EncryptedCredentialEnvelope>, ()>,
    {
        validate_proxy_fields(
            &record.name,
            &record.host,
            record.port,
            record.username.as_deref(),
        )?;
        let transaction = begin(&self.pool).await?;
        ensure_name_available(&transaction, &record.name, None).await?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let placeholder = record
            .username
            .as_ref()
            .map(|_| pending_encrypted_json())
            .transpose()?;
        let model = proxies::ActiveModel {
            active_name: Set(Some(record.name.clone())),
            name: Set(record.name),
            scheme: Set(record.scheme.as_str().to_owned()),
            host: Set(SensitiveString::from(record.host)),
            port: Set(i32::from(record.port)),
            username: Set(record.username.map(SensitiveString::from)),
            password_secret: Set(placeholder),
            trust_proxy_dns: Set(record.trust_proxy_dns),
            enabled: Set(record.enabled),
            version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| proxy_write_error(&error, "credential_proxy_create"))?;
        let proxy_id = ProxyId::new(model.id)
            .map_err(|_| internal_error(CredentialProxyRepositoryError::Invariant))?;
        let password = prepare_password(proxy_id)
            .map_err(|()| internal_error(CredentialProxyRepositoryError::SecretPreparation))?;
        if record_authentication_shape(model.username.is_some(), password.is_some()).is_err() {
            return Err(CredentialProxyRepositoryError::InvalidInput);
        }
        let password_secret = password.map(encrypted_json).transpose()?;
        if model.username.is_some() {
            let result = proxies::Entity::update_many()
                .filter(proxies::Column::Id.eq(proxy_id.get()))
                .col_expr(
                    proxies::Column::PasswordSecret,
                    sea_orm::sea_query::Expr::value(password_secret),
                )
                .exec(&transaction)
                .await
                .map_err(|_| query_error("credential_proxy_password_write"))?;
            if result.rows_affected != 1 {
                return Err(internal_error(CredentialProxyRepositoryError::Invariant));
            }
        }
        let saved = find_active(&transaction, proxy_id)
            .await?
            .ok_or_else(|| internal_error(CredentialProxyRepositoryError::Invariant))?;
        transaction
            .commit()
            .await
            .map_err(|_| query_error("credential_proxy_create_commit"))?;
        record_from_model(saved)
    }

    async fn update_inner(
        &self,
        proxy_id: ProxyId,
        record: CredentialProxyUpdateRecord,
    ) -> Result<CredentialProxyMutationOutcome, CredentialProxyRepositoryError> {
        validate_proxy_fields(
            &record.name,
            &record.host,
            record.port,
            record.username.as_deref(),
        )?;
        let transaction = begin(&self.pool).await?;
        let Some(current) = find_active(&transaction, proxy_id).await? else {
            return Ok(CredentialProxyMutationOutcome::NotFound);
        };
        ensure_name_available(&transaction, &record.name, Some(proxy_id)).await?;
        let channels = referenced_channels(&transaction, proxy_id).await?;
        if !record.enabled && !channels.is_empty() {
            return Err(CredentialProxyRepositoryError::Referenced);
        }
        let password_secret = match (record.username.as_ref(), record.password_update) {
            (None, CredentialProxyPasswordUpdate::Keep | CredentialProxyPasswordUpdate::Clear) => {
                None
            }
            (Some(_), CredentialProxyPasswordUpdate::Keep) => current.password_secret,
            (Some(_), CredentialProxyPasswordUpdate::Replace(envelope)) => {
                Some(encrypted_json(envelope)?)
            }
            _ => return Err(CredentialProxyRepositoryError::InvalidInput),
        };
        if record.username.is_some() != password_secret.is_some() {
            return Err(CredentialProxyRepositoryError::InvalidInput);
        }
        let version = current
            .version
            .checked_add(1)
            .ok_or_else(|| internal_error(CredentialProxyRepositoryError::Invariant))?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let active_name = record.name.clone();
        let result = proxies::Entity::update_many()
            .filter(proxies::Column::Id.eq(proxy_id.get()))
            .filter(proxies::Column::DeletedAt.is_null())
            .col_expr(
                proxies::Column::Name,
                sea_orm::sea_query::Expr::value(record.name),
            )
            .col_expr(
                proxies::Column::ActiveName,
                sea_orm::sea_query::Expr::value(Some(active_name)),
            )
            .col_expr(
                proxies::Column::Scheme,
                sea_orm::sea_query::Expr::value(record.scheme.as_str()),
            )
            .col_expr(
                proxies::Column::Host,
                sea_orm::sea_query::Expr::value(SensitiveString::from(record.host)),
            )
            .col_expr(
                proxies::Column::Port,
                sea_orm::sea_query::Expr::value(i32::from(record.port)),
            )
            .col_expr(
                proxies::Column::Username,
                sea_orm::sea_query::Expr::value(record.username.map(SensitiveString::from)),
            )
            .col_expr(
                proxies::Column::PasswordSecret,
                sea_orm::sea_query::Expr::value(password_secret),
            )
            .col_expr(
                proxies::Column::TrustProxyDns,
                sea_orm::sea_query::Expr::value(record.trust_proxy_dns),
            )
            .col_expr(
                proxies::Column::Enabled,
                sea_orm::sea_query::Expr::value(record.enabled),
            )
            .col_expr(
                proxies::Column::Version,
                sea_orm::sea_query::Expr::value(version),
            )
            .col_expr(
                proxies::Column::UpdatedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(|error| proxy_write_error(&error, "credential_proxy_update"))?;
        if result.rows_affected != 1 {
            return Err(internal_error(CredentialProxyRepositoryError::Invariant));
        }
        for channel_id in channels {
            enqueue_scheduler_catalog_change(
                &transaction,
                SchedulerCatalogSubject::Channel(channel_id),
                now,
            )
            .await
            .map_err(|_| query_error("credential_proxy_outbox"))?;
        }
        let saved = find_active(&transaction, proxy_id)
            .await?
            .ok_or_else(|| internal_error(CredentialProxyRepositoryError::Invariant))?;
        transaction
            .commit()
            .await
            .map_err(|_| query_error("credential_proxy_update_commit"))?;
        record_from_model(saved).map(CredentialProxyMutationOutcome::Mutated)
    }

    async fn delete_inner(
        &self,
        proxy_id: ProxyId,
    ) -> Result<CredentialProxyDeleteOutcome, CredentialProxyRepositoryError> {
        let transaction = begin(&self.pool).await?;
        if find_active(&transaction, proxy_id).await?.is_none() {
            return Ok(CredentialProxyDeleteOutcome::NotFound);
        }
        if !referenced_channels(&transaction, proxy_id)
            .await?
            .is_empty()
        {
            return Ok(CredentialProxyDeleteOutcome::Referenced);
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = proxies::Entity::update_many()
            .filter(proxies::Column::Id.eq(proxy_id.get()))
            .filter(proxies::Column::DeletedAt.is_null())
            .col_expr(
                proxies::Column::ActiveName,
                sea_orm::sea_query::Expr::value(Option::<String>::None),
            )
            .col_expr(
                proxies::Column::DeletedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .col_expr(
                proxies::Column::UpdatedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(|_| query_error("credential_proxy_delete"))?;
        if result.rows_affected != 1 {
            return Err(internal_error(CredentialProxyRepositoryError::Invariant));
        }
        transaction
            .commit()
            .await
            .map_err(|_| query_error("credential_proxy_delete_commit"))?;
        Ok(CredentialProxyDeleteOutcome::Deleted)
    }
}

async fn begin(pool: &DatabasePool) -> Result<DatabaseTransaction, CredentialProxyRepositoryError> {
    pool.connection()
        .begin()
        .await
        .map_err(|_| query_error("credential_proxy_begin"))
}

async fn find_active(
    transaction: &DatabaseTransaction,
    proxy_id: ProxyId,
) -> Result<Option<proxies::Model>, CredentialProxyRepositoryError> {
    proxies::Entity::find_by_id(proxy_id.get())
        .filter(proxies::Column::DeletedAt.is_null())
        .one(transaction)
        .await
        .map_err(|_| query_error("credential_proxy_find"))
}

async fn ensure_name_available(
    transaction: &DatabaseTransaction,
    name: &str,
    excluded: Option<ProxyId>,
) -> Result<(), CredentialProxyRepositoryError> {
    let mut query = proxies::Entity::find()
        .select_only()
        .column(proxies::Column::Id)
        .filter(proxies::Column::Name.eq(name))
        .filter(proxies::Column::DeletedAt.is_null())
        .limit(1);
    if let Some(excluded) = excluded {
        query = query.filter(proxies::Column::Id.ne(excluded.get()));
    }
    if query
        .into_tuple::<i64>()
        .one(transaction)
        .await
        .map_err(|_| query_error("credential_proxy_name"))?
        .is_some()
    {
        return Err(CredentialProxyRepositoryError::Conflict);
    }
    Ok(())
}

async fn referenced_channels(
    transaction: &DatabaseTransaction,
    proxy_id: ProxyId,
) -> Result<Vec<ChannelId>, CredentialProxyRepositoryError> {
    credentials::Entity::find()
        .select_only()
        .column(credentials::Column::ChannelId)
        .filter(credentials::Column::ProxyId.eq(proxy_id.get()))
        .filter(credentials::Column::DeletedAt.is_null())
        .distinct()
        .into_tuple::<i64>()
        .all(transaction)
        .await
        .map_err(|_| query_error("credential_proxy_references"))?
        .into_iter()
        .map(|id| {
            ChannelId::new(id)
                .map_err(|_| internal_error(CredentialProxyRepositoryError::Invariant))
        })
        .collect()
}

fn record_from_model(
    model: proxies::Model,
) -> Result<CredentialProxyRecord, CredentialProxyRepositoryError> {
    let proxy_id = ProxyId::new(model.id)
        .map_err(|_| internal_error(CredentialProxyRepositoryError::Invariant))?;
    let scheme = CredentialProxyScheme::parse(&model.scheme)?;
    let port = u16::try_from(model.port)
        .map_err(|_| internal_error(CredentialProxyRepositoryError::Invariant))?;
    let host = model.host.as_str().to_owned();
    let username = model.username.map(|value| value.as_str().to_owned());
    let password_secret = model.password_secret.map(envelope_from_json).transpose()?;
    let created_at = model.created_at.unix_timestamp();
    let updated_at = model.updated_at.unix_timestamp();
    validate_proxy_fields(&model.name, &host, port, username.as_deref())?;
    if model.deleted_at.is_some()
        || model.active_name.as_deref() != Some(model.name.as_str())
        || username.is_some() != password_secret.is_some()
        || model.version < 1
        || created_at < 0
        || updated_at < created_at
    {
        return Err(internal_error(CredentialProxyRepositoryError::Invariant));
    }
    Ok(CredentialProxyRecord {
        proxy_id,
        name: model.name,
        scheme,
        host,
        port,
        username,
        password_secret,
        trust_proxy_dns: model.trust_proxy_dns,
        enabled: model.enabled,
        version: model.version,
        created_at,
        updated_at,
    })
}

fn validate_proxy_fields(
    name: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
) -> Result<(), CredentialProxyRepositoryError> {
    if !valid_text(name, MAX_CREDENTIAL_PROXY_NAME_BYTES)
        || port == 0
        || !valid_proxy_host(host)
        || username.is_some_and(|value| !valid_text(value, MAX_CREDENTIAL_PROXY_USERNAME_BYTES))
    {
        return Err(CredentialProxyRepositoryError::InvalidInput);
    }
    Ok(())
}

fn record_authentication_shape(username: bool, password: bool) -> Result<(), ()> {
    (username == password).then_some(()).ok_or(())
}

fn valid_proxy_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CREDENTIAL_PROXY_HOST_BYTES
        && value.trim() == value
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        && !value.contains("://")
        && !value.contains(['/', '\\', '@'])
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn pending_encrypted_json() -> Result<EncryptedJson, CredentialProxyRepositoryError> {
    encrypted_json(
        EncryptedCredentialEnvelope::new("pending", [1; 24], vec![1; 16])
            .map_err(|_| internal_error(CredentialProxyRepositoryError::Invariant))?,
    )
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, CredentialProxyRepositoryError> {
    EncryptedJson::from_envelope(json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| internal_error(CredentialProxyRepositoryError::Invariant))
}

fn envelope_from_json(
    secret: EncryptedJson,
) -> Result<EncryptedCredentialEnvelope, CredentialProxyRepositoryError> {
    let (key_id, nonce, ciphertext) = secret
        .envelope_parts()
        .map_err(|_| internal_error(CredentialProxyRepositoryError::Invariant))?;
    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
        .map_err(|_| internal_error(CredentialProxyRepositoryError::Invariant))
}

fn query_error(operation: &'static str) -> CredentialProxyRepositoryError {
    tracing::error!(target: "af_db::credential_proxy", error_kind = operation, "专属代理数据库操作失败");
    CredentialProxyRepositoryError::Query
}

/// 将数据库唯一约束稳定收敛为名称冲突，避免并发写入暴露底层错误。
fn proxy_write_error(error: &DbErr, operation: &'static str) -> CredentialProxyRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        CredentialProxyRepositoryError::Conflict
    } else {
        query_error(operation)
    }
}

fn internal_error(error: CredentialProxyRepositoryError) -> CredentialProxyRepositoryError {
    tracing::error!(target: "af_db::credential_proxy", error_kind = ?error, "专属代理内部状态无效");
    error
}
