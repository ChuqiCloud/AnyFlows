use std::{fmt, future::Future, pin::Pin};

use af_db::{
    BalanceAlertSettingsRecord, BalanceAlertSettingsRepository,
    BalanceAlertSettingsRepositoryError, UserNotificationPreferencesRecord,
    UserPasswordChangeOutcome, UserProfileLookupOutcome, UserProfileMutationOutcome,
    UserProfileRecord, UserProfileRepository, UserProfileRepositoryError,
};
use af_domain::{Quota, UserId};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{SessionPrincipal, SessionRole};

/// 个人资料用户名最大字节数。
pub const MAX_USER_PROFILE_USERNAME_BYTES: usize = af_db::MAX_USER_PROFILE_USERNAME_BYTES;

/// 修改密码允许的最小字节数，与注册和密码重置保持一致。
pub const MIN_USER_PROFILE_PASSWORD_BYTES: usize = 12;

/// 修改密码允许的最大字节数，与注册和密码重置保持一致。
pub const MAX_USER_PROFILE_PASSWORD_BYTES: usize = 128;

/// 当前登录用户可见的非敏感资料快照。
#[derive(Clone, Eq, PartialEq)]
pub struct UserProfile {
    user_id: UserId,
    username: String,
    email: Option<String>,
    role: SessionRole,
    notifications: UserNotificationPreferences,
}

impl fmt::Debug for UserProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserProfile(<redacted>)")
    }
}

impl UserProfile {
    /// 返回稳定用户标识。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回登录用户名。
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// 返回已保存邮箱。
    #[must_use]
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    /// 返回当前会话角色。
    #[must_use]
    pub const fn role(&self) -> SessionRole {
        self.role
    }

    /// 返回用户通知偏好。
    #[must_use]
    pub const fn notifications(&self) -> UserNotificationPreferences {
        self.notifications
    }
}

/// 当前用户可修改的邮件通知偏好。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserNotificationPreferences {
    email_product_updates: bool,
    email_usage_alerts: bool,
    balance_alert_enabled: bool,
    balance_alert_threshold: Option<Quota>,
    effective_balance_alert_threshold: Quota,
}

impl UserNotificationPreferences {
    /// 组合通知偏好命令或读取结果。
    #[must_use]
    pub const fn new(
        email_product_updates: bool,
        email_usage_alerts: bool,
        balance_alert_enabled: bool,
        balance_alert_threshold: Option<Quota>,
        effective_balance_alert_threshold: Quota,
    ) -> Self {
        Self {
            email_product_updates,
            email_usage_alerts,
            balance_alert_enabled,
            balance_alert_threshold,
            effective_balance_alert_threshold,
        }
    }

    /// 是否接收产品更新邮件。
    #[must_use]
    pub const fn email_product_updates(self) -> bool {
        self.email_product_updates
    }

    /// 是否接收用量提醒邮件。
    #[must_use]
    pub const fn email_usage_alerts(self) -> bool {
        self.email_usage_alerts
    }

    /// 系统是否启用余额预警任务。
    #[must_use]
    pub const fn balance_alert_enabled(self) -> bool {
        self.balance_alert_enabled
    }

    /// 返回个人阈值；空值表示继承系统默认值。
    #[must_use]
    pub const fn balance_alert_threshold(self) -> Option<Quota> {
        self.balance_alert_threshold
    }

    /// 返回个人阈值与系统默认值合并后的当前有效阈值。
    #[must_use]
    pub const fn effective_balance_alert_threshold(self) -> Quota {
        self.effective_balance_alert_threshold
    }
}

/// 更新当前用户登录用户名的命令。
pub struct UserProfileUpdateCommand {
    username: String,
}

impl UserProfileUpdateCommand {
    /// 校验用户名边界；邮箱变更需等待独立验证切片。
    pub fn new(username: String) -> Result<Self, UserProfileError> {
        if !valid_username(&username) {
            return Err(UserProfileError::InvalidInput);
        }
        Ok(Self { username })
    }

    fn into_username(self) -> String {
        self.username
    }
}

impl fmt::Debug for UserProfileUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserProfileUpdateCommand(<redacted>)")
    }
}

/// 更新当前用户通知偏好的命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserNotificationPreferencesCommand {
    email_product_updates: bool,
    email_usage_alerts: bool,
    balance_alert_threshold: Option<Quota>,
}

impl UserNotificationPreferencesCommand {
    /// 构造通知偏好写命令；零阈值不能表达有效提醒边界。
    pub fn new(
        email_product_updates: bool,
        email_usage_alerts: bool,
        balance_alert_threshold: Option<Quota>,
    ) -> Result<Self, UserProfileError> {
        if balance_alert_threshold.is_some_and(|threshold| threshold.is_zero()) {
            return Err(UserProfileError::InvalidInput);
        }
        Ok(Self {
            email_product_updates,
            email_usage_alerts,
            balance_alert_threshold,
        })
    }

    fn into_record(self) -> UserNotificationPreferencesRecord {
        UserNotificationPreferencesRecord::new(
            self.email_product_updates,
            self.email_usage_alerts,
            self.balance_alert_threshold,
        )
    }
}

/// 修改当前用户密码的敏感命令。
pub struct UserPasswordChangeCommand {
    current_password: Zeroizing<String>,
    new_password: Zeroizing<String>,
}

impl UserPasswordChangeCommand {
    /// 校验新密码边界并把两份密码放入可清零容器。
    pub fn new(current_password: String, new_password: String) -> Result<Self, UserProfileError> {
        if current_password.is_empty()
            || current_password.len() > 4_096
            || current_password.chars().any(char::is_control)
            || !valid_new_password(&new_password)
        {
            return Err(UserProfileError::InvalidInput);
        }
        Ok(Self {
            current_password: Zeroizing::new(current_password),
            new_password: Zeroizing::new(new_password),
        })
    }

    fn into_parts(self) -> (Zeroizing<String>, Zeroizing<String>) {
        (self.current_password, self.new_password)
    }
}

impl fmt::Debug for UserPasswordChangeCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserPasswordChangeCommand(<redacted>)")
    }
}

/// 个人资料应用服务错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserProfileError {
    /// 请求字段违反用户名或密码边界。
    #[error("个人资料输入无效")]
    InvalidInput,
    /// 当前会话已失效或用户已不存在。
    #[error("个人资料会话无效")]
    InvalidSession,
    /// 当前密码错误。
    #[error("当前密码不正确")]
    CurrentPasswordInvalid,
    /// 用户名与其他有效账户冲突。
    #[error("个人资料用户名冲突")]
    Conflict,
    /// 数据库或持久化状态失败。
    #[error("个人资料内部失败")]
    Internal,
}

/// 读取当前用户资料的对象安全 Future。
pub type UserProfileReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserProfile, UserProfileError>> + Send + 'a>>;

/// 更新当前用户资料的对象安全 Future。
pub type UserProfileUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserProfile, UserProfileError>> + Send + 'a>>;

/// 修改当前用户密码的对象安全 Future。
pub type UserPasswordChangeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), UserProfileError>> + Send + 'a>>;

/// 更新当前用户通知偏好的对象安全 Future。
pub type UserNotificationUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserProfile, UserProfileError>> + Send + 'a>>;

/// 普通用户自有资料与安全设置应用端口；所有方法只接受当前会话主体。
pub trait UserProfileService: Send + Sync {
    /// 读取当前会话用户资料。
    fn get<'a>(&'a self, principal: SessionPrincipal) -> UserProfileReadFuture<'a>;

    /// 更新当前会话用户的登录用户名。
    fn update_profile<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserProfileUpdateCommand,
    ) -> UserProfileUpdateFuture<'a>;

    /// 校验旧密码并更新新密码；成功后旧 JWT 全部失效。
    fn change_password<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserPasswordChangeCommand,
    ) -> UserPasswordChangeFuture<'a>;

    /// 更新当前会话用户的通知偏好。
    fn update_notifications<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserNotificationPreferencesCommand,
    ) -> UserNotificationUpdateFuture<'a>;
}

/// 使用用户资料数据库仓储的生产应用服务。
pub struct DatabaseUserProfileService {
    repository: UserProfileRepository,
    balance_alert_settings: BalanceAlertSettingsRepository,
}

impl DatabaseUserProfileService {
    /// 绑定已经配置操作截止时间的用户资料仓储。
    #[must_use]
    pub const fn new(
        repository: UserProfileRepository,
        balance_alert_settings: BalanceAlertSettingsRepository,
    ) -> Self {
        Self {
            repository,
            balance_alert_settings,
        }
    }
}

impl UserProfileService for DatabaseUserProfileService {
    fn get<'a>(&'a self, principal: SessionPrincipal) -> UserProfileReadFuture<'a> {
        Box::pin(async move {
            let settings = self.balance_alert_settings().await?;
            match self
                .repository
                .get(principal.user_id())
                .await
                .map_err(map_repository_error)?
            {
                UserProfileLookupOutcome::Found(record) => profile_from_record(record, settings),
                UserProfileLookupOutcome::NotFound => Err(UserProfileError::InvalidSession),
            }
        })
    }

    fn update_profile<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserProfileUpdateCommand,
    ) -> UserProfileUpdateFuture<'a> {
        Box::pin(async move {
            let settings = self.balance_alert_settings().await?;
            match self
                .repository
                .update_username(principal.user_id(), command.into_username())
                .await
                .map_err(map_repository_error)?
            {
                UserProfileMutationOutcome::Updated(record) => {
                    profile_from_record(record, settings)
                }
                UserProfileMutationOutcome::NotFound => Err(UserProfileError::InvalidSession),
                UserProfileMutationOutcome::Conflict => Err(UserProfileError::Conflict),
            }
        })
    }

    fn change_password<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserPasswordChangeCommand,
    ) -> UserPasswordChangeFuture<'a> {
        Box::pin(async move {
            let (current_password, new_password) = command.into_parts();
            match self
                .repository
                .change_password(
                    principal.user_id(),
                    current_password.as_bytes(),
                    new_password,
                )
                .await
                .map_err(map_repository_error)?
            {
                UserPasswordChangeOutcome::Updated(_) => Ok(()),
                UserPasswordChangeOutcome::NotFound => Err(UserProfileError::InvalidSession),
                UserPasswordChangeOutcome::Rejected => {
                    Err(UserProfileError::CurrentPasswordInvalid)
                }
            }
        })
    }

    fn update_notifications<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserNotificationPreferencesCommand,
    ) -> UserNotificationUpdateFuture<'a> {
        Box::pin(async move {
            let settings = self.balance_alert_settings().await?;
            match self
                .repository
                .update_notifications(principal.user_id(), command.into_record())
                .await
                .map_err(map_repository_error)?
            {
                UserProfileMutationOutcome::Updated(record) => {
                    profile_from_record(record, settings)
                }
                UserProfileMutationOutcome::NotFound => Err(UserProfileError::InvalidSession),
                UserProfileMutationOutcome::Conflict => Err(UserProfileError::Internal),
            }
        })
    }
}

impl DatabaseUserProfileService {
    async fn balance_alert_settings(&self) -> Result<BalanceAlertSettingsRecord, UserProfileError> {
        self.balance_alert_settings
            .settings()
            .await
            .map_err(map_balance_alert_settings_error)
    }
}

impl fmt::Debug for DatabaseUserProfileService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseUserProfileService(<redacted>)")
    }
}

fn profile_from_record(
    record: UserProfileRecord,
    settings: BalanceAlertSettingsRecord,
) -> Result<UserProfile, UserProfileError> {
    let personal_threshold = record.notifications().balance_alert_threshold();
    Ok(UserProfile {
        user_id: record.user_id(),
        username: record.username().to_owned(),
        email: record.email().map(str::to_owned),
        role: SessionRole::from_database(record.role()).map_err(|_| UserProfileError::Internal)?,
        notifications: UserNotificationPreferences::new(
            record.notifications().email_product_updates(),
            record.notifications().email_usage_alerts(),
            settings.enabled(),
            personal_threshold,
            personal_threshold.unwrap_or(settings.default_threshold()),
        ),
    })
}

fn map_balance_alert_settings_error(_: BalanceAlertSettingsRepositoryError) -> UserProfileError {
    UserProfileError::Internal
}

fn map_repository_error(error: UserProfileRepositoryError) -> UserProfileError {
    match error {
        UserProfileRepositoryError::Conflict => UserProfileError::Conflict,
        UserProfileRepositoryError::Query
        | UserProfileRepositoryError::Timeout
        | UserProfileRepositoryError::Invariant
        | UserProfileRepositoryError::Entropy => UserProfileError::Internal,
    }
}

fn valid_username(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_USER_PROFILE_USERNAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_new_password(value: &str) -> bool {
    (MIN_USER_PROFILE_PASSWORD_BYTES..=MAX_USER_PROFILE_PASSWORD_BYTES).contains(&value.len())
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_enforce_boundaries_and_redact_passwords() {
        assert!(UserProfileUpdateCommand::new("owner".to_owned()).is_ok());
        assert_eq!(
            UserProfileUpdateCommand::new(" owner".to_owned()).unwrap_err(),
            UserProfileError::InvalidInput
        );
        let command = UserPasswordChangeCommand::new(
            "old secure password".to_owned(),
            "new secure password".to_owned(),
        )
        .unwrap();
        assert_eq!(
            format!("{command:?}"),
            "UserPasswordChangeCommand(<redacted>)"
        );
        assert!(!format!("{command:?}").contains("new secure password"));
        assert_eq!(
            UserPasswordChangeCommand::new("old".to_owned(), "short".to_owned()).unwrap_err(),
            UserProfileError::InvalidInput
        );
        assert!(UserNotificationPreferencesCommand::new(false, true, None).is_ok());
        assert_eq!(
            UserNotificationPreferencesCommand::new(false, true, Some(Quota::ZERO)).unwrap_err(),
            UserProfileError::InvalidInput
        );
    }
}
