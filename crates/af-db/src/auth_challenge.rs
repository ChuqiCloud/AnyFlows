use std::{fmt, time::Duration};

use af_domain::UserId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, IntoActiveModel, QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType, OnConflict},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{AuthChallengeHash, auth_challenges},
};

/// 认证挑战允许的最短有效期。
pub const MIN_AUTH_CHALLENGE_TTL_SECONDS: u64 = 60;
/// 认证挑战允许的最长有效期。
pub const MAX_AUTH_CHALLENGE_TTL_SECONDS: u64 = 86_400;
/// 同一主体允许的最短重发间隔。
pub const MIN_AUTH_CHALLENGE_RESEND_COOLDOWN_SECONDS: u64 = 1;
/// 同一主体允许的最长重发间隔。
pub const MAX_AUTH_CHALLENGE_RESEND_COOLDOWN_SECONDS: u64 = 3_600;
/// 单个挑战允许的最大错误次数。
pub const MAX_AUTH_CHALLENGE_ATTEMPTS: u32 = 10;

const REGISTRATION_EMAIL_PURPOSE: i16 = 1;
const PASSWORD_RESET_PURPOSE: i16 = 2;
const PASSKEY_AUTHENTICATION_PURPOSE: i16 = 3;
const EMAIL_BINDING_PURPOSE: i16 = 4;

/// 认证挑战的闭合用途；不同用途必须使用独立摘要域。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthChallengePurpose {
    /// 公开注册前验证邮箱控制权。
    RegistrationEmail,
    /// 已存在用户通过邮件重置密码。
    PasswordReset,
    /// Passkey 公开登录的主体与客户端限流。
    PasskeyAuthentication,
    /// 已登录用户绑定或更换邮箱前验证新邮箱控制权。
    EmailBinding,
}

impl AuthChallengePurpose {
    pub(crate) const fn to_database(self) -> i16 {
        match self {
            Self::RegistrationEmail => REGISTRATION_EMAIL_PURPOSE,
            Self::PasswordReset => PASSWORD_RESET_PURPOSE,
            Self::PasskeyAuthentication => PASSKEY_AUTHENTICATION_PURPOSE,
            Self::EmailBinding => EMAIL_BINDING_PURPOSE,
        }
    }

    fn from_database(value: i16) -> Result<Self, AuthChallengeRepositoryError> {
        match value {
            REGISTRATION_EMAIL_PURPOSE => Ok(Self::RegistrationEmail),
            PASSWORD_RESET_PURPOSE => Ok(Self::PasswordReset),
            EMAIL_BINDING_PURPOSE => Ok(Self::EmailBinding),
            _ => Err(record_internal_error(
                AuthChallengeRepositoryError::Invariant,
            )),
        }
    }
}

/// 签发认证挑战前已经完成带密钥摘要的持久化命令。
pub struct AuthChallengeIssue {
    purpose: AuthChallengePurpose,
    subject_fingerprint: [u8; 32],
    secret_digest: [u8; 32],
    target_user_id: Option<UserId>,
    issued_at: u64,
    expires_at: u64,
    next_send_at: u64,
    max_attempts: u32,
}

impl AuthChallengeIssue {
    /// 校验用途、TTL、重发间隔和错误次数，并计算不可溢出的时间边界。
    #[allow(clippy::too_many_arguments, reason = "字段与认证挑战签发契约一一对应")]
    pub fn new(
        purpose: AuthChallengePurpose,
        subject_fingerprint: [u8; 32],
        secret_digest: [u8; 32],
        target_user_id: Option<UserId>,
        issued_at: u64,
        ttl_seconds: u64,
        resend_cooldown_seconds: u64,
        max_attempts: u32,
    ) -> Result<Self, AuthChallengeInputError> {
        if !matches!(
            (purpose, target_user_id),
            (AuthChallengePurpose::RegistrationEmail, None)
                | (AuthChallengePurpose::PasswordReset, Some(_))
                | (AuthChallengePurpose::EmailBinding, Some(_))
        ) {
            return Err(AuthChallengeInputError::InvalidTarget);
        }
        if !(MIN_AUTH_CHALLENGE_TTL_SECONDS..=MAX_AUTH_CHALLENGE_TTL_SECONDS).contains(&ttl_seconds)
            || !(MIN_AUTH_CHALLENGE_RESEND_COOLDOWN_SECONDS
                ..=MAX_AUTH_CHALLENGE_RESEND_COOLDOWN_SECONDS)
                .contains(&resend_cooldown_seconds)
            || resend_cooldown_seconds > ttl_seconds
            || issued_at > i64::MAX as u64
        {
            return Err(AuthChallengeInputError::InvalidTiming);
        }
        if !(1..=MAX_AUTH_CHALLENGE_ATTEMPTS).contains(&max_attempts) {
            return Err(AuthChallengeInputError::InvalidAttempts);
        }
        let expires_at = issued_at
            .checked_add(ttl_seconds)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or(AuthChallengeInputError::InvalidTiming)?;
        let next_send_at = issued_at
            .checked_add(resend_cooldown_seconds)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or(AuthChallengeInputError::InvalidTiming)?;
        Ok(Self {
            purpose,
            subject_fingerprint,
            secret_digest,
            target_user_id,
            issued_at,
            expires_at,
            next_send_at,
            max_attempts,
        })
    }
}

impl fmt::Debug for AuthChallengeIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthChallengeIssue(<redacted>)")
    }
}

/// 校验并消费挑战时提交的带密钥摘要命令。
pub struct AuthChallengeConsume {
    purpose: AuthChallengePurpose,
    subject_fingerprint: [u8; 32],
    secret_digest: [u8; 32],
    attempted_at: u64,
}

impl AuthChallengeConsume {
    /// 校验时间表示边界；公开层仍需把所有拒绝结果映射为统一语义。
    pub fn new(
        purpose: AuthChallengePurpose,
        subject_fingerprint: [u8; 32],
        secret_digest: [u8; 32],
        attempted_at: u64,
    ) -> Result<Self, AuthChallengeInputError> {
        if purpose == AuthChallengePurpose::PasskeyAuthentication {
            return Err(AuthChallengeInputError::InvalidTarget);
        }
        if attempted_at > i64::MAX as u64 {
            return Err(AuthChallengeInputError::InvalidTiming);
        }
        Ok(Self {
            purpose,
            subject_fingerprint,
            secret_digest,
            attempted_at,
        })
    }

    pub(crate) const fn purpose(&self) -> AuthChallengePurpose {
        self.purpose
    }
}

impl fmt::Debug for AuthChallengeConsume {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthChallengeConsume(<redacted>)")
    }
}

/// 新挑战已经持久化后的非敏感时间边界。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthChallengeIssued {
    version: i64,
    expires_at: u64,
    next_send_at: u64,
}

impl AuthChallengeIssued {
    /// 返回本次签发后的单调版本。
    #[must_use]
    pub const fn version(self) -> i64 {
        self.version
    }

    /// 返回挑战失效的 Unix 秒时间戳。
    #[must_use]
    pub const fn expires_at(self) -> u64 {
        self.expires_at
    }

    /// 返回同一主体下次允许重发的 Unix 秒时间戳。
    #[must_use]
    pub const fn next_send_at(self) -> u64 {
        self.next_send_at
    }
}

/// 原子签发或重发挑战后的闭合结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthChallengeIssueOutcome {
    /// 新摘要已经落库，旧挑战已原子失效。
    Issued(AuthChallengeIssued),
    /// 仍处于同一主体的发送冷却窗口。
    Cooldown { retry_after_seconds: u64 },
}

/// 挑战成功消费后业务事务需要的非敏感上下文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthChallengeConsumption {
    target_user_id: Option<UserId>,
    version: i64,
}

impl AuthChallengeConsumption {
    /// 返回密码重置绑定的用户；注册邮箱验证固定为空。
    #[must_use]
    pub const fn target_user_id(self) -> Option<UserId> {
        self.target_user_id
    }

    /// 返回成功消费后的单调版本。
    #[must_use]
    pub const fn version(self) -> i64 {
        self.version
    }
}

/// 挑战消费结果；所有失败原因有意合并，防止形成状态探针。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthChallengeConsumeOutcome {
    /// 当前挑战第一次成功消费。
    Consumed(AuthChallengeConsumption),
    /// 不存在、用途不符、摘要错误、过期、已消费或次数耗尽。
    Rejected,
}

/// 认证挑战命令构造错误，不携带主体或凭据内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AuthChallengeInputError {
    /// 用途与可选目标用户不一致。
    #[error("认证挑战目标无效")]
    InvalidTarget,
    /// TTL、冷却时间或时间戳超出硬边界。
    #[error("认证挑战时间边界无效")]
    InvalidTiming,
    /// 错误次数上限超出硬边界。
    #[error("认证挑战错误次数无效")]
    InvalidAttempts,
}

/// 认证挑战仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AuthChallengeRepositoryConfigError {
    /// 零超时无法形成有效数据库截止时间。
    #[error("认证挑战数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 认证挑战仓储错误；不携带主体、摘要或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AuthChallengeRepositoryError {
    /// 查询、事务或写入失败。
    #[error("认证挑战数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("认证挑战数据库操作超时")]
    Timeout,
    /// 持久化状态违反用途、时间或次数不变量。
    #[error("认证挑战持久化状态损坏")]
    Invariant,
}

/// 数据库支持的跨实例认证挑战仓储。
#[derive(Clone)]
pub struct AuthChallengeRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl AuthChallengeRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, AuthChallengeRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(AuthChallengeRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 原子签发挑战；同一用途与主体仍在冷却期时不替换旧摘要。
    pub async fn issue(
        &self,
        record: AuthChallengeIssue,
    ) -> Result<AuthChallengeIssueOutcome, AuthChallengeRepositoryError> {
        match timeout(self.operation_timeout, self.issue_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AuthChallengeRepositoryError::Timeout)),
        }
    }

    /// 原子累计一次校验并仅允许首个正确摘要消费成功。
    pub async fn consume(
        &self,
        record: AuthChallengeConsume,
    ) -> Result<AuthChallengeConsumeOutcome, AuthChallengeRepositoryError> {
        match timeout(self.operation_timeout, self.consume_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AuthChallengeRepositoryError::Timeout)),
        }
    }

    async fn issue_inner(
        &self,
        record: AuthChallengeIssue,
    ) -> Result<AuthChallengeIssueOutcome, AuthChallengeRepositoryError> {
        let transaction = begin_transaction(&self.pool).await?;
        let purpose = record.purpose.to_database();
        let subject_fingerprint = AuthChallengeHash::from_bytes(record.subject_fingerprint);
        let secret_digest = AuthChallengeHash::from_bytes(record.secret_digest);
        let issued_at = to_database_time(record.issued_at)?;
        let expires_at = to_database_time(record.expires_at)?;
        let next_send_at = to_database_time(record.next_send_at)?;
        let max_attempts = i32::try_from(record.max_attempts)
            .map_err(|_| record_internal_error(AuthChallengeRepositoryError::Invariant))?;
        let target_user_id = record.target_user_id.map(UserId::get);

        auth_challenges::Entity::insert(auth_challenges::ActiveModel {
            purpose: Set(purpose),
            subject_fingerprint: Set(subject_fingerprint.clone()),
            secret_digest: Set(secret_digest.clone()),
            target_user_id: Set(target_user_id),
            attempts: Set(0),
            max_attempts: Set(max_attempts),
            version: Set(1),
            issued_at: Set(issued_at),
            expires_at: Set(expires_at),
            next_send_at: Set(next_send_at),
            consumed_at: Set(None),
            ..Default::default()
        })
        .on_conflict(challenge_on_conflict())
        .exec_without_returning(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| record_query_error("auth_challenge_insert", error))?;

        let existing = lock_challenge(&transaction, purpose, &subject_fingerprint)
            .await?
            .ok_or_else(|| record_internal_error(AuthChallengeRepositoryError::Invariant))?;
        validate_persisted(&existing)?;
        if existing.version == 1
            && existing.target_user_id == target_user_id
            && existing.attempts == 0
            && existing.max_attempts == max_attempts
            && existing.issued_at == issued_at
            && existing.expires_at == expires_at
            && existing.next_send_at == next_send_at
            && existing.consumed_at.is_none()
            && existing.secret_digest.matches_bytes(&record.secret_digest)
        {
            commit_transaction(transaction).await?;
            return Ok(AuthChallengeIssueOutcome::Issued(AuthChallengeIssued {
                version: 1,
                expires_at: record.expires_at,
                next_send_at: record.next_send_at,
            }));
        }
        let existing_next_send_at = to_unix_seconds(existing.next_send_at)?;
        if record.issued_at < existing_next_send_at {
            commit_transaction(transaction).await?;
            return Ok(AuthChallengeIssueOutcome::Cooldown {
                retry_after_seconds: existing_next_send_at - record.issued_at,
            });
        }

        let version = existing
            .version
            .checked_add(1)
            .ok_or_else(|| record_internal_error(AuthChallengeRepositoryError::Invariant))?;
        let mut active = existing.into_active_model();
        active.secret_digest = Set(secret_digest);
        active.target_user_id = Set(target_user_id);
        active.attempts = Set(0);
        active.max_attempts = Set(max_attempts);
        active.version = Set(version);
        active.issued_at = Set(issued_at);
        active.expires_at = Set(expires_at);
        active.next_send_at = Set(next_send_at);
        active.consumed_at = Set(None);
        active
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|error| record_query_error("auth_challenge_reissue", error))?;
        commit_transaction(transaction).await?;
        Ok(AuthChallengeIssueOutcome::Issued(AuthChallengeIssued {
            version,
            expires_at: record.expires_at,
            next_send_at: record.next_send_at,
        }))
    }

    async fn consume_inner(
        &self,
        record: AuthChallengeConsume,
    ) -> Result<AuthChallengeConsumeOutcome, AuthChallengeRepositoryError> {
        let transaction = begin_transaction(&self.pool).await?;
        let outcome = consume_in_transaction(&transaction, &record).await?;
        commit_transaction(transaction).await?;
        Ok(outcome)
    }
}

/// 在调用方事务中累计校验并消费挑战，供身份变更与挑战消费形成同一原子边界。
pub(crate) async fn consume_in_transaction(
    transaction: &DatabaseTransaction,
    record: &AuthChallengeConsume,
) -> Result<AuthChallengeConsumeOutcome, AuthChallengeRepositoryError> {
    let purpose = record.purpose.to_database();
    let subject_fingerprint = AuthChallengeHash::from_bytes(record.subject_fingerprint);
    let Some(existing) = lock_challenge(transaction, purpose, &subject_fingerprint).await? else {
        return Ok(AuthChallengeConsumeOutcome::Rejected);
    };
    validate_persisted(&existing)?;
    let attempted_at = to_database_time(record.attempted_at)?;
    if attempted_at < existing.issued_at
        || attempted_at >= existing.expires_at
        || existing.consumed_at.is_some()
        || existing.attempts >= existing.max_attempts
    {
        return Ok(AuthChallengeConsumeOutcome::Rejected);
    }

    let matched = existing.secret_digest.matches_bytes(&record.secret_digest);
    let attempts = existing
        .attempts
        .checked_add(1)
        .ok_or_else(|| record_internal_error(AuthChallengeRepositoryError::Invariant))?;
    let version = existing
        .version
        .checked_add(1)
        .ok_or_else(|| record_internal_error(AuthChallengeRepositoryError::Invariant))?;
    let target_user_id = existing
        .target_user_id
        .map(UserId::new)
        .transpose()
        .map_err(|_| record_internal_error(AuthChallengeRepositoryError::Invariant))?;
    let mut active = existing.into_active_model();
    active.attempts = Set(attempts);
    active.version = Set(version);
    if matched {
        active.consumed_at = Set(Some(attempted_at));
    }
    active
        .update(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| record_query_error("auth_challenge_consume", error))?;

    if matched {
        Ok(AuthChallengeConsumeOutcome::Consumed(
            AuthChallengeConsumption {
                target_user_id,
                version,
            },
        ))
    } else {
        Ok(AuthChallengeConsumeOutcome::Rejected)
    }
}

impl fmt::Debug for AuthChallengeRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthChallengeRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

fn challenge_on_conflict() -> OnConflict {
    OnConflict::columns([
        auth_challenges::Column::Purpose,
        auth_challenges::Column::SubjectFingerprint,
    ])
    // SeaQuery 的无参数 do_nothing 会生成 MySQL 不支持的 ON DUPLICATE KEY IGNORE。
    .do_nothing_on([
        auth_challenges::Column::Purpose,
        auth_challenges::Column::SubjectFingerprint,
    ])
    .to_owned()
}

async fn lock_challenge(
    transaction: &DatabaseTransaction,
    purpose: i16,
    subject_fingerprint: &AuthChallengeHash,
) -> Result<Option<auth_challenges::Model>, AuthChallengeRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，通过无变化写入取得数据库写锁并串行化消费。
        auth_challenges::Entity::update_many()
            .filter(auth_challenges::Column::Purpose.eq(purpose))
            .filter(auth_challenges::Column::SubjectFingerprint.eq(subject_fingerprint.clone()))
            .col_expr(
                auth_challenges::Column::Version,
                Expr::col(auth_challenges::Column::Version).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|error| record_query_error("auth_challenge_lock", error))?;
    }

    let mut query = auth_challenges::Entity::find()
        .filter(auth_challenges::Column::Purpose.eq(purpose))
        .filter(auth_challenges::Column::SubjectFingerprint.eq(subject_fingerprint.clone()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| record_query_error("auth_challenge_read_for_update", error))
}

fn validate_persisted(
    model: &auth_challenges::Model,
) -> Result<AuthChallengePurpose, AuthChallengeRepositoryError> {
    let purpose = AuthChallengePurpose::from_database(model.purpose)?;
    let target_is_valid = matches!(
        (purpose, model.target_user_id),
        (AuthChallengePurpose::RegistrationEmail, None)
            | (AuthChallengePurpose::PasswordReset, Some(_))
            | (AuthChallengePurpose::EmailBinding, Some(_))
    );
    if !target_is_valid
        || !(0..=model.max_attempts).contains(&model.attempts)
        || !(1..=MAX_AUTH_CHALLENGE_ATTEMPTS as i32).contains(&model.max_attempts)
        || model.version < 1
        || model.expires_at <= model.issued_at
        || model.next_send_at < model.issued_at
        || model.next_send_at > model.expires_at
        || model
            .consumed_at
            .is_some_and(|consumed| consumed < model.issued_at || consumed >= model.expires_at)
    {
        return Err(record_internal_error(
            AuthChallengeRepositoryError::Invariant,
        ));
    }
    Ok(purpose)
}

fn to_database_time(
    timestamp: u64,
) -> Result<TimeDateTimeWithTimeZone, AuthChallengeRepositoryError> {
    let timestamp = i64::try_from(timestamp)
        .map_err(|_| record_internal_error(AuthChallengeRepositoryError::Invariant))?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(timestamp)
        .map_err(|_| record_internal_error(AuthChallengeRepositoryError::Invariant))
}

fn to_unix_seconds(
    timestamp: TimeDateTimeWithTimeZone,
) -> Result<u64, AuthChallengeRepositoryError> {
    u64::try_from(timestamp.unix_timestamp())
        .map_err(|_| record_internal_error(AuthChallengeRepositoryError::Invariant))
}

async fn begin_transaction(
    pool: &DatabasePool,
) -> Result<DatabaseTransaction, AuthChallengeRepositoryError> {
    pool.connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| record_query_error("auth_challenge_begin", error))
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), AuthChallengeRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| record_query_error("auth_challenge_commit", error))
}

fn record_internal_error(error: AuthChallengeRepositoryError) -> AuthChallengeRepositoryError {
    tracing::error!(
        error_kind = match error {
            AuthChallengeRepositoryError::Query => "auth_challenge_query",
            AuthChallengeRepositoryError::Timeout => "auth_challenge_timeout",
            AuthChallengeRepositoryError::Invariant => "auth_challenge_invariant",
        },
        "认证挑战仓储操作失败"
    );
    error
}

fn record_query_error(operation: &'static str, _error: DbErr) -> AuthChallengeRepositoryError {
    tracing::error!(
        error_kind = "auth_challenge_query",
        operation,
        "认证挑战仓储数据库操作失败"
    );
    AuthChallengeRepositoryError::Query
}

#[cfg(test)]
mod sql_tests {
    use sea_orm::sea_query::MysqlQueryBuilder;

    use super::*;

    #[test]
    fn mysql_insert_uses_supported_duplicate_key_noop() {
        let statement = sea_orm::sea_query::Query::insert()
            .into_table(auth_challenges::Entity)
            .columns([
                auth_challenges::Column::Purpose,
                auth_challenges::Column::SubjectFingerprint,
            ])
            .values_panic([REGISTRATION_EMAIL_PURPOSE.into(), "11".repeat(32).into()])
            .on_conflict(challenge_on_conflict())
            .to_owned()
            .to_string(MysqlQueryBuilder);

        assert!(statement.contains("ON DUPLICATE KEY UPDATE"), "{statement}");
        assert!(!statement.contains("IGNORE"), "{statement}");
    }

    #[test]
    fn mysql_lock_update_keeps_sensitive_values_bound() {
        let statement = sea_orm::sea_query::Query::update()
            .table(auth_challenges::Entity)
            .value(
                auth_challenges::Column::Version,
                Expr::col(auth_challenges::Column::Version),
            )
            .and_where(auth_challenges::Column::Purpose.eq(REGISTRATION_EMAIL_PURPOSE))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(statement.contains("`version` = `version`"), "{statement}");
    }
}
