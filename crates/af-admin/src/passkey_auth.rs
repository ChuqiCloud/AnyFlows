use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_account::{PlainSystemSecret, SystemSecretCipher, SystemSecretKind};
use af_db::{
    AuthChallengePurpose, AuthChallengeRateLimitClaim, AuthChallengeRateLimitOutcome,
    AuthChallengeRateLimitRepository, AuthChallengeRateLimitRepositoryError,
    PasskeyAuthenticationOutcome, PasskeyRepository, PasskeyRepositoryError,
};
use af_domain::{TrustedClientIp, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use webauthn_rs::prelude::{
    Passkey, PasskeyAuthentication, PublicKeyCredential, Uuid, Webauthn, WebauthnError,
};

use crate::{
    RegistrationService,
    auth_challenge_security::{AuthChallengeSecurityKey, AuthChallengeSecurityKeyError},
};

const MAX_USERNAME_BYTES: usize = 64;
const MAX_CREDENTIAL_JSON_BYTES: usize = 64 * 1_024;
const AUTHENTICATION_STATE_VERSION: u8 = 1;
const AUTHENTICATION_STATE_PURPOSE: &str = "passkey_authentication";
const PASSKEY_AUTHENTICATION_RATE_LIMIT_ATTEMPTS: u32 = 20;
const PASSKEY_AUTHENTICATION_RATE_LIMIT_WINDOW_SECONDS: u64 = 60;
const PASSKEY_AUTHENTICATION_SUBJECT_DOMAIN: &[u8] =
    b"AnyFlows passkey authentication username subject v1";
const PASSKEY_AUTHENTICATION_CLIENT_DOMAIN: &[u8] = b"AnyFlows passkey authentication client v1";

/// 浏览器 Passkey 登录选项及其短期关联摘要。
#[derive(Clone, Debug, PartialEq)]
pub struct PasskeyAuthenticationOptions {
    options: Value,
    challenge_digest: String,
}

impl PasskeyAuthenticationOptions {
    /// 返回浏览器需要的 `PublicKeyCredentialRequestOptions`。
    #[must_use]
    pub fn options(&self) -> &Value {
        &self.options
    }

    /// 返回服务端使用的挑战摘要。
    #[must_use]
    pub fn challenge_digest(&self) -> &str {
        &self.challenge_digest
    }
}

/// 浏览器提交的 Passkey 认证响应。
pub struct PasskeyAuthenticationCommand {
    credential: Value,
}

impl PasskeyAuthenticationCommand {
    /// 校验认证响应大小和 JSON 形状，拒绝未知顶层类型。
    pub fn new(credential: Value) -> Result<Self, PasskeyAuthenticationError> {
        if !credential.is_object()
            || serde_json::to_vec(&credential)
                .map_or(true, |bytes| bytes.len() > MAX_CREDENTIAL_JSON_BYTES)
        {
            return Err(PasskeyAuthenticationError::InvalidInput);
        }
        Ok(Self { credential })
    }

    fn into_credential(self) -> Value {
        self.credential
    }
}

impl fmt::Debug for PasskeyAuthenticationCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasskeyAuthenticationCommand(<redacted>)")
    }
}

/// Passkey 登录错误；认证失败不区分用户、挑战和凭证状态。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PasskeyAuthenticationError {
    #[error("Passkey 登录输入无效")]
    InvalidInput,
    #[error("用户名密码登录策略当前关闭")]
    LoginDisabled,
    #[error("Passkey 登录验证失败")]
    Rejected,
    #[error("Passkey 登录内部失败")]
    Internal,
}

/// Passkey 登录服务的启动配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PasskeyAuthenticationServiceConfigError {
    /// 缺少认证限流摘要派生密钥。
    #[error("Passkey 登录安全派生密钥缺失")]
    MissingSecurityKey,
    /// 认证限流摘要派生密钥格式无效。
    #[error("Passkey 登录安全派生密钥无效")]
    InvalidSecurityKey,
}

/// 创建 Passkey 登录挑战的 Future。
pub type PasskeyAuthenticationOptionsFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PasskeyAuthenticationOptions, PasskeyAuthenticationError>>
            + Send
            + 'a,
    >,
>;

/// 完成 Passkey 登录校验并返回用户 ID 的 Future。
pub type PasskeyAuthenticationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserId, PasskeyAuthenticationError>> + Send + 'a>>;

/// 用户名优先 Passkey 登录应用服务。
pub trait PasskeyAuthenticationService: Send + Sync {
    /// 为活动账户创建一次性认证挑战。
    fn start<'a>(
        &'a self,
        client_ip: TrustedClientIp,
        username: &'a str,
    ) -> PasskeyAuthenticationOptionsFuture<'a>;

    /// 校验浏览器响应并原子消费挑战。
    fn finish<'a>(
        &'a self,
        command: PasskeyAuthenticationCommand,
    ) -> PasskeyAuthenticationFuture<'a>;
}

/// 使用数据库挑战仓储和受信 WebAuthn 配置的生产实现。
pub struct DatabasePasskeyAuthenticationService {
    repository: PasskeyRepository,
    cipher: SystemSecretCipher,
    webauthn: Webauthn,
    registration_service: Arc<dyn RegistrationService>,
    rate_limit_repository: AuthChallengeRateLimitRepository,
    security_key: AuthChallengeSecurityKey,
}

impl DatabasePasskeyAuthenticationService {
    /// 绑定启动期校验过的 Passkey 运行时和认证策略。
    pub fn new(
        repository: PasskeyRepository,
        cipher: SystemSecretCipher,
        webauthn: Webauthn,
        registration_service: Arc<dyn RegistrationService>,
        rate_limit_repository: AuthChallengeRateLimitRepository,
        security_key: Option<&str>,
    ) -> Result<Self, PasskeyAuthenticationServiceConfigError> {
        Ok(Self {
            repository,
            cipher,
            webauthn,
            registration_service,
            rate_limit_repository,
            security_key: AuthChallengeSecurityKey::new(security_key)
                .map_err(map_security_key_error)?,
        })
    }
}

impl PasskeyAuthenticationService for DatabasePasskeyAuthenticationService {
    fn start<'a>(
        &'a self,
        client_ip: TrustedClientIp,
        username: &'a str,
    ) -> PasskeyAuthenticationOptionsFuture<'a> {
        Box::pin(async move {
            if !valid_username(username) {
                return Err(PasskeyAuthenticationError::Rejected);
            }
            let status = self
                .registration_service
                .status()
                .await
                .map_err(|_| PasskeyAuthenticationError::Internal)?;
            if !status.password_login_enabled() {
                return Err(PasskeyAuthenticationError::LoginDisabled);
            }
            let attempted_at = current_timestamp().ok_or(PasskeyAuthenticationError::Internal)?;
            let rate_limit = AuthChallengeRateLimitClaim::new(
                AuthChallengePurpose::PasskeyAuthentication,
                self.security_key.derive(
                    PASSKEY_AUTHENTICATION_SUBJECT_DOMAIN,
                    &[username.as_bytes()],
                ),
                self.security_key
                    .derive_client(PASSKEY_AUTHENTICATION_CLIENT_DOMAIN, client_ip),
                attempted_at,
                PASSKEY_AUTHENTICATION_RATE_LIMIT_ATTEMPTS,
                PASSKEY_AUTHENTICATION_RATE_LIMIT_WINDOW_SECONDS,
            )
            .map_err(|_| PasskeyAuthenticationError::Internal)?;
            if matches!(
                self.rate_limit_repository
                    .claim(rate_limit)
                    .await
                    .map_err(map_rate_limit_repository_error)?,
                AuthChallengeRateLimitOutcome::RateLimited { .. }
            ) {
                return Err(PasskeyAuthenticationError::Rejected);
            }
            let target = self
                .repository
                .authentication_target(username)
                .await
                .map_err(map_repository_error)?
                .ok_or(PasskeyAuthenticationError::Rejected)?;
            let credentials = target
                .credentials()
                .iter()
                .cloned()
                .map(serde_json::from_value::<Passkey>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| PasskeyAuthenticationError::Rejected)?;
            let (request, state) = self
                .webauthn
                .start_passkey_authentication(&credentials)
                .map_err(|_| PasskeyAuthenticationError::Rejected)?;
            let challenge_digest = challenge_digest(request.public_key.challenge.as_ref());
            let wrapped = AuthenticationStateEnvelope {
                version: AUTHENTICATION_STATE_VERSION,
                purpose: AUTHENTICATION_STATE_PURPOSE.to_owned(),
                user_id: target.user_id().get(),
                session_version: target.session_version(),
                state,
            };
            let serialized = serde_json::to_string(&wrapped)
                .map_err(|_| PasskeyAuthenticationError::Internal)?;
            let plaintext = PlainSystemSecret::new(serialized)
                .map_err(|_| PasskeyAuthenticationError::Internal)?;
            let encrypted = self
                .cipher
                .encrypt(
                    SystemSecretKind::PasskeyAuthenticationState(target.user_id()),
                    &plaintext,
                )
                .map_err(|_| PasskeyAuthenticationError::Internal)?;
            let now = af_db::DatabaseTimestamp::now_utc();
            self.repository
                .replace_authentication_challenge(
                    target.user_id(),
                    target.session_version(),
                    challenge_digest.clone(),
                    encrypted,
                    now + Duration::from_secs(5 * 60),
                    now,
                )
                .await
                .map_err(map_repository_error)?;
            let options =
                serde_json::to_value(request).map_err(|_| PasskeyAuthenticationError::Internal)?;
            Ok(PasskeyAuthenticationOptions {
                options,
                challenge_digest,
            })
        })
    }

    fn finish<'a>(
        &'a self,
        command: PasskeyAuthenticationCommand,
    ) -> PasskeyAuthenticationFuture<'a> {
        Box::pin(async move {
            let status = self
                .registration_service
                .status()
                .await
                .map_err(|_| PasskeyAuthenticationError::Internal)?;
            if !status.password_login_enabled() {
                return Err(PasskeyAuthenticationError::LoginDisabled);
            }
            let credential_json = command.into_credential();
            let credential: PublicKeyCredential =
                serde_json::from_value(credential_json.clone())
                    .map_err(|_| PasskeyAuthenticationError::Rejected)?;
            let challenge_digest =
                authentication_response_digest(credential.response.client_data_json.as_ref())
                    .ok_or(PasskeyAuthenticationError::Rejected)?;
            let challenge = self
                .repository
                .load_authentication_challenge(
                    &challenge_digest,
                    af_db::DatabaseTimestamp::now_utc(),
                )
                .await
                .map_err(map_repository_error)?;
            let plaintext = self
                .cipher
                .decrypt(
                    SystemSecretKind::PasskeyAuthenticationState(challenge.user_id()),
                    challenge.state(),
                )
                .map_err(|_| PasskeyAuthenticationError::Rejected)?;
            let wrapped: AuthenticationStateEnvelope =
                serde_json::from_str(plaintext.expose_secret())
                    .map_err(|_| PasskeyAuthenticationError::Rejected)?;
            if wrapped.version != AUTHENTICATION_STATE_VERSION
                || wrapped.purpose != AUTHENTICATION_STATE_PURPOSE
                || wrapped.user_id != challenge.user_id().get()
                || wrapped.session_version != challenge.session_version()
            {
                return Err(PasskeyAuthenticationError::Rejected);
            }
            let expected_user_id = passkey_user_uuid(challenge.user_id());
            if !user_handle_matches(credential.get_user_unique_id(), expected_user_id.as_bytes()) {
                return Err(PasskeyAuthenticationError::Rejected);
            }
            let result = match self
                .webauthn
                .finish_passkey_authentication(&credential, &wrapped.state)
            {
                Ok(result) => result,
                Err(WebauthnError::CredentialPossibleCompromise) => {
                    // 库已完成签名、Origin、RP ID 和用户验证检查，计数回退才会进入此分支。
                    let credential_id = URL_SAFE_NO_PAD.encode(credential.get_credential_id());
                    self.repository
                        .consume_authentication_anomaly(
                            &challenge_digest,
                            challenge.user_id(),
                            challenge.session_version(),
                            credential_id,
                            af_db::DatabaseTimestamp::now_utc(),
                        )
                        .await
                        .map_err(map_repository_error)?;
                    return Err(PasskeyAuthenticationError::Rejected);
                }
                Err(_) => return Err(PasskeyAuthenticationError::Rejected),
            };
            if !result.user_verified() {
                return Err(PasskeyAuthenticationError::Rejected);
            }
            let target = self
                .repository
                .authentication_target_for_user(challenge.user_id())
                .await
                .map_err(map_repository_error)?
                .ok_or(PasskeyAuthenticationError::Rejected)?;
            let mut matched = None;
            for value in target.credentials() {
                let mut passkey: Passkey = serde_json::from_value(value.clone())
                    .map_err(|_| PasskeyAuthenticationError::Rejected)?;
                if passkey.cred_id() == result.cred_id() {
                    passkey.update_credential(&result);
                    matched = Some(passkey);
                    break;
                }
            }
            let passkey = matched.ok_or(PasskeyAuthenticationError::Rejected)?;
            let passkey_json =
                serde_json::to_value(passkey).map_err(|_| PasskeyAuthenticationError::Rejected)?;
            let credential_id = URL_SAFE_NO_PAD.encode(credential.get_credential_id());
            let outcome = self
                .repository
                .consume_authentication_challenge(
                    &challenge_digest,
                    challenge.user_id(),
                    challenge.session_version(),
                    credential_id,
                    passkey_json,
                    i64::from(result.counter()),
                    af_db::DatabaseTimestamp::now_utc(),
                )
                .await
                .map_err(map_repository_error)?;
            match outcome {
                PasskeyAuthenticationOutcome::Authenticated => Ok(challenge.user_id()),
                PasskeyAuthenticationOutcome::Rejected
                | PasskeyAuthenticationOutcome::AnomalyDetected => {
                    Err(PasskeyAuthenticationError::Rejected)
                }
            }
        })
    }
}

fn map_security_key_error(
    error: AuthChallengeSecurityKeyError,
) -> PasskeyAuthenticationServiceConfigError {
    match error {
        AuthChallengeSecurityKeyError::Missing => {
            PasskeyAuthenticationServiceConfigError::MissingSecurityKey
        }
        AuthChallengeSecurityKeyError::Invalid => {
            PasskeyAuthenticationServiceConfigError::InvalidSecurityKey
        }
    }
}

fn map_rate_limit_repository_error(
    error: AuthChallengeRateLimitRepositoryError,
) -> PasskeyAuthenticationError {
    match error {
        AuthChallengeRateLimitRepositoryError::Query
        | AuthChallengeRateLimitRepositoryError::Timeout
        | AuthChallengeRateLimitRepositoryError::Invariant => PasskeyAuthenticationError::Internal,
    }
}

impl fmt::Debug for DatabasePasskeyAuthenticationService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabasePasskeyAuthenticationService(<redacted>)")
    }
}

#[derive(Deserialize, Serialize)]
struct AuthenticationStateEnvelope {
    version: u8,
    purpose: String,
    user_id: i64,
    session_version: i64,
    state: PasskeyAuthentication,
}

fn map_repository_error(error: PasskeyRepositoryError) -> PasskeyAuthenticationError {
    match error {
        PasskeyRepositoryError::NotFound
        | PasskeyRepositoryError::Expired
        | PasskeyRepositoryError::Consumed
        | PasskeyRepositoryError::Conflict => PasskeyAuthenticationError::Rejected,
        PasskeyRepositoryError::InvalidConfiguration
        | PasskeyRepositoryError::Query
        | PasskeyRepositoryError::Timeout
        | PasskeyRepositoryError::Invariant => PasskeyAuthenticationError::Internal,
    }
}

fn valid_username(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_USERNAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn current_timestamp() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
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

fn authentication_response_digest(client_data_json: &[u8]) -> Option<String> {
    let client_data: Value = serde_json::from_slice(client_data_json).ok()?;
    if client_data.get("type")?.as_str()? != "webauthn.get" {
        return None;
    }
    let challenge = URL_SAFE_NO_PAD
        .decode(client_data.get("challenge")?.as_str()?)
        .ok()?;
    Some(challenge_digest(&challenge))
}

fn user_handle_matches(actual: Option<&[u8]>, expected: &[u8]) -> bool {
    // allowCredentials 非空时认证器可以省略 userHandle；若返回则必须严格绑定当前挑战用户。
    actual.is_none_or(|actual| actual == expected)
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use serde_json::json;

    use super::{
        PasskeyAuthenticationCommand, PasskeyAuthenticationError, authentication_response_digest,
        challenge_digest, user_handle_matches,
    };

    #[test]
    fn omitted_user_handle_is_allowed_but_present_value_must_match() {
        let expected = [0x11_u8; 16];
        assert!(user_handle_matches(None, &expected));
        assert!(user_handle_matches(Some(&expected), &expected));
        assert!(!user_handle_matches(Some(&[0x22_u8; 16]), &expected));
    }

    #[test]
    fn client_data_digest_requires_the_authentication_type() {
        let challenge = b"passkey-authentication-challenge";
        let encoded = URL_SAFE_NO_PAD.encode(challenge);
        let valid = serde_json::to_vec(&json!({
            "type": "webauthn.get",
            "challenge": encoded,
            "origin": "https://console.example",
        }))
        .unwrap();
        assert_eq!(
            authentication_response_digest(&valid),
            Some(challenge_digest(challenge))
        );

        let registration = serde_json::to_vec(&json!({
            "type": "webauthn.create",
            "challenge": URL_SAFE_NO_PAD.encode(challenge),
        }))
        .unwrap();
        assert_eq!(authentication_response_digest(&registration), None);
    }

    #[test]
    fn authentication_command_rejects_non_object_credentials() {
        assert_eq!(
            PasskeyAuthenticationCommand::new(json!([])).unwrap_err(),
            PasskeyAuthenticationError::InvalidInput
        );
        assert!(
            !format!(
                "{:?}",
                PasskeyAuthenticationCommand::new(json!({})).unwrap()
            )
            .contains("credential")
        );
    }
}
