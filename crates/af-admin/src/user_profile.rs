use std::{fmt, future::Future, pin::Pin, time::Duration};

use af_account::{PlainSystemSecret, SystemSecretCipher, SystemSecretKind};
use af_db::{
    BalanceAlertSettingsRecord, BalanceAlertSettingsRepository,
    BalanceAlertSettingsRepositoryError, PasskeyRepository, PasskeyRepositoryError,
    PasskeyRevokeOutcome, UserNotificationPreferencesRecord, UserPasswordChangeOutcome,
    UserProfileLookupOutcome, UserProfileMutationOutcome, UserProfileRecord, UserProfileRepository,
    UserProfileRepositoryError, UserTwoFactorMutationOutcome,
};
use af_domain::{Quota, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use thiserror::Error;
use webauthn_rs::prelude::{
    Credential, CredentialID, PasskeyRegistration, RegisterPublicKeyCredential, Uuid, Webauthn,
};
use zeroize::Zeroizing;

use crate::{
    EmailBindingError, EmailBindingIssued, EmailBindingService, PasskeyRegistrationCommand,
    PasskeyRegistrationOptions, PasskeyRenameCommand, PasskeyRevokeCommand, SessionPrincipal,
    SessionRole, TwoFactorEnrollment, UserPasskey, two_factor,
};

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
    subscription_alert_enabled: bool,
    subscription_remaining_percent: i16,
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
        subscription_alert_enabled: bool,
        subscription_remaining_percent: i16,
    ) -> Self {
        Self {
            email_product_updates,
            email_usage_alerts,
            balance_alert_enabled,
            balance_alert_threshold,
            effective_balance_alert_threshold,
            subscription_alert_enabled,
            subscription_remaining_percent,
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

    /// 系统是否启用订阅窗口剩余额度预警。
    #[must_use]
    pub const fn subscription_alert_enabled(self) -> bool {
        self.subscription_alert_enabled
    }

    /// 返回订阅窗口预警使用的全局剩余额度百分比。
    #[must_use]
    pub const fn subscription_remaining_percent(self) -> i16 {
        self.subscription_remaining_percent
    }
}

/// 更新当前用户登录用户名的命令。
pub struct UserProfileUpdateCommand {
    username: String,
}

impl UserProfileUpdateCommand {
    /// 校验用户名边界。
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

/// 请求向新邮箱发送绑定或换绑验证码。
pub struct UserEmailBindingStartCommand {
    email: String,
}

impl UserEmailBindingStartCommand {
    pub fn new(email: String) -> Result<Self, UserProfileError> {
        let email = crate::email_binding::normalize_binding_email(email)
            .map_err(|_| UserProfileError::InvalidInput)?;
        Ok(Self { email })
    }

    fn email(&self) -> &str {
        &self.email
    }
}

impl fmt::Debug for UserEmailBindingStartCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserEmailBindingStartCommand(<redacted>)")
    }
}

/// 使用新邮箱验证码完成绑定或换绑。
pub struct UserEmailBindingConfirmCommand {
    email: String,
    verification_code: Zeroizing<String>,
}

impl UserEmailBindingConfirmCommand {
    pub fn new(email: String, verification_code: String) -> Result<Self, UserProfileError> {
        let email = crate::email_binding::normalize_binding_email(email)
            .map_err(|_| UserProfileError::InvalidInput)?;
        if verification_code.len() != 6
            || !verification_code.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(UserProfileError::InvalidInput);
        }
        Ok(Self {
            email,
            verification_code: Zeroizing::new(verification_code),
        })
    }

    fn into_parts(self) -> (String, Zeroizing<String>) {
        (self.email, self.verification_code)
    }
}

impl fmt::Debug for UserEmailBindingConfirmCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserEmailBindingConfirmCommand(<redacted>)")
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

/// 当前用户的二次验证状态；不包含 secret、备份码或其他敏感材料。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserTwoFactorStatus {
    enabled: bool,
}

impl UserTwoFactorStatus {
    /// 组合二次验证状态快照。
    #[must_use]
    pub const fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// 当前账户是否已启用 TOTP。
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }
}

/// 启用 TOTP 所需的当前密码。
pub struct UserTwoFactorEnableCommand {
    current_password: Zeroizing<String>,
}

impl UserTwoFactorEnableCommand {
    /// 校验当前密码输入边界并放入可清零容器。
    pub fn new(current_password: String) -> Result<Self, UserProfileError> {
        if current_password.is_empty()
            || current_password.len() > 4_096
            || current_password.chars().any(char::is_control)
        {
            return Err(UserProfileError::InvalidInput);
        }
        Ok(Self {
            current_password: Zeroizing::new(current_password),
        })
    }

    fn into_password(self) -> Zeroizing<String> {
        self.current_password
    }
}

impl fmt::Debug for UserTwoFactorEnableCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTwoFactorEnableCommand(<redacted>)")
    }
}

/// 停用 TOTP 与启用使用同一份当前密码边界。
pub type UserTwoFactorDisableCommand = UserTwoFactorEnableCommand;

/// 高风险账户操作所需的当前密码与可选二次验证码。
pub struct UserSecurityStepUpCommand {
    current_password: Zeroizing<String>,
    totp_code: Option<Zeroizing<String>>,
}

impl UserSecurityStepUpCommand {
    /// 校验密码和可选 TOTP/备份码边界，并在内存中使用可清零容器保存。
    pub fn new(
        current_password: String,
        totp_code: Option<String>,
    ) -> Result<Self, UserProfileError> {
        if current_password.is_empty()
            || current_password.len() > 4_096
            || current_password.chars().any(char::is_control)
            || totp_code.as_deref().is_some_and(|code| {
                code.is_empty() || code.len() > 128 || code.chars().any(char::is_control)
            })
        {
            return Err(UserProfileError::InvalidInput);
        }
        Ok(Self {
            current_password: Zeroizing::new(current_password),
            totp_code: totp_code.map(Zeroizing::new),
        })
    }

    fn into_parts(self) -> (Zeroizing<String>, Option<Zeroizing<String>>) {
        (self.current_password, self.totp_code)
    }
}

impl fmt::Debug for UserSecurityStepUpCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserSecurityStepUpCommand(<redacted>)")
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
    /// TOTP 已经启用，必须先停用后重新绑定。
    #[error("二次验证已经启用")]
    TwoFactorAlreadyEnabled,
    /// TOTP 尚未启用，不能重复停用。
    #[error("二次验证尚未启用")]
    TwoFactorNotEnabled,
    /// Passkey 撤销要求账户当前的二次验证因素。
    #[error("Passkey 撤销需要二次验证")]
    TwoFactorRequired,
    /// Passkey 注册或撤销时的二次验证码无效。
    #[error("Passkey 二次验证无效")]
    TwoFactorInvalid,
    /// WebAuthn 注册状态或浏览器响应验证失败。
    #[error("Passkey 注册验证失败")]
    PasskeyRegistrationRejected,
    /// Passkey 不存在或已经撤销。
    #[error("Passkey 不存在")]
    PasskeyNotFound,
    /// 用户名与其他有效账户冲突。
    #[error("个人资料用户名冲突")]
    Conflict,
    /// 数据库或持久化状态失败。
    #[error("个人资料内部失败")]
    Internal,
    /// 新邮箱验证码无效或已过期。
    #[error("邮箱绑定验证码无效")]
    EmailVerificationRejected,
    /// 当前实例尚未配置可用邮件投递。
    #[error("邮箱服务尚未配置")]
    EmailNotConfigured,
    /// 新邮箱验证码投递失败。
    #[error("邮箱验证码投递失败")]
    EmailDeliveryFailed,
    /// 邮箱验证码发送过于频繁。
    #[error("邮箱验证码发送过于频繁")]
    EmailVerificationRateLimited { retry_after_seconds: u64 },
}

/// 读取当前用户资料的对象安全 Future。
pub type UserProfileReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserProfile, UserProfileError>> + Send + 'a>>;

/// 更新当前用户资料的对象安全 Future。
pub type UserProfileUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserProfile, UserProfileError>> + Send + 'a>>;

pub type UserEmailBindingStartFuture<'a> =
    Pin<Box<dyn Future<Output = Result<EmailBindingIssued, UserProfileError>> + Send + 'a>>;
pub type UserEmailBindingConfirmFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserProfile, UserProfileError>> + Send + 'a>>;

/// 修改当前用户密码的对象安全 Future。
pub type UserPasswordChangeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), UserProfileError>> + Send + 'a>>;

/// 更新当前用户通知偏好的对象安全 Future。
pub type UserNotificationUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserProfile, UserProfileError>> + Send + 'a>>;

/// 读取二次验证状态的对象安全 Future。
pub type UserTwoFactorReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserTwoFactorStatus, UserProfileError>> + Send + 'a>>;

/// 启用二次验证并返回一次性入网材料的对象安全 Future。
pub type UserTwoFactorEnableFuture<'a> =
    Pin<Box<dyn Future<Output = Result<TwoFactorEnrollment, UserProfileError>> + Send + 'a>>;

/// 停用二次验证的对象安全 Future。
pub type UserTwoFactorDisableFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), UserProfileError>> + Send + 'a>>;

/// 执行当前账户二次验证的对象安全 Future。
pub type UserSecurityStepUpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), UserProfileError>> + Send + 'a>>;

/// 当前用户 Passkey 目录查询 Future。
pub type UserPasskeyListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<UserPasskey>, UserProfileError>> + Send + 'a>>;

/// Passkey 注册选项 Future。
pub type UserPasskeyRegistrationOptionsFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PasskeyRegistrationOptions, UserProfileError>> + Send + 'a>>;

/// Passkey 注册完成 Future。
pub type UserPasskeyRegistrationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserPasskey, UserProfileError>> + Send + 'a>>;

/// Passkey 重命名 Future。
pub type UserPasskeyRenameFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserPasskey, UserProfileError>> + Send + 'a>>;

/// Passkey 撤销 Future。
pub type UserPasskeyRevokeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), UserProfileError>> + Send + 'a>>;

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

    /// 向待绑定邮箱发送一次性验证码。
    fn send_email_binding_verification<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: UserEmailBindingStartCommand,
    ) -> UserEmailBindingStartFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    /// 消费验证码并写入新邮箱。
    fn confirm_email_binding<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: UserEmailBindingConfirmCommand,
    ) -> UserEmailBindingConfirmFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

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

    /// 读取当前用户的 TOTP 状态；默认实现保持旧测试适配器失败关闭。
    fn get_two_factor<'a>(&'a self, _principal: SessionPrincipal) -> UserTwoFactorReadFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    /// 校验当前密码并启用 TOTP；默认实现保持旧测试适配器失败关闭。
    fn enable_two_factor<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: UserTwoFactorEnableCommand,
    ) -> UserTwoFactorEnableFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    /// 校验当前密码并停用 TOTP；默认实现保持旧测试适配器失败关闭。
    fn disable_two_factor<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: UserTwoFactorDisableCommand,
    ) -> UserTwoFactorDisableFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    /// 用当前密码和已启用的 TOTP/备份码确认高风险账户操作。
    fn step_up<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: UserSecurityStepUpCommand,
    ) -> UserSecurityStepUpFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    /// 查询当前用户 Passkey 脱敏目录。
    fn list_passkeys<'a>(&'a self, _principal: SessionPrincipal) -> UserPasskeyListFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    /// 创建当前用户的 WebAuthn 注册选项。
    fn start_passkey_registration<'a>(
        &'a self,
        _principal: SessionPrincipal,
    ) -> UserPasskeyRegistrationOptionsFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    /// 验证浏览器注册响应并保存凭证。
    fn finish_passkey_registration<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: PasskeyRegistrationCommand,
    ) -> UserPasskeyRegistrationFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    /// 修改当前用户凭证的展示名称。
    fn rename_passkey<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _passkey_id: i64,
        _command: PasskeyRenameCommand,
    ) -> UserPasskeyRenameFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    /// 通过密码和必要的 TOTP/备份码撤销当前用户凭证。
    fn revoke_passkey<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _passkey_id: i64,
        _command: PasskeyRevokeCommand,
    ) -> UserPasskeyRevokeFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }
}

/// 使用用户资料数据库仓储的生产应用服务。
pub struct DatabaseUserProfileService {
    repository: UserProfileRepository,
    balance_alert_settings: BalanceAlertSettingsRepository,
    system_secret_cipher: Option<SystemSecretCipher>,
    passkey_repository: Option<PasskeyRepository>,
    webauthn: Option<Webauthn>,
    email_binding: Option<EmailBindingService>,
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
            system_secret_cipher: None,
            passkey_repository: None,
            webauthn: None,
            email_binding: None,
        }
    }

    /// 注入启动期已校验的系统密钥，用于 TOTP 配置加密。
    #[must_use]
    pub fn with_system_secret_cipher(mut self, cipher: SystemSecretCipher) -> Self {
        self.system_secret_cipher = Some(cipher);
        self
    }

    /// 注入 Passkey 数据仓储；即使 WebAuthn 运行时暂未启用，也应允许读取和管理已有凭证。
    #[must_use]
    pub fn with_passkey_repository(mut self, repository: PasskeyRepository) -> Self {
        self.passkey_repository = Some(repository);
        self
    }

    /// 注入邮箱绑定验证码服务；未配置邮件或安全密钥时保持失败关闭。
    #[must_use]
    pub fn with_email_binding(mut self, service: EmailBindingService) -> Self {
        self.email_binding = Some(service);
        self
    }

    /// 注入可选邮箱绑定服务；邮件服务未完成配置时保持功能关闭。
    #[must_use]
    pub fn with_email_binding_option(mut self, service: Option<EmailBindingService>) -> Self {
        self.email_binding = service;
        self
    }

    /// 注入 Passkey 仓储和已由可信 Origin 构造的 WebAuthn 运行时。
    #[must_use]
    pub fn with_passkey_runtime(
        mut self,
        repository: PasskeyRepository,
        webauthn: Webauthn,
    ) -> Self {
        self.passkey_repository = Some(repository);
        self.webauthn = Some(webauthn);
        self
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

    fn send_email_binding_verification<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserEmailBindingStartCommand,
    ) -> UserEmailBindingStartFuture<'a> {
        Box::pin(async move {
            let service = self
                .email_binding
                .as_ref()
                .ok_or(UserProfileError::Internal)?;
            service
                .issue(principal.user_id(), command.email())
                .await
                .map_err(map_email_binding_error)
        })
    }

    fn confirm_email_binding<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserEmailBindingConfirmCommand,
    ) -> UserEmailBindingConfirmFuture<'a> {
        Box::pin(async move {
            let service = self
                .email_binding
                .as_ref()
                .ok_or(UserProfileError::Internal)?;
            let (email, verification_code) = command.into_parts();
            service
                .consume(principal.user_id(), &email, verification_code.as_str())
                .await
                .map_err(map_email_binding_error)?;
            let settings = self.balance_alert_settings().await?;
            match self
                .repository
                .update_email(principal.user_id(), email)
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

    fn get_two_factor<'a>(&'a self, principal: SessionPrincipal) -> UserTwoFactorReadFuture<'a> {
        Box::pin(async move {
            self.repository
                .two_factor_enabled(principal.user_id())
                .await
                .map_err(map_repository_error)?
                .map(UserTwoFactorStatus::new)
                .ok_or(UserProfileError::InvalidSession)
        })
    }

    fn enable_two_factor<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserTwoFactorEnableCommand,
    ) -> UserTwoFactorEnableFuture<'a> {
        Box::pin(async move {
            let cipher = self
                .system_secret_cipher
                .as_ref()
                .ok_or(UserProfileError::Internal)?;
            let (encrypted, enrollment) =
                two_factor::prepare_enrollment(cipher, principal.user_id())
                    .map_err(|_| UserProfileError::Internal)?;
            let current_password = command.into_password();
            match self
                .repository
                .enable_two_factor(principal.user_id(), current_password.as_bytes(), encrypted)
                .await
                .map_err(map_repository_error)?
            {
                UserTwoFactorMutationOutcome::Updated => Ok(enrollment),
                UserTwoFactorMutationOutcome::NotFound => Err(UserProfileError::InvalidSession),
                UserTwoFactorMutationOutcome::Rejected => {
                    Err(UserProfileError::CurrentPasswordInvalid)
                }
                UserTwoFactorMutationOutcome::AlreadyEnabled => {
                    Err(UserProfileError::TwoFactorAlreadyEnabled)
                }
                UserTwoFactorMutationOutcome::NotEnabled => Err(UserProfileError::Internal),
            }
        })
    }

    fn disable_two_factor<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserTwoFactorDisableCommand,
    ) -> UserTwoFactorDisableFuture<'a> {
        Box::pin(async move {
            let current_password = command.into_password();
            match self
                .repository
                .disable_two_factor(principal.user_id(), current_password.as_bytes())
                .await
                .map_err(map_repository_error)?
            {
                UserTwoFactorMutationOutcome::Updated => Ok(()),
                UserTwoFactorMutationOutcome::NotFound => Err(UserProfileError::InvalidSession),
                UserTwoFactorMutationOutcome::Rejected => {
                    Err(UserProfileError::CurrentPasswordInvalid)
                }
                UserTwoFactorMutationOutcome::NotEnabled => {
                    Err(UserProfileError::TwoFactorNotEnabled)
                }
                UserTwoFactorMutationOutcome::AlreadyEnabled => Err(UserProfileError::Internal),
            }
        })
    }

    fn step_up<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserSecurityStepUpCommand,
    ) -> UserSecurityStepUpFuture<'a> {
        Box::pin(async move {
            let (current_password, totp_code) = command.into_parts();
            self.verify_security_step_up(
                principal,
                current_password.as_bytes(),
                totp_code.as_ref().map(|code| code.as_str()),
            )
            .await
        })
    }

    fn list_passkeys<'a>(&'a self, principal: SessionPrincipal) -> UserPasskeyListFuture<'a> {
        Box::pin(async move {
            let repository = self
                .passkey_repository
                .as_ref()
                .ok_or(UserProfileError::Internal)?;
            repository
                .list(principal.user_id())
                .await
                .map_err(map_passkey_repository_error)
                .map(|records| records.into_iter().map(passkey_from_record).collect())
        })
    }

    fn start_passkey_registration<'a>(
        &'a self,
        principal: SessionPrincipal,
    ) -> UserPasskeyRegistrationOptionsFuture<'a> {
        Box::pin(async move {
            let repository = self
                .passkey_repository
                .as_ref()
                .ok_or(UserProfileError::Internal)?;
            let webauthn = self.webauthn.as_ref().ok_or(UserProfileError::Internal)?;
            let cipher = self
                .system_secret_cipher
                .as_ref()
                .ok_or(UserProfileError::Internal)?;
            let profile = match self
                .repository
                .get(principal.user_id())
                .await
                .map_err(map_repository_error)?
            {
                UserProfileLookupOutcome::Found(profile) => profile,
                UserProfileLookupOutcome::NotFound => return Err(UserProfileError::InvalidSession),
            };
            let exclude_credentials = repository
                .active_credential_ids(principal.user_id())
                .await
                .map_err(map_passkey_repository_error)?
                .into_iter()
                .map(|value| {
                    URL_SAFE_NO_PAD
                        .decode(value)
                        .map(CredentialID::from)
                        .map_err(|_| UserProfileError::Internal)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let (creation_options, state) = webauthn
                .start_passkey_registration(
                    passkey_user_uuid(principal.user_id()),
                    profile.username(),
                    profile.username(),
                    Some(exclude_credentials),
                )
                .map_err(|_| UserProfileError::PasskeyRegistrationRejected)?;
            let serialized_state = serde_json::to_string(&state)
                .map_err(|_| UserProfileError::PasskeyRegistrationRejected)?;
            let plaintext = PlainSystemSecret::new(serialized_state)
                .map_err(|_| UserProfileError::PasskeyRegistrationRejected)?;
            let encrypted = cipher
                .encrypt(
                    SystemSecretKind::PasskeyRegistrationState(principal.user_id()),
                    &plaintext,
                )
                .map_err(|_| UserProfileError::Internal)?;
            let now = af_db::DatabaseTimestamp::now_utc();
            let challenge_digest = challenge_digest(creation_options.public_key.challenge.as_ref());
            repository
                .replace_registration_challenge(
                    principal.user_id(),
                    challenge_digest.clone(),
                    encrypted,
                    now + Duration::from_secs(5 * 60),
                    now,
                )
                .await
                .map_err(map_passkey_repository_error)?;
            let options = serde_json::to_value(creation_options)
                .map_err(|_| UserProfileError::PasskeyRegistrationRejected)?;
            Ok(PasskeyRegistrationOptions::new(options, challenge_digest))
        })
    }

    fn finish_passkey_registration<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: PasskeyRegistrationCommand,
    ) -> UserPasskeyRegistrationFuture<'a> {
        Box::pin(async move {
            let repository = self
                .passkey_repository
                .as_ref()
                .ok_or(UserProfileError::Internal)?;
            let webauthn = self.webauthn.as_ref().ok_or(UserProfileError::Internal)?;
            let cipher = self
                .system_secret_cipher
                .as_ref()
                .ok_or(UserProfileError::Internal)?;
            let (credential_json, display_name) = command.into_parts();
            let credential: RegisterPublicKeyCredential =
                serde_json::from_value(credential_json)
                    .map_err(|_| UserProfileError::PasskeyRegistrationRejected)?;
            let digest =
                registration_response_digest(credential.response.client_data_json.as_ref())
                    .ok_or(UserProfileError::PasskeyRegistrationRejected)?;
            let challenge = repository
                .load_registration_challenge(
                    principal.user_id(),
                    &digest,
                    af_db::DatabaseTimestamp::now_utc(),
                )
                .await
                .map_err(map_passkey_repository_error)?;
            let plaintext = cipher
                .decrypt(
                    SystemSecretKind::PasskeyRegistrationState(principal.user_id()),
                    challenge.state(),
                )
                .map_err(|_| UserProfileError::PasskeyRegistrationRejected)?;
            let state: PasskeyRegistration = serde_json::from_str(plaintext.expose_secret())
                .map_err(|_| UserProfileError::PasskeyRegistrationRejected)?;
            let passkey = webauthn
                .finish_passkey_registration(&credential, &state)
                .map_err(|_| UserProfileError::PasskeyRegistrationRejected)?;
            let sign_count = i64::from(Credential::from(passkey.clone()).counter);
            let credential_id = URL_SAFE_NO_PAD.encode(passkey.cred_id().as_ref());
            let passkey_json = serde_json::to_value(passkey)
                .map_err(|_| UserProfileError::PasskeyRegistrationRejected)?;
            let record = repository
                .consume_registration_challenge(
                    principal.user_id(),
                    &digest,
                    credential_id,
                    passkey_json,
                    display_name,
                    sign_count,
                    af_db::DatabaseTimestamp::now_utc(),
                )
                .await
                .map_err(map_passkey_repository_error)?;
            Ok(passkey_from_record(record))
        })
    }

    fn rename_passkey<'a>(
        &'a self,
        principal: SessionPrincipal,
        passkey_id: i64,
        command: PasskeyRenameCommand,
    ) -> UserPasskeyRenameFuture<'a> {
        Box::pin(async move {
            let repository = self
                .passkey_repository
                .as_ref()
                .ok_or(UserProfileError::Internal)?;
            repository
                .rename(principal.user_id(), passkey_id, command.into_display_name())
                .await
                .map_err(map_passkey_repository_error)?
                .map(passkey_from_record)
                .ok_or(UserProfileError::PasskeyNotFound)
        })
    }

    fn revoke_passkey<'a>(
        &'a self,
        principal: SessionPrincipal,
        passkey_id: i64,
        command: PasskeyRevokeCommand,
    ) -> UserPasskeyRevokeFuture<'a> {
        Box::pin(async move {
            let repository = self
                .passkey_repository
                .as_ref()
                .ok_or(UserProfileError::Internal)?;
            let (current_password, totp_code) = command.into_parts();
            self.verify_security_step_up(
                principal,
                current_password.as_bytes(),
                totp_code.as_ref().map(|code| code.as_str()),
            )
            .await?;
            match repository
                .revoke(
                    principal.user_id(),
                    passkey_id,
                    current_password.as_bytes(),
                    af_db::DatabaseTimestamp::now_utc(),
                )
                .await
                .map_err(map_passkey_repository_error)?
            {
                PasskeyRevokeOutcome::Revoked => Ok(()),
                PasskeyRevokeOutcome::NotFound | PasskeyRevokeOutcome::AlreadyRevoked => {
                    Err(UserProfileError::PasskeyNotFound)
                }
                PasskeyRevokeOutcome::PasswordRejected => {
                    Err(UserProfileError::CurrentPasswordInvalid)
                }
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

    /// 在密码确认后校验已启用 TOTP；备份码只在密码正确时原子消费。
    async fn verify_security_step_up(
        &self,
        principal: SessionPrincipal,
        current_password: &[u8],
        totp_code: Option<&str>,
    ) -> Result<(), UserProfileError> {
        let repository = self
            .passkey_repository
            .as_ref()
            .ok_or(UserProfileError::Internal)?;
        let factors = repository
            .security_factors(principal.user_id())
            .await
            .map_err(map_passkey_repository_error)?
            .ok_or(UserProfileError::InvalidSession)?;
        if !factors.verify_password(current_password) {
            return Err(UserProfileError::CurrentPasswordInvalid);
        }
        let Some(totp_secret) = factors.totp_secret() else {
            return Ok(());
        };
        let code = totp_code.ok_or(UserProfileError::TwoFactorRequired)?;
        let cipher = self
            .system_secret_cipher
            .as_ref()
            .ok_or(UserProfileError::Internal)?;
        match two_factor::verify_login_code(
            cipher,
            principal.user_id(),
            totp_secret,
            code,
            unix_now_seconds(),
        )
        .map_err(|_| UserProfileError::TwoFactorInvalid)?
        {
            two_factor::LoginFactorResult::Totp => Ok(()),
            two_factor::LoginFactorResult::Backup { replacement } => {
                let consumed = repository
                    .consume_totp_backup_code(principal.user_id(), totp_secret.clone(), replacement)
                    .await
                    .map_err(map_passkey_repository_error)?;
                if consumed {
                    Ok(())
                } else {
                    Err(UserProfileError::TwoFactorInvalid)
                }
            }
        }
    }
}

fn passkey_from_record(record: af_db::PasskeyRecord) -> UserPasskey {
    UserPasskey::new(
        record.id(),
        record.display_name().to_owned(),
        record.created_at().unix_timestamp(),
        record.last_used_at().map(|value| value.unix_timestamp()),
        record.revoked_at().map(|value| value.unix_timestamp()),
    )
}

fn map_passkey_repository_error(error: PasskeyRepositoryError) -> UserProfileError {
    match error {
        PasskeyRepositoryError::NotFound
        | PasskeyRepositoryError::Expired
        | PasskeyRepositoryError::Consumed
        | PasskeyRepositoryError::Conflict
        | PasskeyRepositoryError::InvalidConfiguration
        | PasskeyRepositoryError::Query
        | PasskeyRepositoryError::Timeout
        | PasskeyRepositoryError::Invariant => UserProfileError::Internal,
    }
}

fn map_email_binding_error(error: EmailBindingError) -> UserProfileError {
    match error {
        EmailBindingError::InvalidInput => UserProfileError::InvalidInput,
        EmailBindingError::Rejected => UserProfileError::EmailVerificationRejected,
        EmailBindingError::EmailNotConfigured => UserProfileError::EmailNotConfigured,
        EmailBindingError::DeliveryFailed => UserProfileError::EmailDeliveryFailed,
        EmailBindingError::RateLimited {
            retry_after_seconds,
        } => UserProfileError::EmailVerificationRateLimited {
            retry_after_seconds,
        },
        EmailBindingError::Internal => UserProfileError::Internal,
    }
}

fn passkey_user_uuid(user_id: UserId) -> Uuid {
    let digest = Sha256::digest(format!("anyflows:passkey:user:{}", user_id.get()).as_bytes());
    Uuid::from_bytes(digest[..16].try_into().expect("SHA-256 截取必须为 16 字节"))
}

fn challenge_digest(challenge: &[u8]) -> String {
    Sha256::digest(challenge)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn registration_response_digest(client_data_json: &[u8]) -> Option<String> {
    let client_data: serde_json::Value = serde_json::from_slice(client_data_json).ok()?;
    if client_data.get("type")?.as_str()? != "webauthn.create" {
        return None;
    }
    let challenge = client_data.get("challenge")?.as_str()?;
    let challenge = URL_SAFE_NO_PAD.decode(challenge).ok()?;
    Some(challenge_digest(&challenge))
}

fn unix_now_seconds() -> u64 {
    u64::try_from(af_db::DatabaseTimestamp::now_utc().unix_timestamp()).unwrap_or_default()
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
            settings.subscription_alert_enabled(),
            settings.subscription_remaining_percent(),
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
        let step_up = UserSecurityStepUpCommand::new(
            "current secure password".to_owned(),
            Some("123456".to_owned()),
        )
        .unwrap();
        assert_eq!(
            format!("{step_up:?}"),
            "UserSecurityStepUpCommand(<redacted>)"
        );
        assert!(!format!("{step_up:?}").contains("123456"));
        assert_eq!(
            UserSecurityStepUpCommand::new("".to_owned(), None).unwrap_err(),
            UserProfileError::InvalidInput
        );
        assert_eq!(
            UserSecurityStepUpCommand::new("current".to_owned(), Some("\n".to_owned()))
                .unwrap_err(),
            UserProfileError::InvalidInput
        );
        assert!(UserNotificationPreferencesCommand::new(false, true, None).is_ok());
        assert_eq!(
            UserNotificationPreferencesCommand::new(false, true, Some(Quota::ZERO)).unwrap_err(),
            UserProfileError::InvalidInput
        );
    }
}
