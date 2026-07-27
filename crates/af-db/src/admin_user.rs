use std::{fmt, time::Duration};

use af_domain::{GroupId, UserId};
use sea_orm::{
    ConnectionTrait, DbErr, QueryResult,
    sea_query::{Alias, Expr, Order, Query, SelectStatement},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, entity::users};

/// 单页用户查询允许返回的最大记录数。
pub const MAX_ADMIN_USER_PAGE_SIZE: usize = 100;

/// 管理端可读取的非敏感用户快照。
pub struct AdminUserRecord {
    user_id: UserId,
    username: String,
    email: Option<String>,
    role: i16,
    status: i16,
    default_group_id: GroupId,
    quota: i64,
    used_quota: i64,
    frozen_quota: i64,
    request_count: i64,
    rpm_limit: Option<i32>,
    concurrency: Option<i32>,
}

impl AdminUserRecord {
    /// 返回用户主键。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回精确用户名。
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// 返回可选邮箱。
    #[must_use]
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    /// 返回数据库用户角色码。
    #[must_use]
    pub const fn role(&self) -> i16 {
        self.role
    }

    /// 返回数据库用户状态码。
    #[must_use]
    pub const fn status(&self) -> i16 {
        self.status
    }

    /// 返回当前默认分组标识。
    #[must_use]
    pub const fn default_group_id(&self) -> GroupId {
        self.default_group_id
    }

    /// 返回当前可用额度。
    #[must_use]
    pub const fn quota(&self) -> i64 {
        self.quota
    }

    /// 返回累计已用额度。
    #[must_use]
    pub const fn used_quota(&self) -> i64 {
        self.used_quota
    }

    /// 返回两阶段计费冻结额度。
    #[must_use]
    pub const fn frozen_quota(&self) -> i64 {
        self.frozen_quota
    }

    /// 返回累计请求数。
    #[must_use]
    pub const fn request_count(&self) -> i64 {
        self.request_count
    }

    /// 返回用户级 RPM 兜底限制。
    #[must_use]
    pub const fn rpm_limit(&self) -> Option<i32> {
        self.rpm_limit
    }

    /// 返回用户级并发限制。
    #[must_use]
    pub const fn concurrency(&self) -> Option<i32> {
        self.concurrency
    }
}

impl fmt::Debug for AdminUserRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUserRecord(<redacted>)")
    }
}

/// 一页有界用户结果；下一游标只在仍有记录时返回。
pub struct AdminUserPageRecord {
    users: Vec<AdminUserRecord>,
    next_cursor: Option<UserId>,
}

impl AdminUserPageRecord {
    /// 消费页面并同时返回用户记录和下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<AdminUserRecord>, Option<UserId>) {
        (self.users, self.next_cursor)
    }

    /// 消费页面并返回用户记录。
    #[must_use]
    pub fn into_users(self) -> Vec<AdminUserRecord> {
        self.users
    }

    /// 返回下一页应使用的最后用户 ID。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<UserId> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminUserPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUserPageRecord(<redacted>)")
    }
}

/// 用户详情查询结果。
pub enum AdminUserLookupOutcome {
    /// 找到当前未软删除用户。
    Found(AdminUserRecord),
    /// 用户不存在或已经软删除。
    NotFound,
}

/// 管理用户仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminUserRepositoryConfigError {
    /// 零超时无法形成有效的数据库查询截止时间。
    #[error("管理用户查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 管理用户仓储内部错误；不携带用户名、邮箱或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminUserRepositoryError {
    /// 获取连接或执行查询失败。
    #[error("管理用户数据库查询失败")]
    Query,
    /// 查询超过配置的硬截止时间。
    #[error("管理用户数据库查询超时")]
    Timeout,
    /// 查询输入或持久化结果违反不变量。
    #[error("管理用户持久化状态损坏")]
    Invariant,
    /// 写入的用户名或邮箱与当前有效用户冲突。
    #[error("管理用户唯一身份冲突")]
    Conflict,
    /// 写入的分组或关联目标不存在。
    #[error("管理用户关联目标无效")]
    InvalidReference,
    /// 系统随机源不可用，无法安全生成密码盐或邀请码。
    #[error("管理用户随机源不可用")]
    Entropy,
}

/// 管理端用户列表、详情与写入操作共用的仓储。
#[derive(Clone)]
pub struct AdminUserRepository {
    pub(crate) pool: DatabasePool,
    pub(crate) lookup_timeout: Duration,
}

impl AdminUserRepository {
    /// 使用共享数据库连接池和单次查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, AdminUserRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(AdminUserRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按单调用户 ID 游标读取一页未软删除用户。
    pub async fn list(
        &self,
        after: Option<UserId>,
        limit: usize,
    ) -> Result<AdminUserPageRecord, AdminUserRepositoryError> {
        if !(1..=MAX_ADMIN_USER_PAGE_SIZE).contains(&limit) {
            return Err(record_internal_error(AdminUserRepositoryError::Invariant));
        }
        let mut results = match timeout(
            self.lookup_timeout,
            self.query_all(list_query(after, limit)),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => return Err(record_internal_error(AdminUserRepositoryError::Timeout)),
        };
        let has_more = results.len() > limit;
        if has_more {
            results.truncate(limit);
        }
        let users = results
            .iter()
            .map(AdminUserRecord::try_from_query_result)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| users.last().map(AdminUserRecord::user_id))
            .flatten();
        Ok(AdminUserPageRecord { users, next_cursor })
    }

    /// 按稳定用户 ID 查询当前未软删除用户。
    pub async fn get(
        &self,
        user_id: UserId,
    ) -> Result<AdminUserLookupOutcome, AdminUserRepositoryError> {
        let mut results =
            match timeout(self.lookup_timeout, self.query_all(detail_query(user_id))).await {
                Ok(result) => result?,
                Err(_) => return Err(record_internal_error(AdminUserRepositoryError::Timeout)),
            };
        match results.len() {
            0 => Ok(AdminUserLookupOutcome::NotFound),
            1 => {
                Ok(AdminUserLookupOutcome::Found(
                    AdminUserRecord::try_from_query_result(&results.pop().ok_or_else(|| {
                        record_internal_error(AdminUserRepositoryError::Invariant)
                    })?)?,
                ))
            }
            _ => Err(record_internal_error(AdminUserRepositoryError::Invariant)),
        }
    }

    async fn query_all(
        &self,
        query: SelectStatement,
    ) -> Result<Vec<QueryResult>, AdminUserRepositoryError> {
        let connection = self.pool.connection();
        let statement = connection.get_database_backend().build(&query);
        connection
            .query_all(statement)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(AdminUserRepositoryError::Query))
    }
}

impl fmt::Debug for AdminUserRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminUserRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

impl AdminUserRecord {
    pub(crate) fn try_from_query_result(
        result: &QueryResult,
    ) -> Result<Self, AdminUserRepositoryError> {
        let row = AdminUserRow::try_from_query_result(result)
            .map_err(|_| record_internal_error(AdminUserRepositoryError::Invariant))?;
        row.validate()
    }
}

struct AdminUserRow {
    user_id: i64,
    username: String,
    email: Option<String>,
    role: i16,
    status: i16,
    default_group_id: i64,
    quota: i64,
    used_quota: i64,
    frozen_quota: i64,
    request_count: i64,
    rpm_limit: Option<i32>,
    concurrency: Option<i32>,
}

impl AdminUserRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            user_id: result.try_get("", "user_id")?,
            username: result.try_get("", "username")?,
            email: result.try_get("", "email")?,
            role: result.try_get("", "role")?,
            status: result.try_get("", "status")?,
            default_group_id: result.try_get("", "default_group_id")?,
            quota: result.try_get("", "quota")?,
            used_quota: result.try_get("", "used_quota")?,
            frozen_quota: result.try_get("", "frozen_quota")?,
            request_count: result.try_get("", "request_count")?,
            rpm_limit: result.try_get("", "rpm_limit")?,
            concurrency: result.try_get("", "concurrency")?,
        })
    }

    fn validate(self) -> Result<AdminUserRecord, AdminUserRepositoryError> {
        let valid_username = !self.username.is_empty()
            && self.username.len() <= 64
            && !self.username.chars().any(char::is_control);
        let valid_email = self.email.as_ref().is_none_or(|email| {
            !email.is_empty() && email.len() <= 320 && !email.chars().any(char::is_control)
        });
        if !valid_username
            || !valid_email
            || !matches!(self.role, 0 | 1)
            || !matches!(self.status, 1 | 2)
            || [
                self.quota,
                self.used_quota,
                self.frozen_quota,
                self.request_count,
            ]
            .iter()
            .any(|value| *value < 0)
            || self.rpm_limit.is_some_and(|value| value < 0)
            || self.concurrency.is_some_and(|value| value < 0)
        {
            return Err(record_internal_error(AdminUserRepositoryError::Invariant));
        }
        Ok(AdminUserRecord {
            user_id: UserId::new(self.user_id)
                .map_err(|_| record_internal_error(AdminUserRepositoryError::Invariant))?,
            username: self.username,
            email: self.email,
            role: self.role,
            status: self.status,
            default_group_id: GroupId::new(self.default_group_id)
                .map_err(|_| record_internal_error(AdminUserRepositoryError::Invariant))?,
            quota: self.quota,
            used_quota: self.used_quota,
            frozen_quota: self.frozen_quota,
            request_count: self.request_count,
            rpm_limit: self.rpm_limit,
            concurrency: self.concurrency,
        })
    }
}

fn list_query(after: Option<UserId>, limit: usize) -> SelectStatement {
    let mut query = base_query();
    query
        .and_where(Expr::col((users::Entity, users::Column::DeletedAt)).is_null())
        .order_by((users::Entity, users::Column::Id), Order::Asc)
        .limit((limit + 1) as u64);
    if let Some(after) = after {
        query.and_where(Expr::col((users::Entity, users::Column::Id)).gt(after.get()));
    }
    query.to_owned()
}

pub(crate) fn detail_query(user_id: UserId) -> SelectStatement {
    base_query()
        .and_where(Expr::col((users::Entity, users::Column::Id)).eq(user_id.get()))
        .and_where(Expr::col((users::Entity, users::Column::DeletedAt)).is_null())
        .limit(2)
        .to_owned()
}

fn base_query() -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((users::Entity, users::Column::Id)),
            Alias::new("user_id"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Username)),
            Alias::new("username"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Email)),
            Alias::new("email"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Role)),
            Alias::new("role"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Status)),
            Alias::new("status"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::DefaultGroupId)),
            Alias::new("default_group_id"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Quota)),
            Alias::new("quota"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::UsedQuota)),
            Alias::new("used_quota"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::FrozenQuota)),
            Alias::new("frozen_quota"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::RequestCount)),
            Alias::new("request_count"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::RpmLimit)),
            Alias::new("rpm_limit"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Concurrency)),
            Alias::new("concurrency"),
        )
        .from(users::Entity)
        .to_owned()
}

pub(crate) fn record_internal_error(error: AdminUserRepositoryError) -> AdminUserRepositoryError {
    let error_kind = match error {
        AdminUserRepositoryError::Query => "admin_user_query",
        AdminUserRepositoryError::Timeout => "admin_user_timeout",
        AdminUserRepositoryError::Invariant => "admin_user_invariant",
        AdminUserRepositoryError::Conflict => "admin_user_conflict",
        AdminUserRepositoryError::InvalidReference => "admin_user_invalid_reference",
        AdminUserRepositoryError::Entropy => "admin_user_entropy",
    };
    tracing::error!(target: "af_db::admin_user", error_kind, "管理用户仓储发生内部错误");
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_validation_rejects_unknown_or_negative_state() {
        let valid = || AdminUserRow {
            user_id: 1,
            username: "user".to_owned(),
            email: Some("user@example.com".to_owned()),
            role: 0,
            status: 1,
            default_group_id: 2,
            quota: 0,
            used_quota: 0,
            frozen_quota: 0,
            request_count: 0,
            rpm_limit: None,
            concurrency: None,
        };
        assert!(valid().validate().is_ok());
        let mut invalid_role = valid();
        invalid_role.role = 2;
        assert_eq!(
            invalid_role.validate().unwrap_err(),
            AdminUserRepositoryError::Invariant
        );
        let mut invalid_quota = valid();
        invalid_quota.quota = -1;
        assert_eq!(
            invalid_quota.validate().unwrap_err(),
            AdminUserRepositoryError::Invariant
        );
    }

    #[test]
    fn queries_never_select_password_totp_or_settings() {
        let rendered = sea_orm::DbBackend::Postgres
            .build(&list_query(None, 10))
            .to_string();
        for forbidden in ["password_hash", "totp_secret", "settings", "aff_code"] {
            assert!(!rendered.contains(forbidden), "{rendered}");
        }
        assert!(rendered.contains("LIMIT 11"), "{rendered}");
        assert!(rendered.contains("deleted_at"), "{rendered}");
    }
}
