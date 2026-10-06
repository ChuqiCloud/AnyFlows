use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_account::SystemSecretCipher;
use af_db::{
    AdminUserCreateRecord, AdminUserRegistrationCreateOutcome, AdminUserRepository,
    AdminUserRepositoryError, AuthChallengeRateLimitRepository, AuthChallengeRepository,
    EmailSettingsRepository, MAX_REGISTRATION_RATE_LIMIT_ATTEMPTS,
    MAX_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS, MIN_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS,
    RegistrationAttemptOutcome, RegistrationPolicyRecord, RegistrationPolicyWriteRecord,
    RegistrationRepository, RegistrationRepositoryError,
};
use af_domain::{GroupId, Quota, TrustedClientIp, UserId};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{
    AdminUserStatus, EmailDelivery, LoginCredentials, SessionPrincipal, SessionRole,
    registration_verification::{REGISTRATION_EMAIL_CODE_BYTES, RegistrationEmailVerification},
};

/// 公开注册密码的最小 UTF-8 字节数。
pub const MIN_REGISTRATION_PASSWORD_BYTES: usize = 12;
/// 公开注册密码的最大 UTF-8 字节数。
pub const MAX_REGISTRATION_PASSWORD_BYTES: usize = 128;

/// 游客可读取的最小注册能力状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationStatus {
    password_login_enabled: bool,
    enabled: bool,
    email_required: bool,
}

impl RegistrationStatus {
    /// 组合已完成业务校验的公开能力状态，供端口适配器与测试实现使用。
    #[must_use]
    pub const fn from_parts(
        password_login_enabled: bool,
        enabled: bool,
        email_required: bool,
    ) -> Self {
        Self {
            password_login_enabled,
            enabled,
            email_required,
        }
    }

    /// 返回用户名密码登录是否可用。
    #[must_use]
    pub const fn password_login_enabled(self) -> bool {
        self.password_login_enabled
    }

    /// 返回公开注册是否可用。
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// 返回注册时是否必须填写邮箱。
    #[must_use]
    pub const fn email_required(self) -> bool {
        self.email_required
    }
}

/// 管理员可读取的完整注册策略。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationPolicy {
    password_login_enabled: bool,
    enabled: bool,
    default_group_id: GroupId,
    initial_quota: i64,
    invitation_rebate_quota: i64,
    email_required: bool,
    rate_limit_attempts: u32,
    rate_limit_window_seconds: u64,
    version: i64,
}

impl RegistrationPolicy {
    /// 组合已完成业务校验的完整策略，供端口适配器与测试实现使用。
    #[allow(clippy::too_many_arguments, reason = "字段与注册策略契约一一对应")]
    #[must_use]
    pub const fn from_parts(
        password_login_enabled: bool,
        enabled: bool,
        default_group_id: GroupId,
        initial_quota: i64,
        invitation_rebate_quota: i64,
        email_required: bool,
        rate_limit_attempts: u32,
        rate_limit_window_seconds: u64,
        version: i64,
    ) -> Self {
        Self {
            password_login_enabled,
            enabled,
            default_group_id,
            initial_quota,
            invitation_rebate_quota,
            email_required,
            rate_limit_attempts,
            rate_limit_window_seconds,
            version,
        }
    }

    /// 返回用户名密码登录是否可用。
    #[must_use]
    pub const fn password_login_enabled(self) -> bool {
        self.password_login_enabled
    }

    /// 返回公开注册是否启用。
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// 返回新用户绑定的默认分组。
    #[must_use]
    pub const fn default_group_id(self) -> GroupId {
        self.default_group_id
    }

    /// 返回新用户初始额度。
    #[must_use]
    pub const fn initial_quota(self) -> i64 {
        self.initial_quota
    }

    /// 返回每个有效邀请码注册触发的非负返利额度。
    #[must_use]
    pub const fn invitation_rebate_quota(self) -> i64 {
        self.invitation_rebate_quota
    }

    /// 返回注册时是否必须填写邮箱。
    #[must_use]
    pub const fn email_required(self) -> bool {
        self.email_required
    }

    /// 返回固定窗口允许的尝试次数。
    #[must_use]
    pub const fn rate_limit_attempts(self) -> u32 {
        self.rate_limit_attempts
    }

    /// 返回固定窗口秒数。
    #[must_use]
    pub const fn rate_limit_window_seconds(self) -> u64 {
        self.rate_limit_window_seconds
    }

    /// 返回认证设置的单调版本。
    #[must_use]
    pub const fn version(self) -> i64 {
        self.version
    }
}

impl From<RegistrationPolicyRecord> for RegistrationPolicy {
    fn from(record: RegistrationPolicyRecord) -> Self {
        Self {
            password_login_enabled: record.password_login_enabled(),
            enabled: record.enabled(),
            default_group_id: record.default_group_id(),
            initial_quota: record.initial_quota(),
            invitation_rebate_quota: record.invitation_rebate_quota(),
            email_required: record.email_required(),
            rate_limit_attempts: record.rate_limit_attempts(),
            rate_limit_window_seconds: record.rate_limit_window_seconds(),
            version: record.version(),
        }
    }
}

/// 管理员覆盖注册策略时提交的完整命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationPolicyCommand {
    password_login_enabled: bool,
    enabled: bool,
    default_group_id: GroupId,
    initial_quota: i64,
    invitation_rebate_quota: i64,
    email_required: bool,
    rate_limit_attempts: u32,
    rate_limit_window_seconds: u64,
}

impl RegistrationPolicyCommand {
    /// 校验非负额度和固定窗口硬边界。
    #[allow(clippy::too_many_arguments, reason = "字段与注册策略契约一一对应")]
    pub fn new(
        password_login_enabled: bool,
        enabled: bool,
        default_group_id: GroupId,
        initial_quota: i64,
        invitation_rebate_quota: i64,
        email_required: bool,
        rate_limit_attempts: u32,
        rate_limit_window_seconds: u64,
    ) -> Result<Self, RegistrationError> {
        if (enabled && !password_login_enabled)
            || initial_quota < 0
            || invitation_rebate_quota < 0
            || !(1..=MAX_REGISTRATION_RATE_LIMIT_ATTEMPTS).contains(&rate_limit_attempts)
            || !(MIN_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS
                ..=MAX_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS)
                .contains(&rate_limit_window_seconds)
        {
            return Err(RegistrationError::InvalidInput);
        }
        Ok(Self {
            password_login_enabled,
            enabled,
            default_group_id,
            initial_quota,
            invitation_rebate_quota,
            email_required,
            rate_limit_attempts,
            rate_limit_window_seconds,
        })
    }

    fn into_record(self) -> RegistrationPolicyWriteRecord {
        RegistrationPolicyWriteRecord::new(
            self.password_login_enabled,
            self.enabled,
            self.default_group_id,
            self.initial_quota,
            self.invitation_rebate_quota,
            self.email_required,
            self.rate_limit_attempts,
            self.rate_limit_window_seconds,
        )
    }
}

/// 已完成公开边界校验的注册输入；调试输出始终隐藏全部身份字段。
pub struct RegistrationCommand {
    username: String,
    email: Option<String>,
    password: String,
    verification_code: Option<Zeroizing<String>>,
    invite_code: Option<String>,
}

impl RegistrationCommand {
    /// 校验用户名、邮箱形状和密码强度边界。
    pub fn new(
        username: String,
        email: Option<String>,
        password: String,
        verification_code: Option<String>,
        invite_code: Option<String>,
    ) -> Result<Self, RegistrationError> {
        validate_username(&username)?;
        let email = email.map(normalize_email).transpose()?;
        validate_password(&password)?;
        let verification_code = match (email.as_ref(), verification_code) {
            (Some(_), Some(code)) if is_valid_verification_code(&code) => {
                Some(Zeroizing::new(code))
            }
            (None, None) => None,
            _ => return Err(RegistrationError::InvalidInput),
        };
        let invite_code = invite_code
            .map(|code| {
                if is_valid_invitation_code(&code) {
                    Ok(code)
                } else {
                    Err(RegistrationError::InvalidInput)
                }
            })
            .transpose()?;
        Ok(Self {
            username,
            email,
            password,
            verification_code,
            invite_code,
        })
    }

    fn user_record(&self, policy: RegistrationPolicyRecord) -> AdminUserCreateRecord {
        AdminUserCreateRecord::new(
            self.username.clone(),
            self.email.clone(),
            Some(self.password.clone()),
            SessionRole::User.to_database(),
            AdminUserStatus::Enabled.to_database(),
            policy.default_group_id(),
            policy.initial_quota(),
            None,
            None,
        )
    }

    /// 注册成功后消费原始凭据，交给统一会话认证器完成自动登录。
    #[must_use]
    pub fn into_login_credentials(self) -> LoginCredentials {
        LoginCredentials::new(self.username, self.password)
    }
}

impl fmt::Debug for RegistrationCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RegistrationCommand(<redacted>)")
    }
}

/// 公开发码入口已经完成规范化的邮箱命令。
pub struct RegistrationEmailVerificationCommand {
    email: String,
}

impl RegistrationEmailVerificationCommand {
    /// 校验并规范化邮箱；调试输出不会暴露邮箱内容。
    pub fn new(email: String) -> Result<Self, RegistrationError> {
        Ok(Self {
            email: normalize_email(email)?,
        })
    }
}

impl fmt::Debug for RegistrationEmailVerificationCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RegistrationEmailVerificationCommand(<redacted>)")
    }
}

/// 注册验证码成功投递后返回的服务端时间边界。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationEmailVerificationResult {
    expires_at: u64,
    next_send_at: u64,
}

impl RegistrationEmailVerificationResult {
    /// 为端口适配器与替代实现组装已经校验的服务端时间边界。
    #[must_use]
    pub const fn from_parts(expires_at: u64, next_send_at: u64) -> Self {
        Self {
            expires_at,
            next_send_at,
        }
    }

    /// 返回当前验证码失效的 Unix 秒时间戳。
    #[must_use]
    pub const fn expires_at(self) -> u64 {
        self.expires_at
    }

    /// 返回服务端允许同一主体再次发送的 Unix 秒时间戳。
    #[must_use]
    pub const fn next_send_at(self) -> u64 {
        self.next_send_at
    }
}

/// 注册成功结果，只携带新普通用户的稳定标识。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationResult {
    user_id: UserId,
}

impl RegistrationResult {
    /// 从已持久化用户标识构造注册结果。
    #[must_use]
    pub const fn from_user_id(user_id: UserId) -> Self {
        Self { user_id }
    }

    /// 返回新注册用户标识。
    #[must_use]
    pub const fn user_id(self) -> UserId {
        self.user_id
    }
}

/// 注册服务构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RegistrationServiceConfigError {
    /// 缺少用于派生注册指纹和挑战摘要的应用密钥。
    #[error("注册安全派生密钥缺失")]
    MissingSecurityKey,
    /// 注册安全派生密钥不是规范的 32 字节 Base64URL 文本。
    #[error("注册安全派生密钥无效")]
    InvalidSecurityKey,
}

/// 公开注册与管理员策略操作的闭合错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RegistrationError {
    /// 输入字段或策略字段无效。
    #[error("注册输入无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("注册策略权限不足")]
    Forbidden,
    /// 用户名密码登录已被管理员关闭。
    #[error("用户名密码登录未启用")]
    LoginDisabled,
    /// 公开注册未启用。
    #[error("公开注册未启用")]
    Disabled,
    /// 用户名或邮箱与现有有效用户冲突。
    #[error("注册身份冲突")]
    Conflict,
    /// 验证码无效或带邮箱的公开身份无法创建；公开边界不得细分原因。
    #[error("注册验证被拒绝")]
    VerificationRejected,
    /// 邀请码不存在、已失效或不能用于当前注册。
    #[error("注册邀请码被拒绝")]
    InvitationRejected,
    /// 当前客户端已耗尽固定窗口尝试次数。
    #[error("注册尝试过于频繁")]
    RateLimited { retry_after_seconds: u64 },
    /// SMTP 设置尚未达到可投递状态。
    #[error("注册邮件未配置")]
    EmailNotConfigured,
    /// SMTP 连接、认证或投递失败。
    #[error("注册邮件投递失败")]
    EmailDeliveryFailed,
    /// 仓储、时钟、随机源或持久化状态发生内部故障。
    #[error("注册内部失败")]
    Internal,
}

/// 公开注册状态查询 Future。
pub type RegistrationStatusFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RegistrationStatus, RegistrationError>> + Send + 'a>>;
/// 公开注册写入 Future。
pub type RegistrationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RegistrationResult, RegistrationError>> + Send + 'a>>;
/// 注册邮箱验证码发送 Future。
pub type RegistrationEmailVerificationFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<RegistrationEmailVerificationResult, RegistrationError>>
            + Send
            + 'a,
    >,
>;
/// 管理员注册策略读取 Future。
pub type RegistrationPolicyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RegistrationPolicy, RegistrationError>> + Send + 'a>>;
/// 管理员注册策略更新 Future。
pub type RegistrationPolicyUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RegistrationPolicy, RegistrationError>> + Send + 'a>>;

/// 公开注册和管理员注册策略应用端口。
pub trait RegistrationService: Send + Sync {
    /// 读取游客可见的最小能力状态。
    fn status(&self) -> RegistrationStatusFuture<'_>;

    /// 受主体与客户端 IP 双作用域限流保护地发送注册邮箱验证码。
    fn send_email_verification<'a>(
        &'a self,
        client_ip: TrustedClientIp,
        command: &'a RegistrationEmailVerificationCommand,
    ) -> RegistrationEmailVerificationFuture<'a>;

    /// 占用限流配额并创建普通用户。
    fn register<'a>(
        &'a self,
        client_ip: TrustedClientIp,
        command: &'a RegistrationCommand,
    ) -> RegistrationFuture<'a>;

    /// 读取管理员完整策略。
    fn policy(&self, principal: SessionPrincipal) -> RegistrationPolicyFuture<'_>;

    /// 原子覆盖管理员完整策略。
    fn update_policy(
        &self,
        principal: SessionPrincipal,
        command: RegistrationPolicyCommand,
    ) -> RegistrationPolicyUpdateFuture<'_>;
}

/// 使用数据库策略、限流仓储和既有用户仓储实现注册闭环。
pub struct DatabaseRegistrationService {
    registration_repository: RegistrationRepository,
    user_repository: AdminUserRepository,
    email_verification: RegistrationEmailVerification,
}

impl DatabaseRegistrationService {
    /// 绑定注册、挑战、邮件与安全派生依赖。
    #[allow(clippy::too_many_arguments, reason = "显式注入独立仓储和邮件投递边界")]
    pub fn new(
        registration_repository: RegistrationRepository,
        user_repository: AdminUserRepository,
        challenge_repository: AuthChallengeRepository,
        rate_limit_repository: AuthChallengeRateLimitRepository,
        email_settings_repository: EmailSettingsRepository,
        system_secret_cipher: SystemSecretCipher,
        delivery: Arc<dyn EmailDelivery>,
        security_key: Option<&str>,
    ) -> Result<Self, RegistrationServiceConfigError> {
        Ok(Self {
            registration_repository,
            user_repository,
            email_verification: RegistrationEmailVerification::new(
                challenge_repository,
                rate_limit_repository,
                email_settings_repository,
                system_secret_cipher,
                delivery,
                security_key,
            )?,
        })
    }
}

impl RegistrationService for DatabaseRegistrationService {
    fn status(&self) -> RegistrationStatusFuture<'_> {
        Box::pin(async move {
            self.registration_repository
                .status()
                .await
                .map(|status| RegistrationStatus {
                    password_login_enabled: status.password_login_enabled(),
                    enabled: status.enabled(),
                    email_required: status.email_required(),
                })
                .map_err(map_registration_repository_error)
        })
    }

    fn send_email_verification<'a>(
        &'a self,
        client_ip: TrustedClientIp,
        command: &'a RegistrationEmailVerificationCommand,
    ) -> RegistrationEmailVerificationFuture<'a> {
        Box::pin(async move {
            let issued_at = current_timestamp().ok_or(RegistrationError::Internal)?;
            let policy = self
                .registration_repository
                .policy()
                .await
                .map_err(map_registration_repository_error)?;
            if !policy.email_required() {
                return Err(RegistrationError::InvalidInput);
            }
            let issued = self
                .email_verification
                .issue(client_ip, &command.email, policy, issued_at)
                .await?;
            Ok(RegistrationEmailVerificationResult {
                expires_at: issued.expires_at(),
                next_send_at: issued.next_send_at(),
            })
        })
    }

    fn register<'a>(
        &'a self,
        client_ip: TrustedClientIp,
        command: &'a RegistrationCommand,
    ) -> RegistrationFuture<'a> {
        Box::pin(async move {
            let attempted_at = current_timestamp().ok_or(RegistrationError::Internal)?;
            let fingerprint = self
                .email_verification
                .registration_ip_fingerprint(client_ip);
            let policy = match self
                .registration_repository
                .claim_attempt(fingerprint, attempted_at)
                .await
                .map_err(map_registration_repository_error)?
            {
                RegistrationAttemptOutcome::Disabled => return Err(RegistrationError::Disabled),
                RegistrationAttemptOutcome::RateLimited {
                    retry_after_seconds,
                } => {
                    return Err(RegistrationError::RateLimited {
                        retry_after_seconds,
                    });
                }
                RegistrationAttemptOutcome::Allowed(policy) => policy,
            };
            if policy.email_required() && command.email.is_none() {
                return Err(RegistrationError::InvalidInput);
            }
            let challenge = if let (Some(email), Some(code)) = (
                command.email.as_deref(),
                command.verification_code.as_deref(),
            ) {
                Some(self.email_verification.consume(email, code, attempted_at)?)
            } else {
                None
            };
            let rebate_quota = Quota::new(policy.invitation_rebate_quota())
                .map_err(|_| RegistrationError::Internal)?;
            let result = self
                .user_repository
                .create_registration(
                    command.user_record(policy),
                    challenge,
                    command.invite_code.as_deref(),
                    rebate_quota,
                    attempted_at,
                )
                .await
                .map_err(|error| {
                    if command.email.is_some() {
                        map_verified_user_repository_error(error)
                    } else {
                        map_user_repository_error(error)
                    }
                })?;
            let user = match result {
                AdminUserRegistrationCreateOutcome::Created(user) => user,
                AdminUserRegistrationCreateOutcome::VerificationRejected => {
                    return Err(RegistrationError::VerificationRejected);
                }
                AdminUserRegistrationCreateOutcome::InvitationRejected => {
                    return Err(RegistrationError::InvitationRejected);
                }
            };
            Ok(RegistrationResult {
                user_id: user.user_id(),
            })
        })
    }

    fn policy(&self, principal: SessionPrincipal) -> RegistrationPolicyFuture<'_> {
        Box::pin(async move {
            require_admin(principal)?;
            self.registration_repository
                .policy()
                .await
                .map(RegistrationPolicy::from)
                .map_err(map_registration_repository_error)
        })
    }

    fn update_policy(
        &self,
        principal: SessionPrincipal,
        command: RegistrationPolicyCommand,
    ) -> RegistrationPolicyUpdateFuture<'_> {
        Box::pin(async move {
            require_admin(principal)?;
            self.registration_repository
                .update_policy(command.into_record())
                .await
                .map(RegistrationPolicy::from)
                .map_err(map_registration_repository_error)
        })
    }
}

impl fmt::Debug for DatabaseRegistrationService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseRegistrationService(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), RegistrationError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(RegistrationError::Forbidden)
    }
}

fn validate_username(username: &str) -> Result<(), RegistrationError> {
    if username.is_empty()
        || username.len() > 64
        || username.trim() != username
        || username.chars().any(char::is_control)
    {
        return Err(RegistrationError::InvalidInput);
    }
    Ok(())
}

fn validate_email(email: Option<&str>) -> Result<(), RegistrationError> {
    let Some(email) = email else {
        return Ok(());
    };
    if email.is_empty()
        || email.len() > 320
        || email.trim() != email
        || email
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(RegistrationError::InvalidInput);
    }
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty()
        || domain.is_empty()
        || parts.next().is_some()
        || local.starts_with('.')
        || local.ends_with('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return Err(RegistrationError::InvalidInput);
    }
    Ok(())
}

pub(crate) fn normalize_email(mut email: String) -> Result<String, RegistrationError> {
    validate_email(Some(&email))?;
    email.make_ascii_lowercase();
    Ok(email)
}

fn is_valid_verification_code(code: &str) -> bool {
    code.len() == REGISTRATION_EMAIL_CODE_BYTES && code.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_valid_invitation_code(code: &str) -> bool {
    code.len() == 25
        && code.starts_with("af-")
        && code[3..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(crate) fn validate_password(password: &str) -> Result<(), RegistrationError> {
    if !(MIN_REGISTRATION_PASSWORD_BYTES..=MAX_REGISTRATION_PASSWORD_BYTES)
        .contains(&password.len())
        || password.chars().any(char::is_control)
    {
        return Err(RegistrationError::InvalidInput);
    }
    Ok(())
}

fn map_registration_repository_error(error: RegistrationRepositoryError) -> RegistrationError {
    match error {
        RegistrationRepositoryError::InvalidPolicy
        | RegistrationRepositoryError::InvalidReference => RegistrationError::InvalidInput,
        RegistrationRepositoryError::Query
        | RegistrationRepositoryError::Timeout
        | RegistrationRepositoryError::Invariant => RegistrationError::Internal,
    }
}

fn map_user_repository_error(error: AdminUserRepositoryError) -> RegistrationError {
    match error {
        AdminUserRepositoryError::Conflict => RegistrationError::Conflict,
        AdminUserRepositoryError::Query
        | AdminUserRepositoryError::Timeout
        | AdminUserRepositoryError::Invariant
        | AdminUserRepositoryError::InvalidReference
        | AdminUserRepositoryError::Entropy => RegistrationError::Internal,
    }
}

fn map_verified_user_repository_error(error: AdminUserRepositoryError) -> RegistrationError {
    match error {
        AdminUserRepositoryError::Conflict => RegistrationError::VerificationRejected,
        AdminUserRepositoryError::Query
        | AdminUserRepositoryError::Timeout
        | AdminUserRepositoryError::Invariant
        | AdminUserRepositoryError::InvalidReference
        | AdminUserRepositoryError::Entropy => RegistrationError::Internal,
    }
}

fn current_timestamp() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_boundaries_and_debug_output_are_stable() {
        let command = RegistrationCommand::new(
            "new-user".to_owned(),
            Some("New@Example.COM".to_owned()),
            "correct horse battery staple".to_owned(),
            Some("042731".to_owned()),
            Some("af-AAAAAAAAAAAAAAAAAAAAAA".to_owned()),
        )
        .unwrap();
        assert_eq!(command.email.as_deref(), Some("new@example.com"));
        assert_eq!(format!("{command:?}"), "RegistrationCommand(<redacted>)");
        assert!(!format!("{command:?}").contains("new@example.com"));
        assert!(
            RegistrationCommand::new(
                " bad ".to_owned(),
                None,
                "correct horse battery staple".to_owned(),
                None,
                None,
            )
            .is_err()
        );
        assert!(
            RegistrationCommand::new(
                "user".to_owned(),
                Some("invalid-email".to_owned()),
                "correct horse battery staple".to_owned(),
                Some("042731".to_owned()),
                None,
            )
            .is_err()
        );
        assert!(
            RegistrationCommand::new("user".to_owned(), None, "too-short".to_owned(), None, None,)
                .is_err()
        );
        assert!(
            RegistrationCommand::new(
                "user".to_owned(),
                None,
                "correct horse battery staple".to_owned(),
                None,
                Some("invalid-invite".to_owned()),
            )
            .is_err()
        );
        assert_eq!(
            format!(
                "{:?}",
                RegistrationEmailVerificationCommand::new("New@Example.COM".to_owned()).unwrap()
            ),
            "RegistrationEmailVerificationCommand(<redacted>)"
        );
        assert!(RegistrationEmailVerificationCommand::new("invalid".to_owned()).is_err());
    }

    #[test]
    fn policy_defaults_and_hard_bounds_are_shared_with_repository() {
        assert_eq!(af_db::DEFAULT_REGISTRATION_RATE_LIMIT_ATTEMPTS, 5);
        assert_eq!(af_db::DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS, 3_600);
        assert!(
            RegistrationPolicyCommand::new(
                true,
                false,
                GroupId::new(1).unwrap(),
                0,
                0,
                false,
                af_db::DEFAULT_REGISTRATION_RATE_LIMIT_ATTEMPTS,
                af_db::DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS,
            )
            .is_ok()
        );
        assert!(
            RegistrationPolicyCommand::new(
                true,
                true,
                GroupId::new(1).unwrap(),
                -1,
                0,
                false,
                0,
                1,
            )
            .is_err()
        );
        assert!(
            RegistrationPolicyCommand::new(
                false,
                true,
                GroupId::new(1).unwrap(),
                0,
                0,
                false,
                5,
                3_600,
            )
            .is_err()
        );
    }
}
