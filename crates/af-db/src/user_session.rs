use std::{fmt, time::Duration};

use af_domain::UserId;
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordVerifier},
};
use sea_orm::{
    ConnectionTrait, DbErr, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Query, SelectStatement},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use zeroize::Zeroizing;

use crate::{DatabasePool, entity::users};

const DUMMY_PASSWORD_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg";

/// 登录查询完成后的用户状态；角色仍由管理域转换为稳定会话角色。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserSessionLookupOutcome {
    /// 用户名、密码、状态和软删除均已通过校验。
    Authenticated { user_id: UserId, role: i16 },
    /// 用户不存在、密码错误、禁用、删除或没有密码。
    Rejected,
}

/// 会话回查完成后的用户状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserSessionLookupByIdOutcome {
    /// 用户仍存在、启用且角色值有效。
    Authenticated { user_id: UserId, role: i16 },
    /// 用户不存在、禁用或已软删除。
    Rejected,
}

/// 用户会话仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserSessionRepositoryConfigError {
    /// 零超时无法形成有效的数据库查询截止时间。
    #[error("用户会话查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 用户会话仓储内部错误；不保留用户名、密码或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserSessionRepositoryError {
    /// 获取连接或执行查询失败。
    #[error("用户会话数据库查询失败")]
    Query,
    /// 查询超过配置的硬截止时间。
    #[error("用户会话数据库查询超时")]
    Timeout,
    /// 持久化结果违反用户身份不变量。
    #[error("用户会话持久化状态损坏")]
    Invariant,
}

/// 按需查询用户状态并在数据库边界执行密码校验。
#[derive(Clone)]
pub struct UserSessionRepository {
    pool: DatabasePool,
    lookup_timeout: Duration,
}

impl UserSessionRepository {
    /// 使用共享数据库连接池和单次查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, UserSessionRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(UserSessionRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按精确用户名完成登录校验；不存在用户也执行固定 dummy 哈希校验。
    pub async fn login(
        &self,
        username: &str,
        password: &[u8],
    ) -> Result<UserSessionLookupOutcome, UserSessionRepositoryError> {
        let mut rows = match timeout(self.lookup_timeout, self.login_rows(username)).await {
            Ok(result) => result?,
            Err(_) => return Err(record_internal_error(UserSessionRepositoryError::Timeout)),
        };
        let row = match rows.len() {
            0 => {
                // 固定 dummy 校验用于减弱用户名枚举的时序差异。
                verify_password(None, password).await?;
                return Ok(UserSessionLookupOutcome::Rejected);
            }
            1 => rows
                .pop()
                .ok_or_else(|| record_internal_error(UserSessionRepositoryError::Invariant))?,
            _ => return Err(record_internal_error(UserSessionRepositoryError::Invariant)),
        };
        let row = LoginUserRow::try_from_query_result(&row)
            .map_err(|_| record_internal_error(UserSessionRepositoryError::Invariant))?;
        let password_matches = verify_password(row.password_hash, password).await?;
        let user_id = UserId::new(row.user_id)
            .map_err(|_| record_internal_error(UserSessionRepositoryError::Invariant))?;
        let role = valid_role(row.role)?;
        let status = valid_status(row.status)?;
        if !password_matches || !status || row.deleted_at.is_some() {
            return Ok(UserSessionLookupOutcome::Rejected);
        }
        Ok(UserSessionLookupOutcome::Authenticated { user_id, role })
    }

    /// 按 JWT 中的稳定用户 ID 回查当前状态，使禁用和软删除立即撤销现有会话。
    pub async fn lookup_by_id(
        &self,
        user_id: UserId,
    ) -> Result<UserSessionLookupByIdOutcome, UserSessionRepositoryError> {
        match timeout(self.lookup_timeout, self.lookup_by_id_inner(user_id)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserSessionRepositoryError::Timeout)),
        }
    }

    async fn login_rows(
        &self,
        username: &str,
    ) -> Result<Vec<QueryResult>, UserSessionRepositoryError> {
        let connection = self.pool.connection();
        let backend = connection.get_database_backend();
        let statement = backend.build(&login_query(username));
        connection
            .query_all(statement)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(UserSessionRepositoryError::Query))
    }

    async fn lookup_by_id_inner(
        &self,
        user_id: UserId,
    ) -> Result<UserSessionLookupByIdOutcome, UserSessionRepositoryError> {
        let connection = self.pool.connection();
        let backend = connection.get_database_backend();
        let statement = backend.build(&session_query(user_id));
        let mut rows = connection
            .query_all(statement)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(UserSessionRepositoryError::Query))?;
        let row = match rows.len() {
            0 => return Ok(UserSessionLookupByIdOutcome::Rejected),
            1 => rows
                .pop()
                .ok_or_else(|| record_internal_error(UserSessionRepositoryError::Invariant))?,
            _ => return Err(record_internal_error(UserSessionRepositoryError::Invariant)),
        };
        let row = SessionUserRow::try_from_query_result(&row)
            .map_err(|_| record_internal_error(UserSessionRepositoryError::Invariant))?;
        let id = UserId::new(row.user_id)
            .map_err(|_| record_internal_error(UserSessionRepositoryError::Invariant))?;
        let role = valid_role(row.role)?;
        let status = valid_status(row.status)?;
        if !status || row.deleted_at.is_some() {
            return Ok(UserSessionLookupByIdOutcome::Rejected);
        }
        Ok(UserSessionLookupByIdOutcome::Authenticated { user_id: id, role })
    }
}

impl fmt::Debug for UserSessionRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSessionRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

fn login_query(username: &str) -> SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((users::Entity, users::Column::Id)),
            Alias::new("user_id"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Role)),
            Alias::new("user_role"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Status)),
            Alias::new("user_status"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::DeletedAt)),
            Alias::new("user_deleted_at"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::PasswordHash)),
            Alias::new("user_password_hash"),
        )
        .from(users::Entity)
        .and_where(Expr::col((users::Entity, users::Column::Username)).eq(username))
        // 软删除用户名可被重新使用，登录查询只能命中当前有效身份行。
        .and_where(Expr::col((users::Entity, users::Column::DeletedAt)).is_null())
        .limit(2)
        .to_owned()
}

fn session_query(user_id: UserId) -> SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((users::Entity, users::Column::Id)),
            Alias::new("user_id"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Role)),
            Alias::new("user_role"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Status)),
            Alias::new("user_status"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::DeletedAt)),
            Alias::new("user_deleted_at"),
        )
        .from(users::Entity)
        .and_where(Expr::col((users::Entity, users::Column::Id)).eq(user_id.get()))
        .limit(2)
        .to_owned()
}

struct LoginUserRow {
    user_id: i64,
    role: i16,
    status: i16,
    deleted_at: Option<TimeDateTimeWithTimeZone>,
    password_hash: Option<crate::entity::PasswordHash>,
}

impl LoginUserRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            user_id: result.try_get("", "user_id")?,
            role: result.try_get("", "user_role")?,
            status: result.try_get("", "user_status")?,
            deleted_at: result.try_get("", "user_deleted_at")?,
            password_hash: result.try_get("", "user_password_hash")?,
        })
    }
}

struct SessionUserRow {
    user_id: i64,
    role: i16,
    status: i16,
    deleted_at: Option<TimeDateTimeWithTimeZone>,
}

impl SessionUserRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            user_id: result.try_get("", "user_id")?,
            role: result.try_get("", "user_role")?,
            status: result.try_get("", "user_status")?,
            deleted_at: result.try_get("", "user_deleted_at")?,
        })
    }
}

fn valid_role(role: i16) -> Result<i16, UserSessionRepositoryError> {
    match role {
        0 | 1 => Ok(role),
        _ => Err(record_internal_error(UserSessionRepositoryError::Invariant)),
    }
}

fn valid_status(status: i16) -> Result<bool, UserSessionRepositoryError> {
    match status {
        1 => Ok(true),
        2 => Ok(false),
        _ => Err(record_internal_error(UserSessionRepositoryError::Invariant)),
    }
}

fn verify_dummy_password(password: &[u8]) {
    if let Ok(hash) = PasswordHash::new(DUMMY_PASSWORD_HASH) {
        let _ = Argon2::default().verify_password(password, &hash);
    }
}

async fn verify_password(
    password_hash: Option<crate::entity::PasswordHash>,
    password: &[u8],
) -> Result<bool, UserSessionRepositoryError> {
    let password = Zeroizing::new(password.to_vec());
    tokio::task::spawn_blocking(move || match password_hash {
        Some(hash) => hash.verify(&password),
        None => {
            verify_dummy_password(&password);
            false
        }
    })
    .await
    .map_err(|_| record_internal_error(UserSessionRepositoryError::Invariant))
}

fn record_internal_error(error: UserSessionRepositoryError) -> UserSessionRepositoryError {
    let error_kind = match error {
        UserSessionRepositoryError::Query => "user_session_query",
        UserSessionRepositoryError::Timeout => "user_session_timeout",
        UserSessionRepositoryError::Invariant => "user_session_invariant",
    };
    tracing::error!(target: "af_db::user_session", error_kind, "用户会话仓储发生内部错误");
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_and_status_values_fail_closed() {
        assert_eq!(valid_role(0), Ok(0));
        assert_eq!(valid_role(1), Ok(1));
        assert_eq!(valid_role(2), Err(UserSessionRepositoryError::Invariant));
        assert_eq!(valid_status(1), Ok(true));
        assert_eq!(valid_status(2), Ok(false));
        assert_eq!(valid_status(3), Err(UserSessionRepositoryError::Invariant));
    }
}
