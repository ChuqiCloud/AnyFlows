use std::{fmt, time::Duration};

use af_domain::{GroupId, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminUserCreateRecord, DatabasePool,
    admin_user_write::create_user_in_transaction,
    entity::{
        SensitiveString, authentication_settings, custom_oauth2_identities,
        custom_oauth2_login_transactions, users,
    },
};

const AUTHENTICATION_SETTINGS_ID: i16 = 1;
const ACTIVE_USER_STATUS: i16 = 1;

/// 回调原子领取 state 后返回的内部事务标识和配置版本。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CustomOAuth2LoginStateClaim {
    transaction_id: i64,
    configuration_version: i64,
}

impl CustomOAuth2LoginStateClaim {
    #[must_use]
    pub const fn transaction_id(self) -> i64 {
        self.transaction_id
    }

    #[must_use]
    pub const fn configuration_version(self) -> i64 {
        self.configuration_version
    }
}

/// 自定义 OAuth2 身份解析和短期票据写入结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomOAuth2IdentityCompletion {
    Issued(UserId),
    Rejected,
}

/// 自定义 OAuth2 登录持久化操作的闭合错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CustomOAuth2LoginRepositoryError {
    #[error("自定义 OAuth2 登录输入无效")]
    InvalidInput,
    #[error("自定义 OAuth2 登录凭据已拒绝")]
    Rejected,
    #[error("自定义 OAuth2 登录身份发生并发冲突")]
    Conflict,
    #[error("自定义 OAuth2 登录数据库查询失败")]
    Query,
    #[error("自定义 OAuth2 登录数据库操作超时")]
    Timeout,
    #[error("自定义 OAuth2 登录持久化状态无效")]
    Invariant,
}

/// 自定义 Provider 身份和单次事务的独立生产数据库仓储。
#[derive(Clone)]
pub struct CustomOAuth2LoginRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl CustomOAuth2LoginRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, CustomOAuth2LoginRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(CustomOAuth2LoginRepositoryError::InvalidInput);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 保存带 Provider 配置版本的 state 摘要，明文 verifier 不进入数据库。
    pub async fn create_state(
        &self,
        provider_key: &str,
        configuration_version: i64,
        state_digest: &str,
        expires_at: u64,
    ) -> Result<(), CustomOAuth2LoginRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.create_state_inner(
                provider_key,
                configuration_version,
                state_digest,
                expires_at,
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal(CustomOAuth2LoginRepositoryError::Timeout)),
        }
    }

    /// 原子领取未过期 state；返回启动时绑定的配置版本供服务层复验。
    pub async fn claim_state(
        &self,
        provider_key: &str,
        state_digest: &str,
        claimed_at: u64,
    ) -> Result<CustomOAuth2LoginStateClaim, CustomOAuth2LoginRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.claim_state_inner(provider_key, state_digest, claimed_at),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal(CustomOAuth2LoginRepositoryError::Timeout)),
        }
    }

    /// 在同一事务内恢复或创建外部身份，并写入短期单次登录票据。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段共同组成不可分割的自定义 OAuth2 身份完成事实"
    )]
    pub async fn complete_identity(
        &self,
        claim: CustomOAuth2LoginStateClaim,
        provider_key: &str,
        subject: &str,
        local_username: &str,
        ticket_digest: &str,
        ticket_expires_at: u64,
        completed_at: u64,
    ) -> Result<CustomOAuth2IdentityCompletion, CustomOAuth2LoginRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.complete_identity_inner(
                claim,
                provider_key,
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
            Err(_) => Err(internal(CustomOAuth2LoginRepositoryError::Timeout)),
        }
    }

    /// 原子消费短期票据，重复、过期和未知票据统一拒绝。
    pub async fn consume_ticket(
        &self,
        ticket_digest: &str,
        consumed_at: u64,
    ) -> Result<UserId, CustomOAuth2LoginRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.consume_ticket_inner(ticket_digest, consumed_at),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal(CustomOAuth2LoginRepositoryError::Timeout)),
        }
    }

    async fn create_state_inner(
        &self,
        provider_key: &str,
        configuration_version: i64,
        state_digest: &str,
        expires_at: u64,
    ) -> Result<(), CustomOAuth2LoginRepositoryError> {
        validate_provider_key(provider_key)?;
        validate_configuration_version(configuration_version)?;
        validate_digest(state_digest)?;
        custom_oauth2_login_transactions::ActiveModel {
            provider_key: Set(provider_key.to_owned()),
            configuration_version: Set(configuration_version),
            state_digest: Set(SensitiveString::from(state_digest)),
            expires_at: Set(timestamp(expires_at)?),
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
        provider_key: &str,
        state_digest: &str,
        claimed_at: u64,
    ) -> Result<CustomOAuth2LoginStateClaim, CustomOAuth2LoginRepositoryError> {
        validate_provider_key(provider_key)?;
        validate_digest(state_digest)?;
        let now = timestamp(claimed_at)?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_state_begin"))?;
        let result = custom_oauth2_login_transactions::Entity::update_many()
            .filter(custom_oauth2_login_transactions::Column::ProviderKey.eq(provider_key))
            .filter(
                custom_oauth2_login_transactions::Column::StateDigest
                    .eq(SensitiveString::from(state_digest)),
            )
            .filter(custom_oauth2_login_transactions::Column::ClaimedAt.is_null())
            .filter(custom_oauth2_login_transactions::Column::ExpiresAt.gt(now))
            .col_expr(
                custom_oauth2_login_transactions::Column::ClaimedAt,
                Expr::value(Some(now)),
            )
            .col_expr(
                custom_oauth2_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_state_claim"))?;
        if result.rows_affected != 1 {
            transaction
                .rollback()
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| query("custom_oauth2_state_rollback"))?;
            return Err(CustomOAuth2LoginRepositoryError::Rejected);
        }
        let model = custom_oauth2_login_transactions::Entity::find()
            .filter(custom_oauth2_login_transactions::Column::ProviderKey.eq(provider_key))
            .filter(
                custom_oauth2_login_transactions::Column::StateDigest
                    .eq(SensitiveString::from(state_digest)),
            )
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_state_read"))?
            .ok_or_else(|| internal(CustomOAuth2LoginRepositoryError::Invariant))?;
        validate_configuration_version(model.configuration_version)?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_state_commit"))?;
        Ok(CustomOAuth2LoginStateClaim {
            transaction_id: model.id,
            configuration_version: model.configuration_version,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "字段共同组成不可分割的自定义 OAuth2 身份完成事实"
    )]
    async fn complete_identity_inner(
        &self,
        claim: CustomOAuth2LoginStateClaim,
        provider_key: &str,
        subject: &str,
        local_username: &str,
        ticket_digest: &str,
        ticket_expires_at: u64,
        completed_at: u64,
    ) -> Result<CustomOAuth2IdentityCompletion, CustomOAuth2LoginRepositoryError> {
        validate_provider_key(provider_key)?;
        validate_subject(subject)?;
        validate_username(local_username)?;
        validate_digest(ticket_digest)?;
        validate_configuration_version(claim.configuration_version)?;
        let now = timestamp(completed_at)?;
        let ticket_expires_at = timestamp(ticket_expires_at)?;
        if ticket_expires_at <= now {
            return Err(CustomOAuth2LoginRepositoryError::InvalidInput);
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_identity_begin"))?;
        let login = lock_login_transaction(&transaction, claim.transaction_id).await?;
        if login.provider_key != provider_key
            || login.configuration_version != claim.configuration_version
            || login.claimed_at.is_none()
            || login.user_id.is_some()
            || login.ticket_digest.is_some()
            || login.exchanged_at.is_some()
            || login.expires_at <= now
        {
            return Err(CustomOAuth2LoginRepositoryError::Rejected);
        }

        let user_id = match find_identity_user(&transaction, provider_key, subject).await? {
            Some(user_id) => user_id,
            None => {
                let settings =
                    authentication_settings::Entity::find_by_id(AUTHENTICATION_SETTINGS_ID)
                        .one(&transaction)
                        .with_subscriber(NoSubscriber::default())
                        .await
                        .map_err(|_| query("custom_oauth2_registration_policy"))?
                        .ok_or_else(|| internal(CustomOAuth2LoginRepositoryError::Invariant))?;
                let Some(default_group_id) = settings.registration_default_group_id else {
                    return Ok(CustomOAuth2IdentityCompletion::Rejected);
                };
                if !settings.registration_enabled || settings.registration_initial_quota < 0 {
                    return Ok(CustomOAuth2IdentityCompletion::Rejected);
                }
                let group_id = GroupId::new(default_group_id)
                    .map_err(|_| internal(CustomOAuth2LoginRepositoryError::Invariant))?;
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
                custom_oauth2_identities::ActiveModel {
                    user_id: Set(user.user_id().get()),
                    provider_key: Set(provider_key.to_owned()),
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

        let result = custom_oauth2_login_transactions::Entity::update_many()
            .filter(custom_oauth2_login_transactions::Column::Id.eq(claim.transaction_id))
            .filter(custom_oauth2_login_transactions::Column::UserId.is_null())
            .filter(custom_oauth2_login_transactions::Column::TicketDigest.is_null())
            .col_expr(
                custom_oauth2_login_transactions::Column::UserId,
                Expr::value(Some(user_id.get())),
            )
            .col_expr(
                custom_oauth2_login_transactions::Column::TicketDigest,
                Expr::value(Some(SensitiveString::from(ticket_digest))),
            )
            .col_expr(
                custom_oauth2_login_transactions::Column::TicketExpiresAt,
                Expr::value(Some(ticket_expires_at)),
            )
            .col_expr(
                custom_oauth2_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        if result.rows_affected != 1 {
            return Err(internal(CustomOAuth2LoginRepositoryError::Invariant));
        }
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_identity_commit"))?;
        Ok(CustomOAuth2IdentityCompletion::Issued(user_id))
    }

    async fn consume_ticket_inner(
        &self,
        ticket_digest: &str,
        consumed_at: u64,
    ) -> Result<UserId, CustomOAuth2LoginRepositoryError> {
        validate_digest(ticket_digest)?;
        let now = timestamp(consumed_at)?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_ticket_begin"))?;
        let result = custom_oauth2_login_transactions::Entity::update_many()
            .filter(
                custom_oauth2_login_transactions::Column::TicketDigest
                    .eq(SensitiveString::from(ticket_digest)),
            )
            .filter(custom_oauth2_login_transactions::Column::UserId.is_not_null())
            .filter(custom_oauth2_login_transactions::Column::ExchangedAt.is_null())
            .filter(custom_oauth2_login_transactions::Column::TicketExpiresAt.gt(now))
            .col_expr(
                custom_oauth2_login_transactions::Column::ExchangedAt,
                Expr::value(Some(now)),
            )
            .col_expr(
                custom_oauth2_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_ticket_consume"))?;
        if result.rows_affected != 1 {
            transaction
                .rollback()
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| query("custom_oauth2_ticket_rollback"))?;
            return Err(CustomOAuth2LoginRepositoryError::Rejected);
        }
        let model = custom_oauth2_login_transactions::Entity::find()
            .filter(
                custom_oauth2_login_transactions::Column::TicketDigest
                    .eq(SensitiveString::from(ticket_digest)),
            )
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_ticket_read"))?
            .ok_or_else(|| internal(CustomOAuth2LoginRepositoryError::Invariant))?;
        let user_id = model
            .user_id
            .and_then(|value| UserId::new(value).ok())
            .ok_or_else(|| internal(CustomOAuth2LoginRepositoryError::Invariant))?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_ticket_commit"))?;
        Ok(user_id)
    }
}

impl fmt::Debug for CustomOAuth2LoginRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomOAuth2LoginRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn lock_login_transaction(
    transaction: &DatabaseTransaction,
    transaction_id: i64,
) -> Result<custom_oauth2_login_transactions::Model, CustomOAuth2LoginRepositoryError> {
    if transaction_id <= 0 {
        return Err(CustomOAuth2LoginRepositoryError::InvalidInput);
    }
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = custom_oauth2_login_transactions::Entity::update_many()
            .filter(custom_oauth2_login_transactions::Column::Id.eq(transaction_id))
            .col_expr(
                custom_oauth2_login_transactions::Column::UpdatedAt,
                Expr::col(custom_oauth2_login_transactions::Column::UpdatedAt).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("custom_oauth2_transaction_lock"))?;
        if result.rows_affected != 1 {
            return Err(CustomOAuth2LoginRepositoryError::Rejected);
        }
    }
    let mut select = custom_oauth2_login_transactions::Entity::find_by_id(transaction_id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        select = select.lock(LockType::Update);
    }
    select
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query("custom_oauth2_transaction_read"))?
        .ok_or(CustomOAuth2LoginRepositoryError::Rejected)
}

async fn find_identity_user(
    transaction: &DatabaseTransaction,
    provider_key: &str,
    subject: &str,
) -> Result<Option<UserId>, CustomOAuth2LoginRepositoryError> {
    let identity = custom_oauth2_identities::Entity::find()
        .filter(custom_oauth2_identities::Column::ProviderKey.eq(provider_key))
        .filter(custom_oauth2_identities::Column::Subject.eq(subject))
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query("custom_oauth2_identity_read"))?;
    let Some(identity) = identity else {
        return Ok(None);
    };
    let user = users::Entity::find_by_id(identity.user_id)
        .filter(users::Column::Status.eq(ACTIVE_USER_STATUS))
        .filter(users::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query("custom_oauth2_identity_user_read"))?;
    match user {
        Some(user) => UserId::new(user.id)
            .map(Some)
            .map_err(|_| internal(CustomOAuth2LoginRepositoryError::Invariant)),
        None => Err(CustomOAuth2LoginRepositoryError::Rejected),
    }
}

fn validate_provider_key(value: &str) -> Result<(), CustomOAuth2LoginRepositoryError> {
    if value.len() <= "custom_".len()
        || value.len() > 32
        || !value.starts_with("custom_")
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(CustomOAuth2LoginRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_configuration_version(value: i64) -> Result<(), CustomOAuth2LoginRepositoryError> {
    if value < 1 {
        Err(CustomOAuth2LoginRepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_subject(value: &str) -> Result<(), CustomOAuth2LoginRepositoryError> {
    if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        Err(CustomOAuth2LoginRepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_username(value: &str) -> Result<(), CustomOAuth2LoginRepositoryError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Err(CustomOAuth2LoginRepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_digest(value: &str) -> Result<(), CustomOAuth2LoginRepositoryError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(CustomOAuth2LoginRepositoryError::InvalidInput)
    }
}

fn timestamp(value: u64) -> Result<TimeDateTimeWithTimeZone, CustomOAuth2LoginRepositoryError> {
    let value = i64::try_from(value).map_err(|_| CustomOAuth2LoginRepositoryError::InvalidInput)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| CustomOAuth2LoginRepositoryError::InvalidInput)
}

fn map_user_error(error: crate::AdminUserRepositoryError) -> CustomOAuth2LoginRepositoryError {
    match error {
        crate::AdminUserRepositoryError::Conflict => CustomOAuth2LoginRepositoryError::Conflict,
        crate::AdminUserRepositoryError::InvalidReference => {
            CustomOAuth2LoginRepositoryError::Rejected
        }
        crate::AdminUserRepositoryError::Entropy
        | crate::AdminUserRepositoryError::Query
        | crate::AdminUserRepositoryError::Timeout
        | crate::AdminUserRepositoryError::Invariant => {
            internal(CustomOAuth2LoginRepositoryError::Invariant)
        }
    }
}

fn map_write_error(error: sea_orm::DbErr) -> CustomOAuth2LoginRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        CustomOAuth2LoginRepositoryError::Conflict
    } else {
        query("custom_oauth2_write")
    }
}

fn query(operation: &'static str) -> CustomOAuth2LoginRepositoryError {
    tracing::error!(
        target: "af_db::custom_oauth2_login",
        error_kind = operation,
        "自定义 OAuth2 登录数据库操作失败"
    );
    CustomOAuth2LoginRepositoryError::Query
}

fn internal(error: CustomOAuth2LoginRepositoryError) -> CustomOAuth2LoginRepositoryError {
    tracing::error!(
        target: "af_db::custom_oauth2_login",
        error_kind = ?error,
        "自定义 OAuth2 登录持久化状态无效"
    );
    error
}
