use std::{
    fmt,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_account::SystemSecretCipher;
use af_db::{
    AuthChallengeConsume, AuthChallengeConsumeOutcome, AuthChallengeIssue,
    AuthChallengeIssueOutcome, AuthChallengePurpose, AuthChallengeRateLimitClaim,
    AuthChallengeRateLimitOutcome, AuthChallengeRateLimitRepository,
    AuthChallengeRateLimitRepositoryError, AuthChallengeRepository, AuthChallengeRepositoryError,
    EmailSettingsRepository, EmailSettingsRepositoryError,
};
use af_domain::UserId;
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

use crate::{
    AdminEmailSettingsError, EmailDelivery, EmailDeliveryRequest,
    auth_challenge_security::{AuthChallengeSecurityKey, AuthChallengeSecurityKeyError},
    registration::{RegistrationError, normalize_email},
};

const CODE_BYTES: usize = 6;
const TTL_SECONDS: u64 = 10 * 60;
const RESEND_COOLDOWN_SECONDS: u64 = 60;
const MAX_ATTEMPTS: u32 = 5;
const RATE_LIMIT_ATTEMPTS: u32 = 5;
const RATE_LIMIT_WINDOW_SECONDS: u64 = 3_600;
const SUBJECT_DOMAIN: &[u8] = b"AnyFlows auth challenge email-binding subject v1";
const SECRET_DOMAIN: &[u8] = b"AnyFlows auth challenge email-binding secret v1";
const CLIENT_DOMAIN: &[u8] = b"AnyFlows auth challenge email-binding client v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmailBindingIssued {
    expires_at: u64,
    next_send_at: u64,
}

impl EmailBindingIssued {
    pub const fn expires_at(self) -> u64 {
        self.expires_at
    }
    pub const fn next_send_at(self) -> u64 {
        self.next_send_at
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EmailBindingError {
    #[error("邮箱输入无效")]
    InvalidInput,
    #[error("邮箱验证码无效")]
    Rejected,
    #[error("邮箱服务尚未配置")]
    EmailNotConfigured,
    #[error("邮箱验证码投递失败")]
    DeliveryFailed,
    #[error("邮箱验证码发送过于频繁")]
    RateLimited { retry_after_seconds: u64 },
    #[error("邮箱绑定内部失败")]
    Internal,
}

pub struct EmailBindingService {
    challenge_repository: AuthChallengeRepository,
    rate_limit_repository: AuthChallengeRateLimitRepository,
    email_settings_repository: EmailSettingsRepository,
    system_secret_cipher: SystemSecretCipher,
    delivery: Arc<dyn EmailDelivery>,
    security_key: AuthChallengeSecurityKey,
}

impl EmailBindingService {
    #[allow(clippy::too_many_arguments, reason = "邮箱绑定依赖必须显式注入")]
    pub fn new(
        challenge_repository: AuthChallengeRepository,
        rate_limit_repository: AuthChallengeRateLimitRepository,
        email_settings_repository: EmailSettingsRepository,
        system_secret_cipher: SystemSecretCipher,
        delivery: Arc<dyn EmailDelivery>,
        security_key: Option<&str>,
    ) -> Result<Self, EmailBindingConfigError> {
        Ok(Self {
            challenge_repository,
            rate_limit_repository,
            email_settings_repository,
            system_secret_cipher,
            delivery,
            security_key: AuthChallengeSecurityKey::new(security_key)
                .map_err(EmailBindingConfigError::from)?,
        })
    }

    pub async fn issue(
        &self,
        user_id: UserId,
        email: &str,
    ) -> Result<EmailBindingIssued, EmailBindingError> {
        let issued_at = current_timestamp().ok_or(EmailBindingError::Internal)?;
        let settings = self
            .email_settings_repository
            .settings()
            .await
            .map_err(map_email_repository_error)?;
        if !settings.delivery_ready() {
            return Err(EmailBindingError::EmailNotConfigured);
        }
        let code = generate_code()?;
        let request = EmailDeliveryRequest::email_binding_verification(
            &settings,
            &self.system_secret_cipher,
            email.to_owned(),
            &code,
            TTL_SECONDS,
        )
        .map_err(map_email_request_error)?;
        let user_bytes = user_id.get().to_be_bytes();
        let subject_fingerprint = self
            .security_key
            .derive(SUBJECT_DOMAIN, &[&user_bytes, email.as_bytes()]);
        let secret_digest = self
            .security_key
            .derive(SECRET_DOMAIN, &[&subject_fingerprint, code.as_bytes()]);
        let client_fingerprint = self.security_key.derive(CLIENT_DOMAIN, &[&user_bytes]);
        let claim = AuthChallengeRateLimitClaim::new(
            AuthChallengePurpose::EmailBinding,
            subject_fingerprint,
            client_fingerprint,
            issued_at,
            RATE_LIMIT_ATTEMPTS,
            RATE_LIMIT_WINDOW_SECONDS,
        )
        .map_err(|_| EmailBindingError::Internal)?;
        if let AuthChallengeRateLimitOutcome::RateLimited {
            retry_after_seconds,
        } = self
            .rate_limit_repository
            .claim(claim)
            .await
            .map_err(map_rate_limit_error)?
        {
            return Err(EmailBindingError::RateLimited {
                retry_after_seconds,
            });
        }
        let issue = AuthChallengeIssue::new(
            AuthChallengePurpose::EmailBinding,
            subject_fingerprint,
            secret_digest,
            Some(user_id),
            issued_at,
            TTL_SECONDS,
            RESEND_COOLDOWN_SECONDS,
            MAX_ATTEMPTS,
        )
        .map_err(|_| EmailBindingError::Internal)?;
        let issued = match self
            .challenge_repository
            .issue(issue)
            .await
            .map_err(map_challenge_error)?
        {
            AuthChallengeIssueOutcome::Issued(value) => value,
            AuthChallengeIssueOutcome::Cooldown {
                retry_after_seconds,
            } => {
                return Err(EmailBindingError::RateLimited {
                    retry_after_seconds,
                });
            }
        };
        self.delivery
            .send(request)
            .await
            .map_err(|_| EmailBindingError::DeliveryFailed)?;
        Ok(EmailBindingIssued {
            expires_at: issued.expires_at(),
            next_send_at: issued.next_send_at(),
        })
    }

    pub async fn consume(
        &self,
        user_id: UserId,
        email: &str,
        code: &str,
    ) -> Result<(), EmailBindingError> {
        if code.len() != CODE_BYTES || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(EmailBindingError::Rejected);
        }
        let now = current_timestamp().ok_or(EmailBindingError::Internal)?;
        let user_bytes = user_id.get().to_be_bytes();
        let subject_fingerprint = self
            .security_key
            .derive(SUBJECT_DOMAIN, &[&user_bytes, email.as_bytes()]);
        let secret_digest = self
            .security_key
            .derive(SECRET_DOMAIN, &[&subject_fingerprint, code.as_bytes()]);
        let consume = AuthChallengeConsume::new(
            AuthChallengePurpose::EmailBinding,
            subject_fingerprint,
            secret_digest,
            now,
        )
        .map_err(|_| EmailBindingError::Internal)?;
        match self
            .challenge_repository
            .consume(consume)
            .await
            .map_err(map_challenge_error)?
        {
            AuthChallengeConsumeOutcome::Consumed(result)
                if result.target_user_id() == Some(user_id) =>
            {
                Ok(())
            }
            AuthChallengeConsumeOutcome::Consumed(_) | AuthChallengeConsumeOutcome::Rejected => {
                Err(EmailBindingError::Rejected)
            }
        }
    }
}

impl fmt::Debug for EmailBindingService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EmailBindingService(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EmailBindingConfigError {
    #[error("邮箱绑定安全派生密钥缺失")]
    MissingSecurityKey,
    #[error("邮箱绑定安全派生密钥无效")]
    InvalidSecurityKey,
}

impl From<AuthChallengeSecurityKeyError> for EmailBindingConfigError {
    fn from(value: AuthChallengeSecurityKeyError) -> Self {
        match value {
            AuthChallengeSecurityKeyError::Missing => Self::MissingSecurityKey,
            AuthChallengeSecurityKeyError::Invalid => Self::InvalidSecurityKey,
        }
    }
}

fn generate_code() -> Result<Zeroizing<String>, EmailBindingError> {
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes).map_err(|_| EmailBindingError::Internal)?;
    let value = u64::from_le_bytes(bytes);
    bytes.zeroize();
    Ok(Zeroizing::new(format!("{:06}", value % 1_000_000)))
}

fn current_timestamp() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|value| value.as_secs())
}

fn map_email_repository_error(_: EmailSettingsRepositoryError) -> EmailBindingError {
    EmailBindingError::Internal
}
fn map_email_request_error(error: AdminEmailSettingsError) -> EmailBindingError {
    match error {
        AdminEmailSettingsError::NotConfigured => EmailBindingError::EmailNotConfigured,
        AdminEmailSettingsError::InvalidInput
        | AdminEmailSettingsError::Forbidden
        | AdminEmailSettingsError::DeliveryFailed
        | AdminEmailSettingsError::Internal => EmailBindingError::Internal,
    }
}
fn map_rate_limit_error(_: AuthChallengeRateLimitRepositoryError) -> EmailBindingError {
    EmailBindingError::Internal
}
fn map_challenge_error(_: AuthChallengeRepositoryError) -> EmailBindingError {
    EmailBindingError::Internal
}

pub fn normalize_binding_email(email: String) -> Result<String, EmailBindingError> {
    normalize_email(email).map_err(|error| match error {
        RegistrationError::InvalidInput => EmailBindingError::InvalidInput,
        _ => EmailBindingError::Internal,
    })
}
