use std::{fmt, sync::Arc};

use af_account::SystemSecretCipher;
use af_db::{
    AuthChallengeConsume, AuthChallengeIssue, AuthChallengeIssueOutcome, AuthChallengePurpose,
    AuthChallengeRateLimitClaim, AuthChallengeRateLimitOutcome, AuthChallengeRateLimitRepository,
    AuthChallengeRateLimitRepositoryError, AuthChallengeRepository, AuthChallengeRepositoryError,
    EmailSettingsRepository, EmailSettingsRepositoryError, RegistrationPolicyRecord,
};
use af_domain::TrustedClientIp;
use zeroize::{Zeroize as _, Zeroizing};

use crate::{
    AdminEmailSettingsError, EmailDelivery, EmailDeliveryRequest,
    auth_challenge_security::{AuthChallengeSecurityKey, AuthChallengeSecurityKeyError},
    registration::{RegistrationError, RegistrationServiceConfigError},
};

pub(super) const REGISTRATION_EMAIL_CODE_BYTES: usize = 6;
const REGISTRATION_EMAIL_TTL_SECONDS: u64 = 10 * 60;
const REGISTRATION_EMAIL_RESEND_COOLDOWN_SECONDS: u64 = 60;
const REGISTRATION_EMAIL_MAX_ATTEMPTS: u32 = 5;
const REGISTRATION_IP_FINGERPRINT_DOMAIN: &[u8] = b"AnyFlows registration IP fingerprint v1";
const REGISTRATION_EMAIL_SUBJECT_DOMAIN: &[u8] =
    b"AnyFlows auth challenge registration-email subject v1";
const REGISTRATION_EMAIL_SECRET_DOMAIN: &[u8] =
    b"AnyFlows auth challenge registration-email secret v1";
const REGISTRATION_EMAIL_CLIENT_DOMAIN: &[u8] =
    b"AnyFlows auth challenge registration-email client v1";

/// 注册邮件成功投递后可公开返回的时间边界。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct IssuedEmailVerification {
    expires_at: u64,
    next_send_at: u64,
}

impl IssuedEmailVerification {
    pub(super) const fn expires_at(self) -> u64 {
        self.expires_at
    }

    pub(super) const fn next_send_at(self) -> u64 {
        self.next_send_at
    }
}

/// 注册验证码派生、限流、持久化与真实邮件投递的应用内协作器。
pub(super) struct RegistrationEmailVerification {
    challenge_repository: AuthChallengeRepository,
    rate_limit_repository: AuthChallengeRateLimitRepository,
    email_settings_repository: EmailSettingsRepository,
    system_secret_cipher: SystemSecretCipher,
    delivery: Arc<dyn EmailDelivery>,
    security_key: AuthChallengeSecurityKey,
}

impl RegistrationEmailVerification {
    #[allow(clippy::too_many_arguments, reason = "显式注入独立持久化与投递边界")]
    pub(super) fn new(
        challenge_repository: AuthChallengeRepository,
        rate_limit_repository: AuthChallengeRateLimitRepository,
        email_settings_repository: EmailSettingsRepository,
        system_secret_cipher: SystemSecretCipher,
        delivery: Arc<dyn EmailDelivery>,
        security_key: Option<&str>,
    ) -> Result<Self, RegistrationServiceConfigError> {
        Ok(Self {
            challenge_repository,
            rate_limit_repository,
            email_settings_repository,
            system_secret_cipher,
            delivery,
            security_key: AuthChallengeSecurityKey::new(security_key)
                .map_err(map_security_key_error)?,
        })
    }

    pub(super) fn registration_ip_fingerprint(&self, client_ip: TrustedClientIp) -> [u8; 32] {
        self.security_key
            .derive_client(REGISTRATION_IP_FINGERPRINT_DOMAIN, client_ip)
    }

    pub(super) async fn issue(
        &self,
        client_ip: TrustedClientIp,
        email: &str,
        policy: RegistrationPolicyRecord,
        issued_at: u64,
    ) -> Result<IssuedEmailVerification, RegistrationError> {
        if !policy.password_login_enabled() || !policy.enabled() {
            return Err(RegistrationError::Disabled);
        }

        let settings = self
            .email_settings_repository
            .settings()
            .await
            .map_err(map_email_repository_error)?;
        if !settings.delivery_ready() {
            return Err(RegistrationError::EmailNotConfigured);
        }

        let code = generate_verification_code()?;
        let request = EmailDeliveryRequest::registration_verification(
            &settings,
            &self.system_secret_cipher,
            email.to_owned(),
            &code,
            REGISTRATION_EMAIL_TTL_SECONDS,
        )
        .map_err(map_email_request_error)?;
        let subject_fingerprint = self
            .security_key
            .derive(REGISTRATION_EMAIL_SUBJECT_DOMAIN, &[email.as_bytes()]);
        let secret_digest = self.security_key.derive(
            REGISTRATION_EMAIL_SECRET_DOMAIN,
            &[&subject_fingerprint, code.as_bytes()],
        );
        let client_fingerprint = self
            .security_key
            .derive_client(REGISTRATION_EMAIL_CLIENT_DOMAIN, client_ip);

        let rate_limit = AuthChallengeRateLimitClaim::new(
            AuthChallengePurpose::RegistrationEmail,
            subject_fingerprint,
            client_fingerprint,
            issued_at,
            policy.rate_limit_attempts(),
            policy.rate_limit_window_seconds(),
        )
        .map_err(|_| RegistrationError::Internal)?;
        match self
            .rate_limit_repository
            .claim(rate_limit)
            .await
            .map_err(map_rate_limit_repository_error)?
        {
            AuthChallengeRateLimitOutcome::Allowed => {}
            AuthChallengeRateLimitOutcome::RateLimited {
                retry_after_seconds,
            } => {
                return Err(RegistrationError::RateLimited {
                    retry_after_seconds,
                });
            }
        }

        let issue = AuthChallengeIssue::new(
            AuthChallengePurpose::RegistrationEmail,
            subject_fingerprint,
            secret_digest,
            None,
            issued_at,
            REGISTRATION_EMAIL_TTL_SECONDS,
            REGISTRATION_EMAIL_RESEND_COOLDOWN_SECONDS,
            REGISTRATION_EMAIL_MAX_ATTEMPTS,
        )
        .map_err(|_| RegistrationError::Internal)?;
        let issued = match self
            .challenge_repository
            .issue(issue)
            .await
            .map_err(map_challenge_repository_error)?
        {
            AuthChallengeIssueOutcome::Issued(issued) => issued,
            AuthChallengeIssueOutcome::Cooldown {
                retry_after_seconds,
            } => {
                return Err(RegistrationError::RateLimited {
                    retry_after_seconds,
                });
            }
        };

        self.delivery
            .send(request)
            .await
            .map_err(|_| RegistrationError::EmailDeliveryFailed)?;
        Ok(IssuedEmailVerification {
            expires_at: issued.expires_at(),
            next_send_at: issued.next_send_at(),
        })
    }

    pub(super) fn consume(
        &self,
        email: &str,
        code: &str,
        attempted_at: u64,
    ) -> Result<AuthChallengeConsume, RegistrationError> {
        let subject_fingerprint = self
            .security_key
            .derive(REGISTRATION_EMAIL_SUBJECT_DOMAIN, &[email.as_bytes()]);
        let secret_digest = self.security_key.derive(
            REGISTRATION_EMAIL_SECRET_DOMAIN,
            &[&subject_fingerprint, code.as_bytes()],
        );
        AuthChallengeConsume::new(
            AuthChallengePurpose::RegistrationEmail,
            subject_fingerprint,
            secret_digest,
            attempted_at,
        )
        .map_err(|_| RegistrationError::Internal)
    }
}

impl fmt::Debug for RegistrationEmailVerification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RegistrationEmailVerification(<redacted>)")
    }
}

fn generate_verification_code() -> Result<Zeroizing<String>, RegistrationError> {
    const RANGE: u64 = 1_000_000;
    const ACCEPT_BELOW: u64 = u64::MAX - (u64::MAX % RANGE);
    loop {
        let mut entropy = [0_u8; 8];
        getrandom::fill(&mut entropy).map_err(|_| RegistrationError::Internal)?;
        let value = u64::from_le_bytes(entropy);
        entropy.zeroize();
        if value < ACCEPT_BELOW {
            return Ok(Zeroizing::new(format!("{:06}", value % RANGE)));
        }
    }
}

fn map_security_key_error(error: AuthChallengeSecurityKeyError) -> RegistrationServiceConfigError {
    match error {
        AuthChallengeSecurityKeyError::Missing => {
            RegistrationServiceConfigError::MissingSecurityKey
        }
        AuthChallengeSecurityKeyError::Invalid => {
            RegistrationServiceConfigError::InvalidSecurityKey
        }
    }
}

fn map_email_repository_error(error: EmailSettingsRepositoryError) -> RegistrationError {
    match error {
        EmailSettingsRepositoryError::InvalidSettings
        | EmailSettingsRepositoryError::Query
        | EmailSettingsRepositoryError::Timeout
        | EmailSettingsRepositoryError::Invariant => RegistrationError::Internal,
    }
}

fn map_email_request_error(error: AdminEmailSettingsError) -> RegistrationError {
    match error {
        AdminEmailSettingsError::NotConfigured => RegistrationError::EmailNotConfigured,
        AdminEmailSettingsError::InvalidInput
        | AdminEmailSettingsError::Forbidden
        | AdminEmailSettingsError::DeliveryFailed
        | AdminEmailSettingsError::Internal => RegistrationError::Internal,
    }
}

fn map_rate_limit_repository_error(
    error: AuthChallengeRateLimitRepositoryError,
) -> RegistrationError {
    match error {
        AuthChallengeRateLimitRepositoryError::Query
        | AuthChallengeRateLimitRepositoryError::Timeout
        | AuthChallengeRateLimitRepositoryError::Invariant => RegistrationError::Internal,
    }
}

fn map_challenge_repository_error(error: AuthChallengeRepositoryError) -> RegistrationError {
    match error {
        AuthChallengeRepositoryError::Query
        | AuthChallengeRepositoryError::Timeout
        | AuthChallengeRepositoryError::Invariant => RegistrationError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

    use crate::auth_challenge_security::AUTH_CHALLENGE_SECURITY_KEY_BYTES;

    use super::*;

    fn key() -> String {
        URL_SAFE_NO_PAD.encode([0x42; AUTH_CHALLENGE_SECURITY_KEY_BYTES])
    }

    #[test]
    fn keyed_derivation_is_normalized_and_domain_separated() {
        let first = AuthChallengeSecurityKey::new(Some(&key())).unwrap();
        let second_key = URL_SAFE_NO_PAD.encode([0x24; AUTH_CHALLENGE_SECURITY_KEY_BYTES]);
        let second = AuthChallengeSecurityKey::new(Some(&second_key)).unwrap();
        let ipv4 = TrustedClientIp::new("192.0.2.9".parse::<IpAddr>().unwrap());
        let mapped = TrustedClientIp::new("::ffff:192.0.2.9".parse::<IpAddr>().unwrap());

        assert_eq!(
            first.derive_client(REGISTRATION_EMAIL_CLIENT_DOMAIN, ipv4),
            first.derive_client(REGISTRATION_EMAIL_CLIENT_DOMAIN, mapped)
        );
        assert_ne!(
            first.derive_client(REGISTRATION_EMAIL_CLIENT_DOMAIN, ipv4),
            first.derive_client(REGISTRATION_IP_FINGERPRINT_DOMAIN, ipv4)
        );
        assert_ne!(
            first.derive(REGISTRATION_EMAIL_SUBJECT_DOMAIN, &[b"user@example.com"]),
            second.derive(REGISTRATION_EMAIL_SUBJECT_DOMAIN, &[b"user@example.com"])
        );
        assert_eq!(format!("{first:?}"), "AuthChallengeSecurityKey(<redacted>)");
    }

    #[test]
    fn generated_codes_are_fixed_width_decimal_and_redacted_by_owner() {
        for _ in 0..64 {
            let code = generate_verification_code().unwrap();
            assert_eq!(code.len(), REGISTRATION_EMAIL_CODE_BYTES);
            assert!(code.bytes().all(|byte| byte.is_ascii_digit()));
        }
    }
}
