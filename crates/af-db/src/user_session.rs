use std::{fmt, time::Duration};

use af_domain::{GroupId, UserId};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordVerifier},
};
use base64::Engine as _;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Query, SelectStatement},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use zeroize::Zeroizing;

use crate::{DatabasePool, entity::users};
use crate::{EncryptedCredentialEnvelope, entity::EncryptedJson};

const DUMMY_PASSWORD_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg";

/// 登录查询完成后的用户状态；角色仍由管理域转换为稳定会话角色。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserSessionLookupOutcome {
    /// 用户名、密码、状态和软删除均已通过校验。
    Authenticated {
        user_id: UserId,
        role: i16,
        session_version: i64,
        totp_secret: Option<EncryptedCredentialEnvelope>,
    },
    /// 用户不存在、密码错误、禁用、删除或没有密码。
    Rejected,
}

/// 会话回查完成后的用户状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserSessionLookupByIdOutcome {
    /// 用户仍存在、启用，且角色与默认分组值有效。
    Authenticated {
        user_id: UserId,
        role: i16,
        group_id: GroupId,
        session_version: i64,
        totp_enabled: bool,
    },
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
        let session_version = valid_session_version(row.session_version)?;
        if !password_matches || !status || row.deleted_at.is_some() {
            return Ok(UserSessionLookupOutcome::Rejected);
        }
        Ok(UserSessionLookupOutcome::Authenticated {
            user_id,
            role,
            session_version,
            totp_secret: row
                .totp_secret
                .map(envelope_from_json)
                .transpose()
                .map_err(|_| record_internal_error(UserSessionRepositoryError::Invariant))?,
        })
    }

    /// 以预期旧封套为并发边界，原子消费一个备份码后的新封套。
    pub async fn consume_totp_backup_code(
        &self,
        user_id: UserId,
        expected: EncryptedCredentialEnvelope,
        replacement: EncryptedCredentialEnvelope,
    ) -> Result<bool, UserSessionRepositoryError> {
        match timeout(
            self.lookup_timeout,
            self.consume_totp_backup_code_inner(user_id, expected, replacement),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserSessionRepositoryError::Timeout)),
        }
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
        let group_id = GroupId::new(row.default_group_id)
            .map_err(|_| record_internal_error(UserSessionRepositoryError::Invariant))?;
        let role = valid_role(row.role)?;
        let status = valid_status(row.status)?;
        let session_version = valid_session_version(row.session_version)?;
        if !status || row.deleted_at.is_some() {
            return Ok(UserSessionLookupByIdOutcome::Rejected);
        }
        Ok(UserSessionLookupByIdOutcome::Authenticated {
            user_id: id,
            role,
            group_id,
            session_version,
            totp_enabled: row.totp_secret.is_some(),
        })
    }

    async fn consume_totp_backup_code_inner(
        &self,
        user_id: UserId,
        expected: EncryptedCredentialEnvelope,
        replacement: EncryptedCredentialEnvelope,
    ) -> Result<bool, UserSessionRepositoryError> {
        // 备份码消费必须把旧封套放入 WHERE，多个并发登录只有一个请求能写入新封套。
        let expected_json = encrypted_json(expected)?;
        let replacement_json = encrypted_json(replacement)?;
        let result = users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id.get()))
            .filter(users::Column::Status.eq(1_i16))
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::TotpSecret.eq(expected_json))
            .col_expr(users::Column::TotpSecret, Expr::value(replacement_json))
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(UserSessionRepositoryError::Query))?;
        match result.rows_affected {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(record_internal_error(UserSessionRepositoryError::Invariant)),
        }
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
        .expr_as(
            Expr::col((users::Entity, users::Column::SessionVersion)),
            Alias::new("user_session_version"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::TotpSecret)),
            Alias::new("user_totp_secret"),
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
            Expr::col((users::Entity, users::Column::DefaultGroupId)),
            Alias::new("user_default_group_id"),
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
            Expr::col((users::Entity, users::Column::SessionVersion)),
            Alias::new("user_session_version"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::TotpSecret)),
            Alias::new("user_totp_secret"),
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
    session_version: i64,
    totp_secret: Option<EncryptedJson>,
}

impl LoginUserRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            user_id: result.try_get("", "user_id")?,
            role: result.try_get("", "user_role")?,
            status: result.try_get("", "user_status")?,
            deleted_at: result.try_get("", "user_deleted_at")?,
            password_hash: result.try_get("", "user_password_hash")?,
            session_version: result.try_get("", "user_session_version")?,
            totp_secret: result.try_get("", "user_totp_secret")?,
        })
    }
}

struct SessionUserRow {
    user_id: i64,
    role: i16,
    default_group_id: i64,
    status: i16,
    deleted_at: Option<TimeDateTimeWithTimeZone>,
    session_version: i64,
    totp_secret: Option<EncryptedJson>,
}

impl SessionUserRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            user_id: result.try_get("", "user_id")?,
            role: result.try_get("", "user_role")?,
            default_group_id: result.try_get("", "user_default_group_id")?,
            status: result.try_get("", "user_status")?,
            deleted_at: result.try_get("", "user_deleted_at")?,
            session_version: result.try_get("", "user_session_version")?,
            totp_secret: result.try_get("", "user_totp_secret")?,
        })
    }
}

fn valid_role(role: i16) -> Result<i16, UserSessionRepositoryError> {
    match role {
        0 | 1 => Ok(role),
        _ => Err(record_internal_error(UserSessionRepositoryError::Invariant)),
    }
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, UserSessionRepositoryError> {
    EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| record_internal_error(UserSessionRepositoryError::Invariant))
}

fn envelope_from_json(
    encrypted: EncryptedJson,
) -> Result<EncryptedCredentialEnvelope, UserSessionRepositoryError> {
    let (key_id, nonce, ciphertext) = encrypted
        .envelope_parts()
        .map_err(|_| record_internal_error(UserSessionRepositoryError::Invariant))?;
    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
        .map_err(|_| record_internal_error(UserSessionRepositoryError::Invariant))
}

fn valid_status(status: i16) -> Result<bool, UserSessionRepositoryError> {
    match status {
        1 => Ok(true),
        2 => Ok(false),
        _ => Err(record_internal_error(UserSessionRepositoryError::Invariant)),
    }
}

fn valid_session_version(version: i64) -> Result<i64, UserSessionRepositoryError> {
    if version >= 1 {
        Ok(version)
    } else {
        Err(record_internal_error(UserSessionRepositoryError::Invariant))
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
