use std::{fmt, net::IpAddr, time::Duration};

use af_domain::{GroupId, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use serde_json::json;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use url::Url;

use crate::{
    AdminUserCreateRecord, DatabasePool, EncryptedCredentialEnvelope,
    admin_user_write::create_user_in_transaction,
    entity::{
        EncryptedJson, SensitiveString, authentication_settings, oauth_login_providers,
        oauth_login_transactions, user_oauth_identities, users,
    },
};

pub const OAUTH_LOGIN_GITHUB_PROVIDER: &str = "github";
pub const OAUTH_LOGIN_DISCORD_PROVIDER: &str = "discord";
pub const OAUTH_LOGIN_OIDC_PROVIDER: &str = "oidc";
pub const OAUTH_LOGIN_LINUXDO_PROVIDER: &str = "linuxdo";
pub const OAUTH_LOGIN_WECHAT_PROVIDER: &str = "wechat";
pub const OAUTH_LOGIN_TELEGRAM_PROVIDER: &str = "telegram";
pub const OAUTH_LOGIN_GOOGLE_PROVIDER: &str = "google";
const LINUXDO_ISSUER: &str = "https://connect.linux.do";
const TELEGRAM_ISSUER: &str = "https://oauth.telegram.org";
const GOOGLE_ISSUER: &str = "https://accounts.google.com";
const AUTHENTICATION_SETTINGS_ID: i16 = 1;
const ACTIVE_USER_STATUS: i16 = 1;

/// OAuth 登录 Provider Client Secret 的更新语义。
pub enum OAuthLoginSecretUpdate {
    Keep,
    Replace(EncryptedCredentialEnvelope),
    Clear,
}

impl fmt::Debug for OAuthLoginSecretUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Keep => "OAuthLoginSecretUpdate::Keep",
            Self::Replace(_) => "OAuthLoginSecretUpdate::Replace(<已脱敏>)",
            Self::Clear => "OAuthLoginSecretUpdate::Clear",
        })
    }
}

/// 已完成不变量校验的 OAuth 登录 Provider 配置。
#[derive(Clone, Eq, PartialEq)]
pub struct OAuthLoginProviderRecord {
    provider: String,
    enabled: bool,
    client_id: Option<String>,
    issuer_url: Option<String>,
    client_secret: Option<EncryptedCredentialEnvelope>,
    version: i64,
}

impl OAuthLoginProviderRecord {
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    #[must_use]
    pub fn issuer_url(&self) -> Option<&str> {
        self.issuer_url.as_deref()
    }

    #[must_use]
    pub const fn client_secret_configured(&self) -> bool {
        self.client_secret.is_some()
    }

    #[must_use]
    pub fn client_secret(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.client_secret.as_ref()
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    #[must_use]
    pub fn available(&self) -> bool {
        self.enabled
            && self.client_id.is_some()
            && self.client_secret.is_some()
            && match self.provider.as_str() {
                OAUTH_LOGIN_OIDC_PROVIDER => self.issuer_url.is_some(),
                OAUTH_LOGIN_LINUXDO_PROVIDER => self.issuer_url.as_deref() == Some(LINUXDO_ISSUER),
                OAUTH_LOGIN_TELEGRAM_PROVIDER => {
                    self.issuer_url.as_deref() == Some(TELEGRAM_ISSUER)
                }
                OAUTH_LOGIN_GOOGLE_PROVIDER => self.issuer_url.as_deref() == Some(GOOGLE_ISSUER),
                OAUTH_LOGIN_GITHUB_PROVIDER
                | OAUTH_LOGIN_DISCORD_PROVIDER
                | OAUTH_LOGIN_WECHAT_PROVIDER => self.issuer_url.is_none(),
                _ => false,
            }
    }
}

impl fmt::Debug for OAuthLoginProviderRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthLoginProviderRecord")
            .field("provider", &self.provider)
            .field("enabled", &self.enabled)
            .field("client_id_configured", &self.client_id.is_some())
            .field("client_secret_configured", &self.client_secret.is_some())
            .field("version", &self.version)
            .finish()
    }
}

/// 管理员使用版本 CAS 完整保存 Provider 配置的写入记录。
pub struct OAuthLoginProviderWriteRecord {
    expected_version: i64,
    enabled: bool,
    client_id: Option<String>,
    issuer_url: Option<String>,
    client_secret: OAuthLoginSecretUpdate,
}

impl OAuthLoginProviderWriteRecord {
    #[must_use]
    pub fn new(
        expected_version: i64,
        enabled: bool,
        client_id: Option<String>,
        issuer_url: Option<String>,
        client_secret: OAuthLoginSecretUpdate,
    ) -> Self {
        Self {
            expected_version,
            enabled,
            client_id,
            issuer_url,
            client_secret,
        }
    }
}

impl fmt::Debug for OAuthLoginProviderWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OAuthLoginProviderWriteRecord(<已脱敏>)")
    }
}

/// 回调成功领取 state 后返回的内部事务标识。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OAuthLoginStateClaim {
    transaction_id: i64,
}

impl OAuthLoginStateClaim {
    #[must_use]
    pub const fn transaction_id(self) -> i64 {
        self.transaction_id
    }
}

/// 外部身份解析和登录票据写入结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthIdentityCompletion {
    Issued(UserId),
    Rejected,
}

/// OAuth 登录数据库操作的闭合错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OAuthLoginRepositoryError {
    #[error("OAuth 登录输入无效")]
    InvalidInput,
    #[error("OAuth 登录配置发生并发更新")]
    ConcurrentUpdate,
    #[error("OAuth 登录凭据已拒绝")]
    Rejected,
    #[error("OAuth 登录身份发生并发冲突")]
    Conflict,
    #[error("OAuth 登录数据库查询失败")]
    Query,
    #[error("OAuth 登录数据库操作超时")]
    Timeout,
    #[error("OAuth 登录持久化状态无效")]
    Invariant,
}

/// Provider 配置、外部身份和一次性事务的生产数据库仓储。
#[derive(Clone)]
pub struct OAuthLoginRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OAuthLoginRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OAuthLoginRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OAuthLoginRepositoryError::InvalidInput);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    pub async fn provider(
        &self,
        provider: &str,
    ) -> Result<OAuthLoginProviderRecord, OAuthLoginRepositoryError> {
        match timeout(self.operation_timeout, self.provider_inner(provider)).await {
            Ok(result) => result,
            Err(_) => Err(internal(OAuthLoginRepositoryError::Timeout)),
        }
    }

    pub async fn update_provider(
        &self,
        provider: &str,
        write: OAuthLoginProviderWriteRecord,
    ) -> Result<OAuthLoginProviderRecord, OAuthLoginRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.update_provider_inner(provider, write),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal(OAuthLoginRepositoryError::Timeout)),
        }
    }

    pub async fn create_state(
        &self,
        provider: &str,
        state_digest: &str,
        expires_at: u64,
    ) -> Result<(), OAuthLoginRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.create_state_inner(provider, state_digest, expires_at),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal(OAuthLoginRepositoryError::Timeout)),
        }
    }

    pub async fn claim_state(
        &self,
        provider: &str,
        state_digest: &str,
        claimed_at: u64,
    ) -> Result<OAuthLoginStateClaim, OAuthLoginRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.claim_state_inner(provider, state_digest, claimed_at),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal(OAuthLoginRepositoryError::Timeout)),
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "字段组成一次不可分割的 OAuth 身份完成事实"
    )]
    pub async fn complete_identity(
        &self,
        claim: OAuthLoginStateClaim,
        provider: &str,
        subject: &str,
        local_username: &str,
        ticket_digest: &str,
        ticket_expires_at: u64,
        completed_at: u64,
    ) -> Result<OAuthIdentityCompletion, OAuthLoginRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.complete_identity_inner(
                claim,
                provider,
                subject,
                local_username,
                ticket_digest,
                ticket_expires_at,
                completed_at,
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal(OAuthLoginRepositoryError::Timeout)),
        }
    }

    pub async fn consume_ticket(
        &self,
        ticket_digest: &str,
        consumed_at: u64,
    ) -> Result<UserId, OAuthLoginRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.consume_ticket_inner(ticket_digest, consumed_at),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal(OAuthLoginRepositoryError::Timeout)),
        }
    }

    async fn provider_inner(
        &self,
        provider: &str,
    ) -> Result<OAuthLoginProviderRecord, OAuthLoginRepositoryError> {
        validate_provider(provider)?;
        let model = oauth_login_providers::Entity::find_by_id(provider.to_owned())
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_provider_read"))?
            .ok_or_else(|| internal(OAuthLoginRepositoryError::Invariant))?;
        provider_from_model(model)
    }

    async fn update_provider_inner(
        &self,
        provider: &str,
        write: OAuthLoginProviderWriteRecord,
    ) -> Result<OAuthLoginProviderRecord, OAuthLoginRepositoryError> {
        validate_provider(provider)?;
        let current = self.provider_inner(provider).await?;
        let client_id = normalize_client_id(write.client_id)?;
        let issuer_url = normalize_issuer_url(write.issuer_url)?;
        validate_provider_issuer(provider, issuer_url.as_deref(), write.enabled)?;
        let secret = match write.client_secret {
            OAuthLoginSecretUpdate::Keep => current.client_secret,
            OAuthLoginSecretUpdate::Replace(envelope) => Some(envelope),
            OAuthLoginSecretUpdate::Clear => None,
        };
        if write.expected_version < 1
            || (write.enabled && (client_id.is_none() || secret.is_none()))
        {
            return Err(OAuthLoginRepositoryError::InvalidInput);
        }
        let version = write
            .expected_version
            .checked_add(1)
            .ok_or_else(|| internal(OAuthLoginRepositoryError::Invariant))?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = oauth_login_providers::Entity::update_many()
            .filter(oauth_login_providers::Column::Provider.eq(provider))
            .filter(oauth_login_providers::Column::Version.eq(write.expected_version))
            .col_expr(
                oauth_login_providers::Column::Enabled,
                Expr::value(write.enabled),
            )
            .col_expr(
                oauth_login_providers::Column::ClientId,
                Expr::value(client_id),
            )
            .col_expr(
                oauth_login_providers::Column::IssuerUrl,
                Expr::value(issuer_url),
            )
            .col_expr(
                oauth_login_providers::Column::ClientSecret,
                Expr::value(secret.map(encrypted_json).transpose()?),
            )
            .col_expr(oauth_login_providers::Column::Version, Expr::value(version))
            .col_expr(oauth_login_providers::Column::UpdatedAt, Expr::value(now))
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_provider_update"))?;
        match result.rows_affected {
            1 => self.provider_inner(provider).await,
            0 => Err(OAuthLoginRepositoryError::ConcurrentUpdate),
            _ => Err(internal(OAuthLoginRepositoryError::Invariant)),
        }
    }

    async fn create_state_inner(
        &self,
        provider: &str,
        state_digest: &str,
        expires_at: u64,
    ) -> Result<(), OAuthLoginRepositoryError> {
        validate_provider(provider)?;
        validate_digest(state_digest)?;
        let expires_at = timestamp(expires_at)?;
        oauth_login_transactions::ActiveModel {
            provider: Set(provider.to_owned()),
            state_digest: Set(SensitiveString::from(state_digest)),
            expires_at: Set(expires_at),
            ..Default::default()
        }
        .insert(self.pool.connection())
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|_| ())
        .map_err(map_write_error)
    }

    async fn claim_state_inner(
        &self,
        provider: &str,
        state_digest: &str,
        claimed_at: u64,
    ) -> Result<OAuthLoginStateClaim, OAuthLoginRepositoryError> {
        validate_provider(provider)?;
        validate_digest(state_digest)?;
        let now = timestamp(claimed_at)?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_state_begin"))?;
        let result = oauth_login_transactions::Entity::update_many()
            .filter(oauth_login_transactions::Column::Provider.eq(provider))
            .filter(
                oauth_login_transactions::Column::StateDigest
                    .eq(SensitiveString::from(state_digest)),
            )
            .filter(oauth_login_transactions::Column::ClaimedAt.is_null())
            .filter(oauth_login_transactions::Column::ExpiresAt.gt(now))
            .col_expr(
                oauth_login_transactions::Column::ClaimedAt,
                Expr::value(Some(now)),
            )
            .col_expr(
                oauth_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_state_claim"))?;
        if result.rows_affected != 1 {
            transaction
                .rollback()
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| query("oauth_login_state_rollback"))?;
            return Err(OAuthLoginRepositoryError::Rejected);
        }
        let model = oauth_login_transactions::Entity::find()
            .filter(oauth_login_transactions::Column::Provider.eq(provider))
            .filter(
                oauth_login_transactions::Column::StateDigest
                    .eq(SensitiveString::from(state_digest)),
            )
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_state_read"))?
            .ok_or_else(|| internal(OAuthLoginRepositoryError::Invariant))?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_state_commit"))?;
        Ok(OAuthLoginStateClaim {
            transaction_id: model.id,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "字段组成一次不可分割的 OAuth 身份完成事实"
    )]
    async fn complete_identity_inner(
        &self,
        claim: OAuthLoginStateClaim,
        provider: &str,
        subject: &str,
        local_username: &str,
        ticket_digest: &str,
        ticket_expires_at: u64,
        completed_at: u64,
    ) -> Result<OAuthIdentityCompletion, OAuthLoginRepositoryError> {
        validate_provider(provider)?;
        validate_subject(subject)?;
        validate_username(local_username)?;
        validate_digest(ticket_digest)?;
        let now = timestamp(completed_at)?;
        let ticket_expires_at = timestamp(ticket_expires_at)?;
        if ticket_expires_at <= now {
            return Err(OAuthLoginRepositoryError::InvalidInput);
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_identity_begin"))?;
        let login = lock_login_transaction(&transaction, claim.transaction_id).await?;
        if login.provider != provider
            || login.claimed_at.is_none()
            || login.user_id.is_some()
            || login.ticket_digest.is_some()
            || login.exchanged_at.is_some()
            || login.expires_at <= now
        {
            return Err(OAuthLoginRepositoryError::Rejected);
        }

        let user_id = match find_identity_user(&transaction, provider, subject).await? {
            Some(user_id) => user_id,
            None => {
                let settings =
                    authentication_settings::Entity::find_by_id(AUTHENTICATION_SETTINGS_ID)
                        .one(&transaction)
                        .with_subscriber(NoSubscriber::default())
                        .await
                        .map_err(|_| query("oauth_login_registration_policy"))?
                        .ok_or_else(|| internal(OAuthLoginRepositoryError::Invariant))?;
                let Some(default_group_id) = settings.registration_default_group_id else {
                    return Ok(OAuthIdentityCompletion::Rejected);
                };
                if !settings.registration_enabled || settings.registration_initial_quota < 0 {
                    return Ok(OAuthIdentityCompletion::Rejected);
                }
                let group_id = GroupId::new(default_group_id)
                    .map_err(|_| internal(OAuthLoginRepositoryError::Invariant))?;
                let user = create_user_in_transaction(
                    &transaction,
                    AdminUserCreateRecord::new(
                        local_username.to_owned(),
                        None,
                        None,
                        0,
                        ACTIVE_USER_STATUS,
                        group_id,
                        settings.registration_initial_quota,
                        None,
                        None,
                    ),
                )
                .await
                .map_err(map_user_error)?;
                user_oauth_identities::ActiveModel {
                    user_id: Set(user.user_id().get()),
                    provider: Set(provider.to_owned()),
                    subject: Set(subject.to_owned()),
                    ..Default::default()
                }
                .insert(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_write_error)?;
                user.user_id()
            }
        };

        let result = oauth_login_transactions::Entity::update_many()
            .filter(oauth_login_transactions::Column::Id.eq(claim.transaction_id))
            .filter(oauth_login_transactions::Column::UserId.is_null())
            .filter(oauth_login_transactions::Column::TicketDigest.is_null())
            .col_expr(
                oauth_login_transactions::Column::UserId,
                Expr::value(Some(user_id.get())),
            )
            .col_expr(
                oauth_login_transactions::Column::TicketDigest,
                Expr::value(Some(SensitiveString::from(ticket_digest))),
            )
            .col_expr(
                oauth_login_transactions::Column::TicketExpiresAt,
                Expr::value(Some(ticket_expires_at)),
            )
            .col_expr(
                oauth_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        if result.rows_affected != 1 {
            return Err(internal(OAuthLoginRepositoryError::Invariant));
        }
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_identity_commit"))?;
        Ok(OAuthIdentityCompletion::Issued(user_id))
    }

    async fn consume_ticket_inner(
        &self,
        ticket_digest: &str,
        consumed_at: u64,
    ) -> Result<UserId, OAuthLoginRepositoryError> {
        validate_digest(ticket_digest)?;
        let now = timestamp(consumed_at)?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_ticket_begin"))?;
        let result = oauth_login_transactions::Entity::update_many()
            .filter(
                oauth_login_transactions::Column::TicketDigest
                    .eq(SensitiveString::from(ticket_digest)),
            )
            .filter(oauth_login_transactions::Column::UserId.is_not_null())
            .filter(oauth_login_transactions::Column::ExchangedAt.is_null())
            .filter(oauth_login_transactions::Column::TicketExpiresAt.gt(now))
            .col_expr(
                oauth_login_transactions::Column::ExchangedAt,
                Expr::value(Some(now)),
            )
            .col_expr(
                oauth_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_ticket_consume"))?;
        if result.rows_affected != 1 {
            transaction
                .rollback()
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| query("oauth_login_ticket_rollback"))?;
            return Err(OAuthLoginRepositoryError::Rejected);
        }
        let model = oauth_login_transactions::Entity::find()
            .filter(
                oauth_login_transactions::Column::TicketDigest
                    .eq(SensitiveString::from(ticket_digest)),
            )
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_ticket_read"))?
            .ok_or_else(|| internal(OAuthLoginRepositoryError::Invariant))?;
        let user_id = model
            .user_id
            .and_then(|value| UserId::new(value).ok())
            .ok_or_else(|| internal(OAuthLoginRepositoryError::Invariant))?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_ticket_commit"))?;
        Ok(user_id)
    }
}

impl fmt::Debug for OAuthLoginRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthLoginRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn lock_login_transaction(
    transaction: &DatabaseTransaction,
    transaction_id: i64,
) -> Result<oauth_login_transactions::Model, OAuthLoginRepositoryError> {
    if transaction_id <= 0 {
        return Err(OAuthLoginRepositoryError::InvalidInput);
    }
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = oauth_login_transactions::Entity::update_many()
            .filter(oauth_login_transactions::Column::Id.eq(transaction_id))
            .col_expr(
                oauth_login_transactions::Column::UpdatedAt,
                Expr::col(oauth_login_transactions::Column::UpdatedAt).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("oauth_login_transaction_lock"))?;
        if result.rows_affected != 1 {
            return Err(OAuthLoginRepositoryError::Rejected);
        }
    }
    let mut select = oauth_login_transactions::Entity::find_by_id(transaction_id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        select = select.lock(LockType::Update);
    }
    select
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query("oauth_login_transaction_read"))?
        .ok_or(OAuthLoginRepositoryError::Rejected)
}

async fn find_identity_user(
    transaction: &DatabaseTransaction,
    provider: &str,
    subject: &str,
) -> Result<Option<UserId>, OAuthLoginRepositoryError> {
    let identity = user_oauth_identities::Entity::find()
        .filter(user_oauth_identities::Column::Provider.eq(provider))
        .filter(user_oauth_identities::Column::Subject.eq(subject))
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query("oauth_login_identity_read"))?;
    let Some(identity) = identity else {
        return Ok(None);
    };
    let user = users::Entity::find_by_id(identity.user_id)
        .filter(users::Column::Status.eq(ACTIVE_USER_STATUS))
        .filter(users::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query("oauth_login_identity_user_read"))?;
    match user {
        Some(user) => UserId::new(user.id)
            .map(Some)
            .map_err(|_| internal(OAuthLoginRepositoryError::Invariant)),
        None => Err(OAuthLoginRepositoryError::Rejected),
    }
}

fn provider_from_model(
    model: oauth_login_providers::Model,
) -> Result<OAuthLoginProviderRecord, OAuthLoginRepositoryError> {
    validate_provider(&model.provider)?;
    let client_id = normalize_client_id(model.client_id)?;
    let client_secret = model.client_secret.map(envelope_from_json).transpose()?;
    let issuer_url = normalize_issuer_url(model.issuer_url)?;
    validate_provider_issuer(&model.provider, issuer_url.as_deref(), model.enabled)
        .map_err(|_| internal(OAuthLoginRepositoryError::Invariant))?;
    if model.version < 1 || (model.enabled && (client_id.is_none() || client_secret.is_none())) {
        return Err(internal(OAuthLoginRepositoryError::Invariant));
    }
    Ok(OAuthLoginProviderRecord {
        provider: model.provider,
        enabled: model.enabled,
        client_id,
        issuer_url,
        client_secret,
        version: model.version,
    })
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, OAuthLoginRepositoryError> {
    EncryptedJson::from_envelope(json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| internal(OAuthLoginRepositoryError::Invariant))
}

fn envelope_from_json(
    secret: EncryptedJson,
) -> Result<EncryptedCredentialEnvelope, OAuthLoginRepositoryError> {
    let (key_id, nonce, ciphertext) = secret
        .envelope_parts()
        .map_err(|_| internal(OAuthLoginRepositoryError::Invariant))?;
    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
        .map_err(|_| internal(OAuthLoginRepositoryError::Invariant))
}

fn normalize_client_id(
    client_id: Option<String>,
) -> Result<Option<String>, OAuthLoginRepositoryError> {
    let client_id = client_id.and_then(|value| {
        let trimmed = value.trim().to_owned();
        (!trimmed.is_empty()).then_some(trimmed)
    });
    if client_id.as_deref().is_some_and(|value| {
        value.len() > 255
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    }) {
        return Err(OAuthLoginRepositoryError::InvalidInput);
    }
    Ok(client_id)
}

fn validate_provider(provider: &str) -> Result<(), OAuthLoginRepositoryError> {
    if [
        OAUTH_LOGIN_GITHUB_PROVIDER,
        OAUTH_LOGIN_DISCORD_PROVIDER,
        OAUTH_LOGIN_OIDC_PROVIDER,
        OAUTH_LOGIN_LINUXDO_PROVIDER,
        OAUTH_LOGIN_WECHAT_PROVIDER,
        OAUTH_LOGIN_TELEGRAM_PROVIDER,
        OAUTH_LOGIN_GOOGLE_PROVIDER,
    ]
    .contains(&provider)
    {
        Ok(())
    } else {
        Err(OAuthLoginRepositoryError::InvalidInput)
    }
}

fn normalize_issuer_url(
    issuer_url: Option<String>,
) -> Result<Option<String>, OAuthLoginRepositoryError> {
    let issuer_url = issuer_url.and_then(|value| {
        let trimmed = value.trim().trim_end_matches('/').to_owned();
        (!trimmed.is_empty()).then_some(trimmed)
    });
    if issuer_url.as_deref().is_some_and(|value| {
        let Ok(url) = Url::parse(value) else {
            return true;
        };
        value.len() > 2048
            || url.scheme() != "https"
            || url.host_str().is_none()
            || url.username() != ""
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.port().is_some()
            || value.chars().any(char::is_control)
            || is_blocked_issuer_host(url.host_str().unwrap_or_default())
    }) {
        return Err(OAuthLoginRepositoryError::InvalidInput);
    }
    Ok(issuer_url)
}

fn validate_provider_issuer(
    provider: &str,
    issuer: Option<&str>,
    enabled: bool,
) -> Result<(), OAuthLoginRepositoryError> {
    let valid = match provider {
        OAUTH_LOGIN_OIDC_PROVIDER => issuer.is_some(),
        OAUTH_LOGIN_LINUXDO_PROVIDER => issuer == Some(LINUXDO_ISSUER),
        OAUTH_LOGIN_TELEGRAM_PROVIDER => issuer == Some(TELEGRAM_ISSUER),
        OAUTH_LOGIN_GOOGLE_PROVIDER => issuer == Some(GOOGLE_ISSUER),
        OAUTH_LOGIN_GITHUB_PROVIDER
        | OAUTH_LOGIN_DISCORD_PROVIDER
        | OAUTH_LOGIN_WECHAT_PROVIDER => issuer.is_none(),
        _ => false,
    };
    let invalid_disabled_issuer = !enabled
        && issuer.is_some()
        && provider != OAUTH_LOGIN_OIDC_PROVIDER
        && provider != OAUTH_LOGIN_LINUXDO_PROVIDER
        && provider != OAUTH_LOGIN_TELEGRAM_PROVIDER
        && provider != OAUTH_LOGIN_GOOGLE_PROVIDER;
    if (enabled && !valid) || invalid_disabled_issuer {
        Err(OAuthLoginRepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}

fn is_blocked_issuer_host(host: &str) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    normalized == "localhost"
        || normalized.ends_with(".localhost")
        || normalized == "localhost.localdomain"
        || normalized.parse::<IpAddr>().is_ok()
}

fn validate_subject(subject: &str) -> Result<(), OAuthLoginRepositoryError> {
    if !subject.is_empty()
        && subject.len() <= 255
        && subject.trim() == subject
        && !subject.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(OAuthLoginRepositoryError::InvalidInput)
    }
}

fn validate_username(username: &str) -> Result<(), OAuthLoginRepositoryError> {
    if !username.is_empty()
        && username.len() <= 64
        && username.trim() == username
        && !username.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(OAuthLoginRepositoryError::InvalidInput)
    }
}

fn validate_digest(digest: &str) -> Result<(), OAuthLoginRepositoryError> {
    if digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(OAuthLoginRepositoryError::InvalidInput)
    }
}

fn timestamp(value: u64) -> Result<TimeDateTimeWithTimeZone, OAuthLoginRepositoryError> {
    let value = i64::try_from(value).map_err(|_| OAuthLoginRepositoryError::InvalidInput)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| OAuthLoginRepositoryError::InvalidInput)
}

fn map_user_error(error: crate::AdminUserRepositoryError) -> OAuthLoginRepositoryError {
    match error {
        crate::AdminUserRepositoryError::Conflict => OAuthLoginRepositoryError::Conflict,
        crate::AdminUserRepositoryError::InvalidReference => OAuthLoginRepositoryError::Rejected,
        crate::AdminUserRepositoryError::Query
        | crate::AdminUserRepositoryError::Timeout
        | crate::AdminUserRepositoryError::Invariant
        | crate::AdminUserRepositoryError::Entropy => {
            internal(OAuthLoginRepositoryError::Invariant)
        }
    }
}

fn map_write_error(error: sea_orm::DbErr) -> OAuthLoginRepositoryError {
    let rendered = error.to_string();
    if rendered.contains("uq_user_oauth_identities")
        || rendered.contains("uq_oauth_login_transactions")
        || rendered.contains("UNIQUE constraint failed")
    {
        return OAuthLoginRepositoryError::Conflict;
    }
    query("oauth_login_write")
}

fn query(operation: &'static str) -> OAuthLoginRepositoryError {
    tracing::error!(
        target: "af_db::oauth_login",
        error_kind = operation,
        "OAuth 登录数据库操作失败"
    );
    OAuthLoginRepositoryError::Query
}

fn internal(error: OAuthLoginRepositoryError) -> OAuthLoginRepositoryError {
    tracing::error!(
        target: "af_db::oauth_login",
        error_kind = ?error,
        "OAuth 登录持久化状态无效"
    );
    error
}
