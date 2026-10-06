use std::{fmt, time::Duration};

use af_domain::UserId;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, QueryFilter, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, Query, SelectStatement},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use zeroize::Zeroizing;

use crate::{
    AuthChallengeConsume, AuthChallengeConsumeOutcome, AuthChallengePurpose,
    AuthChallengeRepositoryError, DatabasePool,
    auth_challenge::consume_in_transaction,
    entity::users,
    identity_secret::{IdentitySecretError, hash_password},
};

/// 按邮箱查找密码重置目标时的非敏感结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PasswordResetTargetOutcome {
    /// 邮箱对应一个可重置的启用用户。
    Found(UserId),
    /// 邮箱不存在、用户无密码、已禁用或已软删除。
    NotFound,
}

/// 密码重置事务的闭合结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PasswordResetOutcome {
    /// challenge 已消费、密码已更新且会话版本已递增。
    Updated(UserId),
    /// challenge 无效或目标用户已经不可重置。
    Rejected,
}

/// 密码重置仓储配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PasswordResetRepositoryConfigError {
    /// 零超时不能形成数据库操作截止时间。
    #[error("密码重置数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 密码重置仓储错误；不携带邮箱、令牌、密码或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PasswordResetRepositoryError {
    /// 查询或事务执行失败。
    #[error("密码重置数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("密码重置数据库操作超时")]
    Timeout,
    /// 持久化状态违反身份或 challenge 不变量。
    #[error("密码重置持久化状态损坏")]
    Invariant,
    /// 随机盐或 Argon2id 生成失败。
    #[error("密码重置密钥材料生成失败")]
    Entropy,
}

/// 负责密码重置目标查找与 challenge/密码原子消费的仓储。
#[derive(Clone)]
pub struct PasswordResetRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl PasswordResetRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, PasswordResetRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(PasswordResetRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 只返回可重置用户的稳定 ID，调用方不得把结果映射为公开账户存在性。
    pub async fn find_target(
        &self,
        email: &str,
    ) -> Result<PasswordResetTargetOutcome, PasswordResetRepositoryError> {
        match timeout(self.operation_timeout, self.find_target_inner(email)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(PasswordResetRepositoryError::Timeout)),
        }
    }

    /// 在同一事务中消费 challenge、更新 Argon2id 密码并递增会话版本。
    pub async fn reset_password(
        &self,
        challenge: AuthChallengeConsume,
        password: Zeroizing<String>,
    ) -> Result<PasswordResetOutcome, PasswordResetRepositoryError> {
        if challenge.purpose() != AuthChallengePurpose::PasswordReset {
            return Err(record_internal_error(
                PasswordResetRepositoryError::Invariant,
            ));
        }
        let password_hash = hash_password(&password).map_err(map_identity_secret_error)?;
        match timeout(
            self.operation_timeout,
            self.reset_password_inner(challenge, password_hash),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(PasswordResetRepositoryError::Timeout)),
        }
    }

    async fn find_target_inner(
        &self,
        email: &str,
    ) -> Result<PasswordResetTargetOutcome, PasswordResetRepositoryError> {
        let statement = self
            .pool
            .connection()
            .get_database_backend()
            .build(&target_query(email));
        let rows = self
            .pool
            .connection()
            .query_all(statement)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasswordResetRepositoryError::Query))?;
        match rows.len() {
            0 => Ok(PasswordResetTargetOutcome::NotFound),
            1 => {
                let id: i64 = rows[0]
                    .try_get("", "user_id")
                    .map_err(|_| record_internal_error(PasswordResetRepositoryError::Invariant))?;
                let id = UserId::new(id)
                    .map_err(|_| record_internal_error(PasswordResetRepositoryError::Invariant))?;
                Ok(PasswordResetTargetOutcome::Found(id))
            }
            _ => Err(record_internal_error(
                PasswordResetRepositoryError::Invariant,
            )),
        }
    }

    async fn reset_password_inner(
        &self,
        challenge: AuthChallengeConsume,
        password_hash: crate::entity::PasswordHash,
    ) -> Result<PasswordResetOutcome, PasswordResetRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasswordResetRepositoryError::Query))?;
        let consumption = match consume_in_transaction(&transaction, &challenge)
            .await
            .map_err(map_auth_challenge_error)?
        {
            AuthChallengeConsumeOutcome::Rejected => {
                commit_transaction(transaction).await?;
                return Ok(PasswordResetOutcome::Rejected);
            }
            AuthChallengeConsumeOutcome::Consumed(consumption) => consumption,
        };
        let Some(user_id) = consumption.target_user_id() else {
            return Err(record_internal_error(
                PasswordResetRepositoryError::Invariant,
            ));
        };

        let result = users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id.get()))
            .filter(users::Column::Status.eq(1_i16))
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::SessionVersion.lt(i64::MAX))
            .col_expr(
                users::Column::PasswordHash,
                Expr::value(Some(password_hash)),
            )
            .col_expr(
                users::Column::SessionVersion,
                Expr::col(users::Column::SessionVersion).add(1_i64),
            )
            .col_expr(
                users::Column::UpdatedAt,
                Expr::value(TimeDateTimeWithTimeZone::now_utc()),
            )
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasswordResetRepositoryError::Query))?;
        if result.rows_affected == 0 {
            commit_transaction(transaction).await?;
            return Ok(PasswordResetOutcome::Rejected);
        }
        if result.rows_affected != 1 {
            return Err(record_internal_error(
                PasswordResetRepositoryError::Invariant,
            ));
        }
        commit_transaction(transaction).await?;
        Ok(PasswordResetOutcome::Updated(user_id))
    }
}

impl fmt::Debug for PasswordResetRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasswordResetRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

fn target_query(email: &str) -> SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((users::Entity, users::Column::Id)),
            sea_orm::sea_query::Alias::new("user_id"),
        )
        .from(users::Entity)
        .and_where(Expr::col((users::Entity, users::Column::Email)).eq(email))
        .and_where(Expr::col((users::Entity, users::Column::Status)).eq(1_i16))
        .and_where(Expr::col((users::Entity, users::Column::PasswordHash)).is_not_null())
        .and_where(Expr::col((users::Entity, users::Column::DeletedAt)).is_null())
        .limit(2)
        .to_owned()
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), PasswordResetRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(PasswordResetRepositoryError::Query))
}

fn map_identity_secret_error(error: IdentitySecretError) -> PasswordResetRepositoryError {
    match error {
        IdentitySecretError::Entropy => PasswordResetRepositoryError::Entropy,
        IdentitySecretError::InvalidHash => {
            record_internal_error(PasswordResetRepositoryError::Invariant)
        }
    }
}

fn map_auth_challenge_error(error: AuthChallengeRepositoryError) -> PasswordResetRepositoryError {
    match error {
        AuthChallengeRepositoryError::Query => PasswordResetRepositoryError::Query,
        AuthChallengeRepositoryError::Timeout => PasswordResetRepositoryError::Timeout,
        AuthChallengeRepositoryError::Invariant => PasswordResetRepositoryError::Invariant,
    }
}

fn record_internal_error(error: PasswordResetRepositoryError) -> PasswordResetRepositoryError {
    let error_kind = match error {
        PasswordResetRepositoryError::Query => "password_reset_query",
        PasswordResetRepositoryError::Timeout => "password_reset_timeout",
        PasswordResetRepositoryError::Invariant => "password_reset_invariant",
        PasswordResetRepositoryError::Entropy => "password_reset_entropy",
    };
    tracing::error!(target: "af_db::password_reset", error_kind, "密码重置仓储发生内部错误");
    error
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;
    use sea_orm::{DbBackend, EntityTrait};

    use crate::entity::users;
    use crate::{
        AuthChallengeIssue, DatabaseOptions, InitialSetupOutcome, InitialSetupRecord,
        InitialSetupRepository, MigrationOptions, UserSessionLookupOutcome, UserSessionRepository,
    };

    #[test]
    fn target_query_excludes_disabled_deleted_and_passwordless_users() {
        let sql = DbBackend::Postgres
            .build(&target_query("user@example.com"))
            .to_string();
        assert!(sql.contains("email"), "{sql}");
        assert!(sql.contains("status"), "{sql}");
        assert!(sql.contains("password_hash"), "{sql}");
        assert!(sql.contains("deleted_at"), "{sql}");
        assert!(sql.contains("LIMIT 2"), "{sql}");
    }

    #[tokio::test]
    async fn reset_consumes_challenge_updates_password_and_revokes_sessions()
    -> Result<(), Box<dyn Error>> {
        let pool = crate::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let InitialSetupOutcome::Initialized { user_id } =
            InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
                .initialize(InitialSetupRecord::new(
                    "owner".to_owned(),
                    "old secure password".to_owned(),
                ))
                .await?
        else {
            panic!("测试数据库必须完成首次安装");
        };
        let repository = PasswordResetRepository::new(pool.clone(), Duration::from_secs(5))?;
        let challenge = AuthChallengeIssue::new(
            AuthChallengePurpose::PasswordReset,
            [0x11; 32],
            [0x22; 32],
            Some(user_id),
            1_000,
            600,
            60,
            3,
        )?;
        let challenge_repository =
            crate::AuthChallengeRepository::new(pool.clone(), Duration::from_secs(5))?;
        challenge_repository.issue(challenge).await?;
        let consume = AuthChallengeConsume::new(
            AuthChallengePurpose::PasswordReset,
            [0x11; 32],
            [0x22; 32],
            1_001,
        )?;

        assert_eq!(
            repository
                .reset_password(consume, Zeroizing::new("new secure password".to_owned()))
                .await?,
            PasswordResetOutcome::Updated(user_id)
        );
        let user = users::Entity::find_by_id(user_id.get())
            .one(pool.connection())
            .await?
            .expect("重置后用户必须存在");
        assert_eq!(user.session_version, 2);

        let sessions = UserSessionRepository::new(pool.clone(), Duration::from_secs(5))?;
        assert_eq!(
            sessions.login("owner", b"old secure password").await?,
            UserSessionLookupOutcome::Rejected
        );
        assert!(matches!(
            sessions.login("owner", b"new secure password").await?,
            UserSessionLookupOutcome::Authenticated { user_id: actual, .. }
                if actual == user_id
        ));
        let replay = AuthChallengeConsume::new(
            AuthChallengePurpose::PasswordReset,
            [0x11; 32],
            [0x22; 32],
            1_001,
        )?;
        assert_eq!(
            repository
                .reset_password(replay, Zeroizing::new("another password".to_owned()))
                .await?,
            PasswordResetOutcome::Rejected
        );

        pool.close().await?;
        Ok(())
    }
}
