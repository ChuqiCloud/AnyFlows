use std::{
    fmt,
    future::Future,
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    AdminUserCreateRecord, AdminUserRepository, AdminUserRepositoryError,
    MAX_REGISTRATION_RATE_LIMIT_ATTEMPTS, MAX_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS,
    MIN_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS, RegistrationAttemptOutcome,
    RegistrationPolicyRecord, RegistrationPolicyWriteRecord, RegistrationRepository,
    RegistrationRepositoryError,
};
use af_domain::{GroupId, TrustedClientIp, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

use crate::{AdminUserStatus, LoginCredentials, SessionPrincipal, SessionRole};

const REGISTRATION_FINGERPRINT_KEY_BYTES: usize = 32;
const SHA256_BLOCK_BYTES: usize = 64;
const REGISTRATION_FINGERPRINT_DOMAIN: &[u8] = b"AnyFlows registration IP fingerprint v1";

/// 公开注册密码的最小 UTF-8 字节数。
pub const MIN_REGISTRATION_PASSWORD_BYTES: usize = 12;
/// 公开注册密码的最大 UTF-8 字节数。
pub const MAX_REGISTRATION_PASSWORD_BYTES: usize = 128;

/// 游客可读取的最小注册能力状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationStatus {
    enabled: bool,
    email_required: bool,
}

impl RegistrationStatus {
    /// 组合已完成业务校验的公开能力状态，供端口适配器与测试实现使用。
    #[must_use]
    pub const fn from_parts(enabled: bool, email_required: bool) -> Self {
        Self {
            enabled,
            email_required,
        }
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
    enabled: bool,
    default_group_id: GroupId,
    initial_quota: i64,
    email_required: bool,
    rate_limit_attempts: u32,
    rate_limit_window_seconds: u64,
}

impl RegistrationPolicy {
    /// 组合已完成业务校验的完整策略，供端口适配器与测试实现使用。
    #[allow(clippy::too_many_arguments, reason = "字段与注册策略契约一一对应")]
    #[must_use]
    pub const fn from_parts(
        enabled: bool,
        default_group_id: GroupId,
        initial_quota: i64,
        email_required: bool,
        rate_limit_attempts: u32,
        rate_limit_window_seconds: u64,
    ) -> Self {
        Self {
            enabled,
            default_group_id,
            initial_quota,
            email_required,
            rate_limit_attempts,
            rate_limit_window_seconds,
        }
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
}

impl From<RegistrationPolicyRecord> for RegistrationPolicy {
    fn from(record: RegistrationPolicyRecord) -> Self {
        Self {
            enabled: record.enabled(),
            default_group_id: record.default_group_id(),
            initial_quota: record.initial_quota(),
            email_required: record.email_required(),
            rate_limit_attempts: record.rate_limit_attempts(),
            rate_limit_window_seconds: record.rate_limit_window_seconds(),
        }
    }
}

/// 管理员覆盖注册策略时提交的完整命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationPolicyCommand {
    enabled: bool,
    default_group_id: GroupId,
    initial_quota: i64,
    email_required: bool,
    rate_limit_attempts: u32,
    rate_limit_window_seconds: u64,
}

impl RegistrationPolicyCommand {
    /// 校验非负额度和固定窗口硬边界。
    #[allow(clippy::too_many_arguments, reason = "字段与注册策略契约一一对应")]
    pub fn new(
        enabled: bool,
        default_group_id: GroupId,
        initial_quota: i64,
        email_required: bool,
        rate_limit_attempts: u32,
        rate_limit_window_seconds: u64,
    ) -> Result<Self, RegistrationError> {
        if initial_quota < 0
            || !(1..=MAX_REGISTRATION_RATE_LIMIT_ATTEMPTS).contains(&rate_limit_attempts)
            || !(MIN_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS
                ..=MAX_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS)
                .contains(&rate_limit_window_seconds)
        {
            return Err(RegistrationError::InvalidInput);
        }
        Ok(Self {
            enabled,
            default_group_id,
            initial_quota,
            email_required,
            rate_limit_attempts,
            rate_limit_window_seconds,
        })
    }

    fn into_record(self) -> RegistrationPolicyWriteRecord {
        RegistrationPolicyWriteRecord::new(
            self.enabled,
            self.default_group_id,
            self.initial_quota,
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
}

impl RegistrationCommand {
    /// 校验用户名、邮箱形状和密码强度边界。
    pub fn new(
        username: String,
        email: Option<String>,
        password: String,
    ) -> Result<Self, RegistrationError> {
        validate_username(&username)?;
        validate_email(email.as_deref())?;
        validate_password(&password)?;
        Ok(Self {
            username,
            email,
            password,
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
    /// 缺少用于派生不可逆 IP 指纹的应用密钥。
    #[error("注册 IP 指纹密钥缺失")]
    MissingFingerprintKey,
    /// 指纹密钥不是规范的 32 字节 Base64URL 文本。
    #[error("注册 IP 指纹密钥无效")]
    InvalidFingerprintKey,
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
    /// 公开注册未启用。
    #[error("公开注册未启用")]
    Disabled,
    /// 用户名或邮箱与现有有效用户冲突。
    #[error("注册身份冲突")]
    Conflict,
    /// 当前客户端已耗尽固定窗口尝试次数。
    #[error("注册尝试过于频繁")]
    RateLimited { retry_after_seconds: u64 },
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
    fingerprint_key: RegistrationFingerprintKey,
}

impl DatabaseRegistrationService {
    /// 绑定仓储，并从管理会话密钥派生独立的 IP 指纹密钥用途。
    pub fn new(
        registration_repository: RegistrationRepository,
        user_repository: AdminUserRepository,
        fingerprint_key: Option<&str>,
    ) -> Result<Self, RegistrationServiceConfigError> {
        Ok(Self {
            registration_repository,
            user_repository,
            fingerprint_key: RegistrationFingerprintKey::new(fingerprint_key)?,
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
                    enabled: status.enabled(),
                    email_required: status.email_required(),
                })
                .map_err(map_registration_repository_error)
        })
    }

    fn register<'a>(
        &'a self,
        client_ip: TrustedClientIp,
        command: &'a RegistrationCommand,
    ) -> RegistrationFuture<'a> {
        Box::pin(async move {
            let attempted_at = current_timestamp().ok_or(RegistrationError::Internal)?;
            let fingerprint = self.fingerprint_key.derive(client_ip);
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
            let user = self
                .user_repository
                .create(command.user_record(policy))
                .await
                .map_err(map_user_repository_error)?;
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

struct RegistrationFingerprintKey(Zeroizing<[u8; REGISTRATION_FINGERPRINT_KEY_BYTES]>);

impl RegistrationFingerprintKey {
    fn new(value: Option<&str>) -> Result<Self, RegistrationServiceConfigError> {
        let value = value.ok_or(RegistrationServiceConfigError::MissingFingerprintKey)?;
        let mut decoded = [0_u8; REGISTRATION_FINGERPRINT_KEY_BYTES];
        let length = URL_SAFE_NO_PAD
            .decode_slice(value, &mut decoded)
            .map_err(|_| RegistrationServiceConfigError::InvalidFingerprintKey)?;
        if length != REGISTRATION_FINGERPRINT_KEY_BYTES {
            decoded.zeroize();
            return Err(RegistrationServiceConfigError::InvalidFingerprintKey);
        }
        Ok(Self(Zeroizing::new(decoded)))
    }

    fn derive(&self, client_ip: TrustedClientIp) -> [u8; 32] {
        let mut inner_pad = [0x36_u8; SHA256_BLOCK_BYTES];
        let mut outer_pad = [0x5c_u8; SHA256_BLOCK_BYTES];
        for (index, key_byte) in self.0.iter().copied().enumerate() {
            inner_pad[index] ^= key_byte;
            outer_pad[index] ^= key_byte;
        }

        let mut address = [0_u8; 17];
        let address_length = client_ip.write_fingerprint_input(&mut address);
        let mut inner = Sha256::new();
        inner.update(inner_pad);
        inner.update(REGISTRATION_FINGERPRINT_DOMAIN);
        inner.update(&address[..address_length]);
        let inner_digest = inner.finalize();

        let mut outer = Sha256::new();
        outer.update(outer_pad);
        outer.update(inner_digest);
        let fingerprint = outer.finalize().into();

        address.zeroize();
        inner_pad.zeroize();
        outer_pad.zeroize();
        fingerprint
    }
}

impl fmt::Debug for RegistrationFingerprintKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RegistrationFingerprintKey(<redacted>)")
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

fn validate_password(password: &str) -> Result<(), RegistrationError> {
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

fn current_timestamp() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    use super::*;

    fn key() -> String {
        URL_SAFE_NO_PAD.encode([0x42; REGISTRATION_FINGERPRINT_KEY_BYTES])
    }

    #[test]
    fn command_boundaries_and_debug_output_are_stable() {
        let command = RegistrationCommand::new(
            "new-user".to_owned(),
            Some("new@example.com".to_owned()),
            "correct horse battery staple".to_owned(),
        )
        .unwrap();
        assert_eq!(format!("{command:?}"), "RegistrationCommand(<redacted>)");
        assert!(!format!("{command:?}").contains("new@example.com"));
        assert!(
            RegistrationCommand::new(
                " bad ".to_owned(),
                None,
                "correct horse battery staple".to_owned(),
            )
            .is_err()
        );
        assert!(
            RegistrationCommand::new(
                "user".to_owned(),
                Some("invalid-email".to_owned()),
                "correct horse battery staple".to_owned(),
            )
            .is_err()
        );
        assert!(
            RegistrationCommand::new("user".to_owned(), None, "too-short".to_owned(),).is_err()
        );
    }

    #[test]
    fn keyed_fingerprint_is_stable_normalized_and_key_scoped() {
        let first = RegistrationFingerprintKey::new(Some(&key())).unwrap();
        let second_key = URL_SAFE_NO_PAD.encode([0x24; REGISTRATION_FINGERPRINT_KEY_BYTES]);
        let second = RegistrationFingerprintKey::new(Some(&second_key)).unwrap();
        let ipv4 = TrustedClientIp::new("192.0.2.9".parse::<IpAddr>().unwrap());
        let mapped = TrustedClientIp::new("::ffff:192.0.2.9".parse::<IpAddr>().unwrap());
        let other = TrustedClientIp::new("192.0.2.10".parse::<IpAddr>().unwrap());

        assert_eq!(first.derive(ipv4), first.derive(mapped));
        assert_ne!(first.derive(ipv4), first.derive(other));
        assert_ne!(first.derive(ipv4), second.derive(ipv4));
        assert_eq!(
            format!("{first:?}"),
            "RegistrationFingerprintKey(<redacted>)"
        );
    }

    #[test]
    fn policy_defaults_and_hard_bounds_are_shared_with_repository() {
        assert_eq!(af_db::DEFAULT_REGISTRATION_RATE_LIMIT_ATTEMPTS, 5);
        assert_eq!(af_db::DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS, 3_600);
        assert!(
            RegistrationPolicyCommand::new(
                false,
                GroupId::new(1).unwrap(),
                0,
                false,
                af_db::DEFAULT_REGISTRATION_RATE_LIMIT_ATTEMPTS,
                af_db::DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS,
            )
            .is_ok()
        );
        assert!(
            RegistrationPolicyCommand::new(true, GroupId::new(1).unwrap(), -1, false, 0, 1,)
                .is_err()
        );
    }
}
