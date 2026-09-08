use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminUserCreateRecord, AdminUserDeleteOutcome, AdminUserMutationOutcome, AdminUserRepository,
    AdminUserRepositoryError, AdminUserUpdateRecord,
};
use af_domain::{GroupId, Quota, UserId};
use thiserror::Error;

use crate::{AdminUser, AdminUserReadError, AdminUserStatus, SessionPrincipal, SessionRole};

/// 管理端用户密码输入最大字节数，与登录接口保持一致。
pub const MAX_ADMIN_USER_PASSWORD_BYTES: usize = 4_096;

/// 管理员创建用户时允许写入的业务字段。
pub struct AdminUserCreateCommand {
    username: String,
    email: Option<String>,
    password: Option<String>,
    role: SessionRole,
    status: AdminUserStatus,
    default_group_id: GroupId,
    quota: Quota,
    rpm_limit: Option<i32>,
    concurrency: Option<i32>,
}

impl AdminUserCreateCommand {
    /// 校验公开写入边界，拒绝控制字符、负数额度和无效限流参数。
    #[allow(clippy::too_many_arguments, reason = "字段与管理端写入契约一一对应")]
    pub fn new(
        username: String,
        email: Option<String>,
        password: Option<String>,
        role: SessionRole,
        status: AdminUserStatus,
        default_group_id: GroupId,
        quota: i64,
        rpm_limit: Option<i32>,
        concurrency: Option<i32>,
    ) -> Result<Self, AdminUserWriteError> {
        validate_username(&username)?;
        validate_email(email.as_deref())?;
        validate_password(password.as_deref())?;
        let quota = Quota::new(quota).map_err(|_| AdminUserWriteError::InvalidInput)?;
        validate_optional_i32(rpm_limit)?;
        validate_optional_i32(concurrency)?;
        Ok(Self {
            username,
            email,
            password,
            role,
            status,
            default_group_id,
            quota,
            rpm_limit,
            concurrency,
        })
    }

    fn into_record(self) -> AdminUserCreateRecord {
        AdminUserCreateRecord::new(
            self.username,
            self.email,
            self.password,
            self.role.to_database(),
            self.status.to_database(),
            self.default_group_id,
            self.quota.units(),
            self.rpm_limit,
            self.concurrency,
        )
    }
}

impl fmt::Debug for AdminUserCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUserCreateCommand(<redacted>)")
    }
}

/// 管理员更新用户时允许覆盖的业务字段；密码缺省表示保持原值。
pub struct AdminUserUpdateCommand {
    username: String,
    email: Option<String>,
    password: Option<String>,
    role: SessionRole,
    status: AdminUserStatus,
    default_group_id: GroupId,
    rpm_limit: Option<i32>,
    concurrency: Option<i32>,
}

impl AdminUserUpdateCommand {
    /// 校验完整更新边界，避免无效值进入仓储事务。
    #[allow(clippy::too_many_arguments, reason = "字段与管理端写入契约一一对应")]
    pub fn new(
        username: String,
        email: Option<String>,
        password: Option<String>,
        role: SessionRole,
        status: AdminUserStatus,
        default_group_id: GroupId,
        rpm_limit: Option<i32>,
        concurrency: Option<i32>,
    ) -> Result<Self, AdminUserWriteError> {
        validate_username(&username)?;
        validate_email(email.as_deref())?;
        validate_password(password.as_deref())?;
        validate_optional_i32(rpm_limit)?;
        validate_optional_i32(concurrency)?;
        Ok(Self {
            username,
            email,
            password,
            role,
            status,
            default_group_id,
            rpm_limit,
            concurrency,
        })
    }

    fn into_record(self) -> AdminUserUpdateRecord {
        AdminUserUpdateRecord::new(
            self.username,
            self.email,
            self.password,
            self.role.to_database(),
            self.status.to_database(),
            self.default_group_id,
            self.rpm_limit,
            self.concurrency,
        )
    }
}

impl fmt::Debug for AdminUserUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUserUpdateCommand(<redacted>)")
    }
}

/// 管理用户写入失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminUserWriteError {
    /// 请求字段违反公开边界或引用了不存在的分组。
    #[error("管理用户写入参数无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("管理用户写入权限不足")]
    Forbidden,
    /// 用户名或邮箱与当前有效用户冲突。
    #[error("管理用户唯一身份冲突")]
    Conflict,
    /// 用户不存在或已经软删除。
    #[error("管理用户不存在")]
    NotFound,
    /// 数据库失败或持久化状态损坏。
    #[error("管理用户写入内部失败")]
    Internal,
}

/// 管理用户创建调用的对象安全 Future。
pub type AdminUserCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminUser, AdminUserWriteError>> + Send + 'a>>;

/// 管理用户更新调用的对象安全 Future。
pub type AdminUserUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminUser, AdminUserWriteError>> + Send + 'a>>;

/// 管理用户删除调用的对象安全 Future。
pub type AdminUserDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminUserWriteError>> + Send + 'a>>;

/// 管理用户写入应用端口；角色校验必须在进入仓储前完成。
pub trait AdminUserWriter: Send + Sync {
    /// 创建用户并返回非敏感管理快照。
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminUserCreateCommand,
    ) -> AdminUserCreateFuture<'a>;

    /// 更新用户并返回非敏感管理快照。
    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        command: AdminUserUpdateCommand,
    ) -> AdminUserUpdateFuture<'a>;

    /// 软删除用户及其直接令牌依赖。
    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
    ) -> AdminUserDeleteFuture<'a>;
}

/// 使用数据库仓储实现管理员用户写入。
pub struct DatabaseAdminUserWriter {
    repository: AdminUserRepository,
}

impl DatabaseAdminUserWriter {
    /// 绑定已经配置查询/写入截止时间的用户仓储。
    #[must_use]
    pub const fn new(repository: AdminUserRepository) -> Self {
        Self { repository }
    }
}

impl AdminUserWriter for DatabaseAdminUserWriter {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminUserCreateCommand,
    ) -> AdminUserCreateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let record = self
                .repository
                .create(command.into_record())
                .await
                .map_err(map_repository_error)?;
            AdminUser::from_record(record).map_err(map_read_error)
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        command: AdminUserUpdateCommand,
    ) -> AdminUserUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .update(user_id, command.into_record())
                .await
                .map_err(map_repository_error)?
            {
                AdminUserMutationOutcome::Mutated(record) => {
                    AdminUser::from_record(record).map_err(map_read_error)
                }
                AdminUserMutationOutcome::NotFound => Err(AdminUserWriteError::NotFound),
            }
        })
    }

    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
    ) -> AdminUserDeleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .delete(user_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminUserDeleteOutcome::Deleted => Ok(()),
                AdminUserDeleteOutcome::NotFound => Err(AdminUserWriteError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminUserWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminUserWriter(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminUserWriteError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminUserWriteError::Forbidden)
    }
}

fn map_repository_error(error: AdminUserRepositoryError) -> AdminUserWriteError {
    match error {
        AdminUserRepositoryError::Conflict => AdminUserWriteError::Conflict,
        AdminUserRepositoryError::InvalidReference => AdminUserWriteError::InvalidInput,
        AdminUserRepositoryError::Query
        | AdminUserRepositoryError::Timeout
        | AdminUserRepositoryError::Invariant
        | AdminUserRepositoryError::Entropy => AdminUserWriteError::Internal,
    }
}

fn map_read_error(error: AdminUserReadError) -> AdminUserWriteError {
    let _ = error;
    AdminUserWriteError::Internal
}

fn validate_username(username: &str) -> Result<(), AdminUserWriteError> {
    if username.is_empty()
        || username.len() > 64
        || username.chars().any(char::is_control)
        || username.trim() != username
    {
        return Err(AdminUserWriteError::InvalidInput);
    }
    Ok(())
}

fn validate_email(email: Option<&str>) -> Result<(), AdminUserWriteError> {
    let Some(email) = email else {
        return Ok(());
    };
    if email.is_empty()
        || email.len() > 320
        || email.chars().any(char::is_control)
        || email.trim() != email
    {
        return Err(AdminUserWriteError::InvalidInput);
    }
    Ok(())
}

fn validate_password(password: Option<&str>) -> Result<(), AdminUserWriteError> {
    let Some(password) = password else {
        return Ok(());
    };
    if password.is_empty() || password.len() > MAX_ADMIN_USER_PASSWORD_BYTES {
        return Err(AdminUserWriteError::InvalidInput);
    }
    Ok(())
}

fn validate_optional_i32(value: Option<i32>) -> Result<(), AdminUserWriteError> {
    if value.is_some_and(|value| value < 0) {
        return Err(AdminUserWriteError::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_validate_public_write_boundaries_and_redact_passwords() {
        let command = AdminUserCreateCommand::new(
            "new-user".to_owned(),
            Some("new@example.com".to_owned()),
            Some("secret-password".to_owned()),
            SessionRole::User,
            AdminUserStatus::Enabled,
            GroupId::new(1).unwrap(),
            0,
            None,
            None,
        )
        .unwrap();
        assert_eq!(format!("{command:?}"), "AdminUserCreateCommand(<redacted>)");
        assert!(!format!("{command:?}").contains("secret-password"));

        assert_eq!(
            AdminUserCreateCommand::new(
                " bad ".to_owned(),
                None,
                None,
                SessionRole::User,
                AdminUserStatus::Enabled,
                GroupId::new(1).unwrap(),
                0,
                None,
                None,
            )
            .unwrap_err(),
            AdminUserWriteError::InvalidInput
        );
        assert_eq!(
            AdminUserCreateCommand::new(
                "ok".to_owned(),
                Some(String::new()),
                None,
                SessionRole::User,
                AdminUserStatus::Enabled,
                GroupId::new(1).unwrap(),
                -1,
                None,
                None,
            )
            .unwrap_err(),
            AdminUserWriteError::InvalidInput
        );
    }

    #[test]
    fn normal_user_is_rejected_before_repository_access() {
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminUserWriteError::Forbidden)
        );
    }
}
