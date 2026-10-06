use std::{fmt, time::Duration};

use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbErr, EntityTrait, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, OnConflict, Query, UpdateStatement},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AuthChallengePurpose, DatabasePool,
    entity::{AuthChallengeHash, auth_challenge_rate_limits},
};

/// 单个固定窗口允许的最大发送尝试次数。
pub const MAX_AUTH_CHALLENGE_RATE_LIMIT_ATTEMPTS: u32 = 100;
/// 认证挑战发送限流允许的最短固定窗口。
pub const MIN_AUTH_CHALLENGE_RATE_LIMIT_WINDOW_SECONDS: u64 = 60;
/// 认证挑战发送限流允许的最长固定窗口。
pub const MAX_AUTH_CHALLENGE_RATE_LIMIT_WINDOW_SECONDS: u64 = 86_400;

const SUBJECT_SCOPE: i16 = 1;
const CLIENT_IP_SCOPE: i16 = 2;

/// 已完成带密钥派生的认证挑战发送限流命令。
pub struct AuthChallengeRateLimitClaim {
    purpose: AuthChallengePurpose,
    subject_fingerprint: [u8; 32],
    client_fingerprint: [u8; 32],
    attempted_at: u64,
    max_attempts: u32,
    window_seconds: u64,
}

impl AuthChallengeRateLimitClaim {
    /// 校验时间和固定窗口边界；主体与客户端指纹不会进入调试输出。
    pub fn new(
        purpose: AuthChallengePurpose,
        subject_fingerprint: [u8; 32],
        client_fingerprint: [u8; 32],
        attempted_at: u64,
        max_attempts: u32,
        window_seconds: u64,
    ) -> Result<Self, AuthChallengeRateLimitInputError> {
        if attempted_at > i64::MAX as u64 {
            return Err(AuthChallengeRateLimitInputError::InvalidTiming);
        }
        if !(1..=MAX_AUTH_CHALLENGE_RATE_LIMIT_ATTEMPTS).contains(&max_attempts) {
            return Err(AuthChallengeRateLimitInputError::InvalidAttempts);
        }
        if !(MIN_AUTH_CHALLENGE_RATE_LIMIT_WINDOW_SECONDS
            ..=MAX_AUTH_CHALLENGE_RATE_LIMIT_WINDOW_SECONDS)
            .contains(&window_seconds)
        {
            return Err(AuthChallengeRateLimitInputError::InvalidTiming);
        }
        Ok(Self {
            purpose,
            subject_fingerprint,
            client_fingerprint,
            attempted_at,
            max_attempts,
            window_seconds,
        })
    }
}

impl fmt::Debug for AuthChallengeRateLimitClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthChallengeRateLimitClaim(<redacted>)")
    }
}

/// 认证挑战发送限流命令的闭合输入错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AuthChallengeRateLimitInputError {
    /// 尝试次数超出持久化硬边界。
    #[error("认证挑战发送限流次数无效")]
    InvalidAttempts,
    /// 时间戳或固定窗口超出持久化硬边界。
    #[error("认证挑战发送限流时间边界无效")]
    InvalidTiming,
}

/// 主体与客户端 IP 双作用域的原子限流结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthChallengeRateLimitOutcome {
    /// 两个作用域都成功占用一次发送配额。
    Allowed,
    /// 至少一个作用域已耗尽当前固定窗口。
    RateLimited { retry_after_seconds: u64 },
}

/// 认证挑战发送限流仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AuthChallengeRateLimitRepositoryConfigError {
    /// 零超时无法形成有效数据库截止时间。
    #[error("认证挑战发送限流数据库超时必须大于零")]
    ZeroOperationTimeout,
}

/// 认证挑战发送限流仓储错误；不携带任何指纹或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AuthChallengeRateLimitRepositoryError {
    /// 查询、事务或写入失败。
    #[error("认证挑战发送限流数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("认证挑战发送限流数据库操作超时")]
    Timeout,
    /// 持久化状态违反固定窗口不变量。
    #[error("认证挑战发送限流持久化状态损坏")]
    Invariant,
}

/// 数据库支持的主体与客户端 IP 双作用域跨实例发送限流仓储。
#[derive(Clone)]
pub struct AuthChallengeRateLimitRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl AuthChallengeRateLimitRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, AuthChallengeRateLimitRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(AuthChallengeRateLimitRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 在同一事务内同时占用主体与客户端 IP 的固定窗口配额。
    pub async fn claim(
        &self,
        record: AuthChallengeRateLimitClaim,
    ) -> Result<AuthChallengeRateLimitOutcome, AuthChallengeRateLimitRepositoryError> {
        match timeout(self.operation_timeout, self.claim_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                AuthChallengeRateLimitRepositoryError::Timeout,
            )),
        }
    }

    async fn claim_inner(
        &self,
        record: AuthChallengeRateLimitClaim,
    ) -> Result<AuthChallengeRateLimitOutcome, AuthChallengeRateLimitRepositoryError> {
        let window_started_at = record
            .attempted_at
            .checked_div(record.window_seconds)
            .and_then(|window| window.checked_mul(record.window_seconds))
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| {
                record_internal_error(AuthChallengeRateLimitRepositoryError::Invariant)
            })?;
        let attempted_at = i64::try_from(record.attempted_at)
            .map_err(|_| record_internal_error(AuthChallengeRateLimitRepositoryError::Invariant))?;
        let now = TimeDateTimeWithTimeZone::from_unix_timestamp(attempted_at)
            .map_err(|_| record_internal_error(AuthChallengeRateLimitRepositoryError::Invariant))?;
        let max_attempts = i32::try_from(record.max_attempts)
            .map_err(|_| record_internal_error(AuthChallengeRateLimitRepositoryError::Invariant))?;
        let purpose = record.purpose.to_database();
        let transaction = begin_transaction(&self.pool).await?;

        for (scope, fingerprint) in [
            (SUBJECT_SCOPE, record.subject_fingerprint),
            (CLIENT_IP_SCOPE, record.client_fingerprint),
        ] {
            let fingerprint = AuthChallengeHash::from_bytes(fingerprint);
            auth_challenge_rate_limits::Entity::insert(auth_challenge_rate_limits::ActiveModel {
                id: sea_orm::NotSet,
                purpose: sea_orm::Set(purpose),
                scope: sea_orm::Set(scope),
                fingerprint: sea_orm::Set(fingerprint.clone()),
                window_started_at: sea_orm::Set(window_started_at),
                attempts: sea_orm::Set(0),
                created_at: sea_orm::Set(now),
                updated_at: sea_orm::Set(now),
            })
            .on_conflict(rate_limit_on_conflict())
            .exec_without_returning(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|error| record_query_error("auth_challenge_rate_limit_insert", error))?;

            let update = claim_update(
                purpose,
                scope,
                fingerprint,
                window_started_at,
                max_attempts,
                now,
            );
            let result = transaction
                .execute(transaction.get_database_backend().build(&update))
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|error| record_query_error("auth_challenge_rate_limit_update", error))?;
            match result.rows_affected() {
                1 => {}
                0 => {
                    rollback_transaction(transaction).await?;
                    let window_end = u64::try_from(window_started_at)
                        .ok()
                        .and_then(|start| start.checked_add(record.window_seconds))
                        .ok_or_else(|| {
                            record_internal_error(AuthChallengeRateLimitRepositoryError::Invariant)
                        })?;
                    return Ok(AuthChallengeRateLimitOutcome::RateLimited {
                        retry_after_seconds: window_end.saturating_sub(record.attempted_at).max(1),
                    });
                }
                _ => {
                    return Err(record_internal_error(
                        AuthChallengeRateLimitRepositoryError::Invariant,
                    ));
                }
            }
        }

        commit_transaction(transaction).await?;
        Ok(AuthChallengeRateLimitOutcome::Allowed)
    }
}

impl fmt::Debug for AuthChallengeRateLimitRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthChallengeRateLimitRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

fn rate_limit_on_conflict() -> OnConflict {
    OnConflict::columns([
        auth_challenge_rate_limits::Column::Purpose,
        auth_challenge_rate_limits::Column::Scope,
        auth_challenge_rate_limits::Column::Fingerprint,
    ])
    .do_nothing_on([
        auth_challenge_rate_limits::Column::Purpose,
        auth_challenge_rate_limits::Column::Scope,
        auth_challenge_rate_limits::Column::Fingerprint,
    ])
    .to_owned()
}

fn claim_update(
    purpose: i16,
    scope: i16,
    fingerprint: AuthChallengeHash,
    window_started_at: i64,
    max_attempts: i32,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    Query::update()
        .table(auth_challenge_rate_limits::Entity)
        // MySQL 按从左到右执行赋值，因此先基于旧窗口计算次数，再覆盖窗口起点。
        .value(
            auth_challenge_rate_limits::Column::Attempts,
            Expr::case(
                Expr::col(auth_challenge_rate_limits::Column::WindowStartedAt)
                    .ne(window_started_at),
                1_i32,
            )
            .finally(Expr::col(auth_challenge_rate_limits::Column::Attempts).add(1_i32)),
        )
        .value(
            auth_challenge_rate_limits::Column::WindowStartedAt,
            window_started_at,
        )
        .value(auth_challenge_rate_limits::Column::UpdatedAt, now)
        .and_where(Expr::col(auth_challenge_rate_limits::Column::Purpose).eq(purpose))
        .and_where(Expr::col(auth_challenge_rate_limits::Column::Scope).eq(scope))
        .and_where(Expr::col(auth_challenge_rate_limits::Column::Fingerprint).eq(fingerprint))
        .and_where(
            Expr::col(auth_challenge_rate_limits::Column::WindowStartedAt)
                .ne(window_started_at)
                .or(Expr::col(auth_challenge_rate_limits::Column::Attempts).lt(max_attempts)),
        )
        .to_owned()
}

async fn begin_transaction(
    pool: &DatabasePool,
) -> Result<DatabaseTransaction, AuthChallengeRateLimitRepositoryError> {
    pool.connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| record_query_error("auth_challenge_rate_limit_begin", error))
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), AuthChallengeRateLimitRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| record_query_error("auth_challenge_rate_limit_commit", error))
}

async fn rollback_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), AuthChallengeRateLimitRepositoryError> {
    transaction
        .rollback()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| record_query_error("auth_challenge_rate_limit_rollback", error))
}

fn record_internal_error(
    error: AuthChallengeRateLimitRepositoryError,
) -> AuthChallengeRateLimitRepositoryError {
    tracing::error!(
        error_kind = match error {
            AuthChallengeRateLimitRepositoryError::Query => "auth_challenge_rate_limit_query",
            AuthChallengeRateLimitRepositoryError::Timeout => "auth_challenge_rate_limit_timeout",
            AuthChallengeRateLimitRepositoryError::Invariant => {
                "auth_challenge_rate_limit_invariant"
            }
        },
        "认证挑战发送限流仓储操作失败"
    );
    error
}

fn record_query_error(
    operation: &'static str,
    _error: DbErr,
) -> AuthChallengeRateLimitRepositoryError {
    tracing::error!(
        error_kind = "auth_challenge_rate_limit_query",
        operation,
        "认证挑战发送限流数据库操作失败"
    );
    AuthChallengeRateLimitRepositoryError::Query
}

#[cfg(test)]
mod sql_tests {
    use sea_orm::sea_query::MysqlQueryBuilder;

    use super::*;

    #[test]
    fn mysql_insert_uses_supported_duplicate_key_noop() {
        let statement = Query::insert()
            .into_table(auth_challenge_rate_limits::Entity)
            .columns([
                auth_challenge_rate_limits::Column::Purpose,
                auth_challenge_rate_limits::Column::Scope,
                auth_challenge_rate_limits::Column::Fingerprint,
            ])
            .values_panic([1_i16.into(), 1_i16.into(), "11".repeat(32).into()])
            .on_conflict(rate_limit_on_conflict())
            .to_owned()
            .to_string(MysqlQueryBuilder);

        assert!(statement.contains("ON DUPLICATE KEY UPDATE"), "{statement}");
        assert!(!statement.contains("IGNORE"), "{statement}");
    }
}
