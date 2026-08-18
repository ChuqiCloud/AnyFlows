use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminUserLookupOutcome, AdminUserRecord, AdminUserRepository, AdminUserRepositoryError,
    MAX_ADMIN_USER_PAGE_SIZE,
};
use af_domain::{GroupId, PlatformPermission, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{PlatformPolicy, SessionPrincipal, SessionRole};

/// 管理用户列表默认页大小。
pub const DEFAULT_ADMIN_USER_PAGE_SIZE: usize = 50;

/// 已校验的管理用户列表查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminUserListQuery {
    after: Option<UserId>,
    limit: usize,
}

impl AdminUserListQuery {
    /// 校验单调 ID 游标和固定页大小边界。
    pub fn new(after: Option<UserId>, limit: usize) -> Result<Self, AdminUserReadError> {
        if !(1..=MAX_ADMIN_USER_PAGE_SIZE).contains(&limit) {
            return Err(AdminUserReadError::InvalidPagination);
        }
        Ok(Self { after, limit })
    }

    /// 返回上一页最后一个用户 ID。
    #[must_use]
    pub const fn after(self) -> Option<UserId> {
        self.after
    }

    /// 返回本页最大记录数。
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for AdminUserListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ADMIN_USER_PAGE_SIZE,
        }
    }
}

/// 管理 API 公开的用户状态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminUserStatus {
    /// 用户允许登录和使用令牌。
    Enabled,
    /// 用户被管理员禁用。
    Disabled,
}

impl AdminUserStatus {
    fn from_database(value: i16) -> Result<Self, AdminUserReadError> {
        match value {
            1 => Ok(Self::Enabled),
            2 => Ok(Self::Disabled),
            _ => Err(AdminUserReadError::Internal),
        }
    }

    /// 返回数据库中固定使用的用户状态编码。
    #[must_use]
    pub const fn to_database(self) -> i16 {
        match self {
            Self::Enabled => 1,
            Self::Disabled => 2,
        }
    }
}

/// 管理 API 可读取的非敏感用户快照。
pub struct AdminUser {
    user_id: UserId,
    username: String,
    email: Option<String>,
    role: SessionRole,
    status: AdminUserStatus,
    default_group_id: GroupId,
    quota: i64,
    used_quota: i64,
    frozen_quota: i64,
    request_count: i64,
    rpm_limit: Option<i32>,
    concurrency: Option<i32>,
}

impl AdminUser {
    /// 组合已完成持久化校验的用户字段，供仓储适配器和测试实现使用。
    #[allow(clippy::too_many_arguments, reason = "字段与稳定管理 API 响应一一对应")]
    #[must_use]
    pub fn from_parts(
        user_id: UserId,
        username: String,
        email: Option<String>,
        role: SessionRole,
        status: AdminUserStatus,
        default_group_id: GroupId,
        quota: i64,
        used_quota: i64,
        frozen_quota: i64,
        request_count: i64,
        rpm_limit: Option<i32>,
        concurrency: Option<i32>,
    ) -> Self {
        Self {
            user_id,
            username,
            email,
            role,
            status,
            default_group_id,
            quota,
            used_quota,
            frozen_quota,
            request_count,
            rpm_limit,
            concurrency,
        }
    }

    /// 返回用户标识。
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

    /// 返回用户角色。
    #[must_use]
    pub const fn role(&self) -> SessionRole {
        self.role
    }

    /// 返回用户状态。
    #[must_use]
    pub const fn status(&self) -> AdminUserStatus {
        self.status
    }

    /// 返回默认分组标识。
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

    /// 返回冻结额度。
    #[must_use]
    pub const fn frozen_quota(&self) -> i64 {
        self.frozen_quota
    }

    /// 返回累计请求数。
    #[must_use]
    pub const fn request_count(&self) -> i64 {
        self.request_count
    }

    /// 返回用户级 RPM 限制。
    #[must_use]
    pub const fn rpm_limit(&self) -> Option<i32> {
        self.rpm_limit
    }

    /// 返回用户级并发限制。
    #[must_use]
    pub const fn concurrency(&self) -> Option<i32> {
        self.concurrency
    }

    pub(crate) fn from_record(record: AdminUserRecord) -> Result<Self, AdminUserReadError> {
        Ok(Self::from_parts(
            record.user_id(),
            record.username().to_owned(),
            record.email().map(str::to_owned),
            SessionRole::from_database(record.role()).map_err(|_| AdminUserReadError::Internal)?,
            AdminUserStatus::from_database(record.status())?,
            record.default_group_id(),
            record.quota(),
            record.used_quota(),
            record.frozen_quota(),
            record.request_count(),
            record.rpm_limit(),
            record.concurrency(),
        ))
    }
}

impl fmt::Debug for AdminUser {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUser(<redacted>)")
    }
}

/// 一页管理用户响应。
pub struct AdminUserPage {
    users: Vec<AdminUser>,
    next_cursor: Option<UserId>,
}

impl AdminUserPage {
    /// 组合用户列表和可选下一游标。
    #[must_use]
    pub fn from_parts(users: Vec<AdminUser>, next_cursor: Option<UserId>) -> Self {
        Self { users, next_cursor }
    }

    /// 返回当前页用户。
    #[must_use]
    pub fn users(&self) -> &[AdminUser] {
        &self.users
    }

    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<UserId> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminUserPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUserPage(<redacted>)")
    }
}

/// 管理用户读取失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminUserReadError {
    /// 游标或页大小不满足公开边界。
    #[error("管理用户分页参数无效")]
    InvalidPagination,
    /// 当前会话不是管理员。
    #[error("管理用户读取权限不足")]
    Forbidden,
    /// 用户不存在或已经软删除。
    #[error("管理用户不存在")]
    NotFound,
    /// 数据库失败或持久化状态损坏。
    #[error("管理用户读取内部失败")]
    Internal,
}

/// 管理用户列表调用的对象安全 Future。
pub type AdminUserListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminUserPage, AdminUserReadError>> + Send + 'a>>;

/// 管理用户详情调用的对象安全 Future。
pub type AdminUserGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminUser, AdminUserReadError>> + Send + 'a>>;

/// 管理用户只读应用端口；角色校验必须在进入仓储前完成。
pub trait AdminUserReader: Send + Sync {
    /// 读取一页用户。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminUserListQuery,
    ) -> AdminUserListFuture<'a>;

    /// 按用户 ID 读取详情。
    fn get<'a>(&'a self, principal: SessionPrincipal, user_id: UserId) -> AdminUserGetFuture<'a>;
}

/// 使用数据库仓储实现管理员用户读取。
pub struct DatabaseAdminUserReader {
    repository: AdminUserRepository,
}

impl DatabaseAdminUserReader {
    /// 绑定已配置查询截止时间的用户仓储。
    #[must_use]
    pub const fn new(repository: AdminUserRepository) -> Self {
        Self { repository }
    }
}

impl AdminUserReader for DatabaseAdminUserReader {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminUserListQuery,
    ) -> AdminUserListFuture<'a> {
        Box::pin(async move {
            PlatformPolicy::authorize(principal, PlatformPermission::UserDirectoryReadAll)
                .map_err(|_| AdminUserReadError::Forbidden)?;
            let page = self
                .repository
                .list(query.after(), query.limit())
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page.into_parts();
            let users = records
                .into_iter()
                .map(AdminUser::from_record)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(AdminUserPage::from_parts(users, next_cursor))
        })
    }

    fn get<'a>(&'a self, principal: SessionPrincipal, user_id: UserId) -> AdminUserGetFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .get(user_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminUserLookupOutcome::Found(record) => AdminUser::from_record(record),
                AdminUserLookupOutcome::NotFound => Err(AdminUserReadError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminUserReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminUserReader(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminUserReadError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminUserReadError::Forbidden)
    }
}

fn map_repository_error(error: AdminUserRepositoryError) -> AdminUserReadError {
    let _ = error;
    AdminUserReadError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_and_sensitive_debug_contract_are_closed() {
        assert_eq!(AdminUserListQuery::default().limit(), 50);
        assert_eq!(
            AdminUserListQuery::new(None, 0),
            Err(AdminUserReadError::InvalidPagination)
        );
        assert_eq!(
            AdminUserListQuery::new(None, MAX_ADMIN_USER_PAGE_SIZE + 1),
            Err(AdminUserReadError::InvalidPagination)
        );
        let user = AdminUser::from_parts(
            UserId::new(1).unwrap(),
            "private-user".to_owned(),
            Some("private@example.com".to_owned()),
            SessionRole::Admin,
            AdminUserStatus::Enabled,
            GroupId::new(2).unwrap(),
            0,
            0,
            0,
            0,
            None,
            None,
        );
        assert_eq!(format!("{user:?}"), "AdminUser(<redacted>)");
    }

    #[test]
    fn normal_user_is_rejected_before_repository_access() {
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(require_admin(principal), Err(AdminUserReadError::Forbidden));
    }
}
