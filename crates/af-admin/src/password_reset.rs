use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_account::SystemSecretCipher;
use af_db::{
    AuthChallengeConsume, AuthChallengeIssue, AuthChallengeIssueOutcome, AuthChallengePurpose,
    AuthChallengeRateLimitClaim, AuthChallengeRateLimitOutcome, AuthChallengeRateLimitRepository,
    AuthChallengeRateLimitRepositoryError, AuthChallengeRepository, AuthChallengeRepositoryError,
    EmailSettingsRepository, EmailSettingsRepositoryError, PasswordResetOutcome,
    PasswordResetRepository, PasswordResetRepositoryError, PasswordResetTargetOutcome,
    SiteSettingsRepository, SiteSettingsRepositoryError,
};
use af_domain::TrustedClientIp;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use getrandom::fill;
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

use crate::{
    AdminEmailSettingsError, EmailDelivery, EmailDeliveryRequest,
    auth_challenge_security::{AuthChallengeSecurityKey, AuthChallengeSecurityKeyError},
    registration::{MAX_REGISTRATION_PASSWORD_BYTES, MIN_REGISTRATION_PASSWORD_BYTES},
};

/// 密码重置密码沿用公开注册的长度边界，避免两条密码入口出现分歧。
pub const MIN_PASSWORD_RESET_PASSWORD_BYTES: usize = MIN_REGISTRATION_PASSWORD_BYTES;
/// 密码重置密码沿用公开注册的长度边界，避免两条密码入口出现分歧。
pub const MAX_PASSWORD_RESET_PASSWORD_BYTES: usize = MAX_REGISTRATION_PASSWORD_BYTES;

const PASSWORD_RESET_SELECTOR_BYTES: usize = 16;
const PASSWORD_RESET_VERIFIER_BYTES: usize = 32;
const PASSWORD_RESET_EMAIL_TTL_SECONDS: u64 = 30 * 60;
const PASSWORD_RESET_EMAIL_RESEND_COOLDOWN_SECONDS: u64 = 60;
const PASSWORD_RESET_MAX_ATTEMPTS: u32 = 5;
const PASSWORD_RESET_RATE_LIMIT_ATTEMPTS: u32 = 5;
const PASSWORD_RESET_RATE_LIMIT_WINDOW_SECONDS: u64 = 86_400;
const PASSWORD_RESET_EMAIL_SUBJECT_DOMAIN: &[u8] =
    b"AnyFlows auth challenge password-reset email subject v1";
const PASSWORD_RESET_EMAIL_CLIENT_DOMAIN: &[u8] =
    b"AnyFlows auth challenge password-reset email client v1";
const PASSWORD_RESET_TOKEN_SUBJECT_DOMAIN: &[u8] =
    b"AnyFlows auth challenge password-reset token subject v1";
const PASSWORD_RESET_TOKEN_SECRET_DOMAIN: &[u8] =
    b"AnyFlows auth challenge password-reset token secret v1";

/// 忘记密码请求的规范化输入。
pub struct PasswordResetRequestCommand {
    email: String,
}

impl PasswordResetRequestCommand {
    /// 校验并规范化邮箱；邮箱内容不会出现在调试输出中。
    pub fn new(email: String) -> Result<Self, PasswordResetError> {
        let email = crate::registration::normalize_email(email)
            .map_err(|_| PasswordResetError::InvalidInput)?;
        Ok(Self { email })
    }
}

impl fmt::Debug for PasswordResetRequestCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasswordResetRequestCommand(<redacted>)")
    }
}

/// 密码重置确认输入；令牌和新密码只在一次请求生命周期内存在。
pub struct PasswordResetConfirmCommand {
    token: PasswordResetToken,
    password: Zeroizing<String>,
}

impl PasswordResetConfirmCommand {
    /// 校验高熵令牌结构与新密码边界。
    pub fn new(token: String, password: String) -> Result<Self, PasswordResetError> {
        let token = PasswordResetToken::parse(&token)?;
        crate::registration::validate_password(&password)
            .map_err(|_| PasswordResetError::InvalidInput)?;
        Ok(Self {
            token,
            password: Zeroizing::new(password),
        })
    }
}

impl fmt::Debug for PasswordResetConfirmCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasswordResetConfirmCommand(<redacted>)")
    }
}

/// 忘记密码请求的公开结果，故意不表达邮箱是否存在。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PasswordResetRequestResult {
    accepted: bool,
}

impl PasswordResetRequestResult {
    /// 构造统一的已受理结果。
    #[must_use]
    pub const fn accepted() -> Self {
        Self { accepted: true }
    }

    /// 返回请求是否被公开入口受理。
    #[must_use]
    pub const fn accepted_flag(self) -> bool {
        self.accepted
    }
}

/// 密码重置确认成功结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PasswordResetConfirmResult {
    user_id: af_domain::UserId,
}

impl PasswordResetConfirmResult {
    /// 从仓储更新结果构造确认结果。
    #[must_use]
    pub const fn from_user_id(user_id: af_domain::UserId) -> Self {
        Self { user_id }
    }

    /// 返回被更新的用户标识；HTTP 层不会将其回显给游客。
    #[must_use]
    pub const fn user_id(self) -> af_domain::UserId {
        self.user_id
    }
}

/// 密码重置 service 的启动配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PasswordResetServiceConfigError {
    /// 缺少认证挑战摘要派生密钥。
    #[error("密码重置安全派生密钥缺失")]
    MissingSecurityKey,
    /// 认证挑战摘要派生密钥格式无效。
    #[error("密码重置安全派生密钥无效")]
    InvalidSecurityKey,
}

/// 密码重置公开服务错误；不携带邮箱、令牌、密码或底层诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PasswordResetError {
    /// 请求字段或令牌结构无效。
    #[error("密码重置请求无效")]
    InvalidInput,
    /// 令牌无效、过期、重放、次数耗尽或目标已不可重置。
    #[error("密码重置挑战无效")]
    Rejected,
    /// 邮件或公开站点基址尚未配置完成。
    #[error("密码重置邮件尚未配置")]
    EmailNotConfigured,
    /// SMTP 投递失败。
    #[error("密码重置邮件投递失败")]
    EmailDeliveryFailed,
    /// 仓储、时钟、随机源或持久化不变量失败。
    #[error("密码重置内部失败")]
    Internal,
}

/// 忘记密码请求 Future。
pub type PasswordResetRequestFuture<'a> = Pin<
    Box<dyn Future<Output = Result<PasswordResetRequestResult, PasswordResetError>> + Send + 'a>,
>;
/// 密码重置确认 Future。
pub type PasswordResetConfirmFuture<'a> = Pin<
    Box<dyn Future<Output = Result<PasswordResetConfirmResult, PasswordResetError>> + Send + 'a>,
>;

/// 公开密码重置 service 的应用边界。
pub trait PasswordResetService: Send + Sync {
    /// 发送重置邮件；未知邮箱和限流均返回同一公开成功语义。
    fn request<'a>(
        &'a self,
        client_ip: TrustedClientIp,
        command: &'a PasswordResetRequestCommand,
    ) -> PasswordResetRequestFuture<'a>;

    /// 消费单次令牌并原子更新密码。
    fn confirm(&self, command: PasswordResetConfirmCommand) -> PasswordResetConfirmFuture<'_>;
}

/// 使用认证挑战、SMTP 和站点设置仓储实现密码重置闭环。
pub struct DatabasePasswordResetService {
    password_reset_repository: PasswordResetRepository,
    challenge_repository: AuthChallengeRepository,
    rate_limit_repository: AuthChallengeRateLimitRepository,
    email_settings_repository: EmailSettingsRepository,
    site_settings_repository: SiteSettingsRepository,
    system_secret_cipher: SystemSecretCipher,
    delivery: Arc<dyn EmailDelivery>,
    security_key: AuthChallengeSecurityKey,
}

impl DatabasePasswordResetService {
    /// 显式注入密码更新、挑战生命周期、邮件和站点设置依赖。
    #[allow(clippy::too_many_arguments, reason = "认证边界依赖必须显式注入")]
    pub fn new(
        password_reset_repository: PasswordResetRepository,
        challenge_repository: AuthChallengeRepository,
        rate_limit_repository: AuthChallengeRateLimitRepository,
        email_settings_repository: EmailSettingsRepository,
        site_settings_repository: SiteSettingsRepository,
        system_secret_cipher: SystemSecretCipher,
        delivery: Arc<dyn EmailDelivery>,
        security_key: Option<&str>,
    ) -> Result<Self, PasswordResetServiceConfigError> {
        Ok(Self {
            password_reset_repository,
            challenge_repository,
            rate_limit_repository,
            email_settings_repository,
            site_settings_repository,
            system_secret_cipher,
            delivery,
            security_key: AuthChallengeSecurityKey::new(security_key)
                .map_err(map_security_key_error)?,
        })
    }
}

impl PasswordResetService for DatabasePasswordResetService {
    fn request<'a>(
        &'a self,
        client_ip: TrustedClientIp,
        command: &'a PasswordResetRequestCommand,
    ) -> PasswordResetRequestFuture<'a> {
        Box::pin(async move {
            let requested_at = current_timestamp().ok_or(PasswordResetError::Internal)?;
            let email_settings = self
                .email_settings_repository
                .settings()
                .await
                .map_err(map_email_repository_error)?;
            let site_settings = self
                .site_settings_repository
                .settings()
                .await
                .map_err(map_site_repository_error)?;
            if !email_settings.enabled()
                || !email_settings.delivery_ready()
                || site_settings.public_base_url().is_none()
            {
                return Err(PasswordResetError::EmailNotConfigured);
            }

            let subject_fingerprint = self.security_key.derive(
                PASSWORD_RESET_EMAIL_SUBJECT_DOMAIN,
                &[command.email.as_bytes()],
            );
            let client_fingerprint = self
                .security_key
                .derive_client(PASSWORD_RESET_EMAIL_CLIENT_DOMAIN, client_ip);
            let target = self
                .password_reset_repository
                .find_target(&command.email)
                .await
                .map_err(map_password_reset_repository_error)?;
            let PasswordResetTargetOutcome::Found(user_id) = target else {
                return Ok(PasswordResetRequestResult::accepted());
            };

            let rate_limit = AuthChallengeRateLimitClaim::new(
                AuthChallengePurpose::PasswordReset,
                subject_fingerprint,
                client_fingerprint,
                requested_at,
                PASSWORD_RESET_RATE_LIMIT_ATTEMPTS,
                PASSWORD_RESET_RATE_LIMIT_WINDOW_SECONDS,
            )
            .map_err(|_| PasswordResetError::Internal)?;
            if matches!(
                self.rate_limit_repository
                    .claim(rate_limit)
                    .await
                    .map_err(map_rate_limit_repository_error)?,
                AuthChallengeRateLimitOutcome::RateLimited { .. }
            ) {
                return Ok(PasswordResetRequestResult::accepted());
            }

            let token = PasswordResetToken::generate()?;
            let challenge_subject = self
                .security_key
                .derive(PASSWORD_RESET_TOKEN_SUBJECT_DOMAIN, &[&token.selector]);
            let secret_digest = self.security_key.derive(
                PASSWORD_RESET_TOKEN_SECRET_DOMAIN,
                &[&token.selector, token.verifier.as_ref()],
            );
            let issue = AuthChallengeIssue::new(
                AuthChallengePurpose::PasswordReset,
                challenge_subject,
                secret_digest,
                Some(user_id),
                requested_at,
                PASSWORD_RESET_EMAIL_TTL_SECONDS,
                PASSWORD_RESET_EMAIL_RESEND_COOLDOWN_SECONDS,
                PASSWORD_RESET_MAX_ATTEMPTS,
            )
            .map_err(|_| PasswordResetError::Internal)?;
            if matches!(
                self.challenge_repository
                    .issue(issue)
                    .await
                    .map_err(map_challenge_repository_error)?,
                AuthChallengeIssueOutcome::Cooldown { .. }
            ) {
                return Ok(PasswordResetRequestResult::accepted());
            }

            let reset_url = build_reset_url(
                site_settings
                    .public_base_url()
                    .ok_or(PasswordResetError::EmailNotConfigured)?,
                &token,
            )?;
            let request = EmailDeliveryRequest::password_reset(
                &email_settings,
                &self.system_secret_cipher,
                command.email.clone(),
                site_settings.site_name(),
                &reset_url,
                PASSWORD_RESET_EMAIL_TTL_SECONDS,
            )
            .map_err(map_email_request_error)?;
            self.delivery
                .send(request)
                .await
                .map_err(|_| PasswordResetError::EmailDeliveryFailed)?;
            Ok(PasswordResetRequestResult::accepted())
        })
    }

    fn confirm(&self, command: PasswordResetConfirmCommand) -> PasswordResetConfirmFuture<'_> {
        Box::pin(async move {
            let attempted_at = current_timestamp().ok_or(PasswordResetError::Internal)?;
            let challenge = command
                .token
                .to_consume(&self.security_key, attempted_at)
                .map_err(|_| PasswordResetError::Rejected)?;
            match self
                .password_reset_repository
                .reset_password(challenge, command.password)
                .await
                .map_err(map_password_reset_repository_error)?
            {
                PasswordResetOutcome::Updated(user_id) => {
                    Ok(PasswordResetConfirmResult::from_user_id(user_id))
                }
                PasswordResetOutcome::Rejected => Err(PasswordResetError::Rejected),
            }
        })
    }
}

impl fmt::Debug for DatabasePasswordResetService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabasePasswordResetService(<redacted>)")
    }
}

struct PasswordResetToken {
    selector: [u8; PASSWORD_RESET_SELECTOR_BYTES],
    verifier: Zeroizing<[u8; PASSWORD_RESET_VERIFIER_BYTES]>,
}

impl PasswordResetToken {
    fn generate() -> Result<Self, PasswordResetError> {
        let mut selector = [0_u8; PASSWORD_RESET_SELECTOR_BYTES];
        fill(&mut selector).map_err(|_| PasswordResetError::Internal)?;
        let mut verifier = [0_u8; PASSWORD_RESET_VERIFIER_BYTES];
        if fill(&mut verifier).is_err() {
            selector.zeroize();
            verifier.zeroize();
            return Err(PasswordResetError::Internal);
        }
        Ok(Self {
            selector,
            verifier: Zeroizing::new(verifier),
        })
    }

    fn parse(value: &str) -> Result<Self, PasswordResetError> {
        let mut parts = value.split('.');
        let selector_part = parts.next().ok_or(PasswordResetError::Rejected)?;
        let verifier_part = parts.next().ok_or(PasswordResetError::Rejected)?;
        if parts.next().is_some() {
            return Err(PasswordResetError::Rejected);
        }
        let mut selector = [0_u8; PASSWORD_RESET_SELECTOR_BYTES];
        let selector_length = URL_SAFE_NO_PAD
            .decode_slice(selector_part, &mut selector)
            .map_err(|_| PasswordResetError::Rejected)?;
        if selector_length != PASSWORD_RESET_SELECTOR_BYTES {
            selector.zeroize();
            return Err(PasswordResetError::Rejected);
        }
        let mut verifier = [0_u8; PASSWORD_RESET_VERIFIER_BYTES];
        let verifier_length = URL_SAFE_NO_PAD
            .decode_slice(verifier_part, &mut verifier)
            .map_err(|_| PasswordResetError::Rejected)?;
        if verifier_length != PASSWORD_RESET_VERIFIER_BYTES {
            selector.zeroize();
            verifier.zeroize();
            return Err(PasswordResetError::Rejected);
        }
        Ok(Self {
            selector,
            verifier: Zeroizing::new(verifier),
        })
    }

    fn encode(&self) -> String {
        format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(self.selector),
            URL_SAFE_NO_PAD.encode(self.verifier.as_ref())
        )
    }

    fn to_consume(
        &self,
        security_key: &AuthChallengeSecurityKey,
        attempted_at: u64,
    ) -> Result<AuthChallengeConsume, PasswordResetError> {
        let subject_fingerprint =
            security_key.derive(PASSWORD_RESET_TOKEN_SUBJECT_DOMAIN, &[&self.selector]);
        let secret_digest = security_key.derive(
            PASSWORD_RESET_TOKEN_SECRET_DOMAIN,
            &[&self.selector, self.verifier.as_ref()],
        );
        AuthChallengeConsume::new(
            AuthChallengePurpose::PasswordReset,
            subject_fingerprint,
            secret_digest,
            attempted_at,
        )
        .map_err(|_| PasswordResetError::Rejected)
    }
}

impl fmt::Debug for PasswordResetToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasswordResetToken(<redacted>)")
    }
}

fn build_reset_url(
    base_url: &str,
    token: &PasswordResetToken,
) -> Result<String, PasswordResetError> {
    let base_url = base_url.trim_end_matches('/');
    if base_url.is_empty() {
        return Err(PasswordResetError::EmailNotConfigured);
    }
    Ok(format!(
        "{base_url}/#/reset-password?token={}",
        token.encode()
    ))
}

fn map_security_key_error(error: AuthChallengeSecurityKeyError) -> PasswordResetServiceConfigError {
    match error {
        AuthChallengeSecurityKeyError::Missing => {
            PasswordResetServiceConfigError::MissingSecurityKey
        }
        AuthChallengeSecurityKeyError::Invalid => {
            PasswordResetServiceConfigError::InvalidSecurityKey
        }
    }
}

fn map_email_repository_error(error: EmailSettingsRepositoryError) -> PasswordResetError {
    match error {
        EmailSettingsRepositoryError::InvalidSettings
        | EmailSettingsRepositoryError::Query
        | EmailSettingsRepositoryError::Timeout
        | EmailSettingsRepositoryError::Invariant => PasswordResetError::Internal,
    }
}

fn map_site_repository_error(error: SiteSettingsRepositoryError) -> PasswordResetError {
    match error {
        SiteSettingsRepositoryError::InvalidSettings
        | SiteSettingsRepositoryError::Conflict
        | SiteSettingsRepositoryError::Query
        | SiteSettingsRepositoryError::Timeout
        | SiteSettingsRepositoryError::Invariant => PasswordResetError::Internal,
    }
}

fn map_email_request_error(error: AdminEmailSettingsError) -> PasswordResetError {
    match error {
        AdminEmailSettingsError::NotConfigured => PasswordResetError::EmailNotConfigured,
        AdminEmailSettingsError::InvalidInput
        | AdminEmailSettingsError::Forbidden
        | AdminEmailSettingsError::DeliveryFailed
        | AdminEmailSettingsError::Internal => PasswordResetError::Internal,
    }
}

fn map_rate_limit_repository_error(
    error: AuthChallengeRateLimitRepositoryError,
) -> PasswordResetError {
    match error {
        AuthChallengeRateLimitRepositoryError::Query
        | AuthChallengeRateLimitRepositoryError::Timeout
        | AuthChallengeRateLimitRepositoryError::Invariant => PasswordResetError::Internal,
    }
}

fn map_challenge_repository_error(error: AuthChallengeRepositoryError) -> PasswordResetError {
    match error {
        AuthChallengeRepositoryError::Query
        | AuthChallengeRepositoryError::Timeout
        | AuthChallengeRepositoryError::Invariant => PasswordResetError::Internal,
    }
}

fn map_password_reset_repository_error(error: PasswordResetRepositoryError) -> PasswordResetError {
    match error {
        PasswordResetRepositoryError::Entropy => PasswordResetError::Internal,
        PasswordResetRepositoryError::Query
        | PasswordResetRepositoryError::Timeout
        | PasswordResetRepositoryError::Invariant => PasswordResetError::Internal,
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
    fn token_round_trip_keeps_high_entropy_parts_redacted() {
        let token = PasswordResetToken::generate().unwrap();
        let encoded = token.encode();
        let parsed = PasswordResetToken::parse(&encoded).unwrap();
        assert_eq!(parsed.selector, token.selector);
        assert_eq!(parsed.verifier.as_ref(), token.verifier.as_ref());
        assert_eq!(format!("{parsed:?}"), "PasswordResetToken(<redacted>)");
        assert!(!format!("{parsed:?}").contains(&encoded));
    }

    #[test]
    fn malformed_tokens_are_rejected_without_echoing_input() {
        for value in ["", "short", "a.b.c", "!!!!.!!!!"] {
            assert_eq!(
                PasswordResetToken::parse(value).unwrap_err(),
                PasswordResetError::Rejected
            );
        }
    }
}
