use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    EncryptedCredentialEnvelope, OAuthCredentialIdentityPatch, OAuthCredentialRepository,
    OAuthCredentialRepositoryError, OAuthCredentialTokenUpdateOutcome,
};
use af_domain::{ChannelId, CredentialId};
use thiserror::Error;

use super::{OAuthTokenSet, UpstreamOAuthProvider};
use crate::{CredentialEncryptor, PlainOAuthCredential};

/// OAuth token 集合持久化结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthTokenPersistenceOutcome {
    /// 完整 token 集合已经加密并写入目标凭据。
    Stored,
    /// 目标不存在、已删除、不属于渠道或不是 OAuth 凭据。
    TargetNotFound,
    /// 目标凭据已经绑定其他 provider。
    ProviderMismatch,
}

/// OAuth token 集合持久化错误；不携带 token、scope、密文或数据库诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthTokenPersistenceError {
    /// 当前切片只允许写入已经绑定渠道与凭据的授权上下文。
    #[error("OAuth 授权上下文缺少目标凭据")]
    InvalidContext,
    /// 相对有效期无法安全转换为绝对 Unix 时间。
    #[error("OAuth token 过期时间无效")]
    InvalidExpiration,
    /// token 集合违反加密明文不变量。
    #[error("OAuth token 集合无效")]
    InvalidTokenSet,
    /// token 集合无法形成有效密文封套。
    #[error("OAuth token 加密失败")]
    Encryption,
    /// 数据库操作失败。
    #[error("OAuth token 持久化失败")]
    RepositoryUnavailable,
    /// 数据库操作超过硬截止时间。
    #[error("OAuth token 持久化超时")]
    RepositoryTimeout,
    /// 内部 provider 或持久化状态违反不变量。
    #[error("OAuth token 持久化状态损坏")]
    Invariant,
}

/// 消费 token 交换结果并写入已有 OAuth 凭据的应用服务。
pub struct OAuthTokenPersistenceService {
    encryptor: CredentialEncryptor,
    repository: OAuthCredentialRepository,
}

impl OAuthTokenPersistenceService {
    /// 组合启动期凭据加密器与聚焦数据库仓储。
    #[must_use]
    pub const fn new(
        encryptor: CredentialEncryptor,
        repository: OAuthCredentialRepository,
    ) -> Self {
        Self {
            encryptor,
            repository,
        }
    }

    /// 按值消费完整 token 集合并原子替换已绑定 OAuth 凭据密文。
    pub async fn persist(
        &self,
        token_set: OAuthTokenSet,
    ) -> Result<OAuthTokenPersistenceOutcome, OAuthTokenPersistenceError> {
        let prepared = prepare_token_set(&self.encryptor, token_set, SystemTime::now())?;
        let outcome = self
            .repository
            .replace_token_secret(
                prepared.channel_id,
                prepared.credential_id,
                prepared.provider.as_str(),
                prepared.expires_at_epoch_seconds,
                prepared.envelope,
                prepared.identity,
            )
            .await
            .map_err(map_repository_error)?;
        Ok(match outcome {
            OAuthCredentialTokenUpdateOutcome::Updated => OAuthTokenPersistenceOutcome::Stored,
            OAuthCredentialTokenUpdateOutcome::TargetNotFound => {
                OAuthTokenPersistenceOutcome::TargetNotFound
            }
            OAuthCredentialTokenUpdateOutcome::ProviderMismatch => {
                OAuthTokenPersistenceOutcome::ProviderMismatch
            }
        })
    }
}

impl fmt::Debug for OAuthTokenPersistenceService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthTokenPersistenceService")
            .field("encryptor", &self.encryptor)
            .field("repository", &self.repository)
            .finish()
    }
}

#[derive(Debug)]
struct PreparedOAuthCredential {
    channel_id: ChannelId,
    credential_id: CredentialId,
    provider: UpstreamOAuthProvider,
    expires_at_epoch_seconds: Option<i64>,
    envelope: EncryptedCredentialEnvelope,
    identity: OAuthCredentialIdentityPatch,
}

fn prepare_token_set(
    encryptor: &CredentialEncryptor,
    token_set: OAuthTokenSet,
    now: SystemTime,
) -> Result<PreparedOAuthCredential, OAuthTokenPersistenceError> {
    let context = token_set.context();
    let (Some(channel_id), Some(credential_id)) = (context.channel_id(), context.credential_id())
    else {
        return Err(OAuthTokenPersistenceError::InvalidContext);
    };
    let parts = token_set.into_parts();
    let expires_at_epoch_seconds = parts
        .expires_in
        .map(|expires_in| checked_expiration_epoch_seconds(now, expires_in))
        .transpose()
        .map_err(|()| OAuthTokenPersistenceError::InvalidExpiration)?;
    let provider = parts.provider;
    let (account_key, project_id) = parts.identity.into_parts();
    let identity = OAuthCredentialIdentityPatch::new(account_key, project_id)
        .map_err(|_| OAuthTokenPersistenceError::InvalidTokenSet)?;
    let secret = PlainOAuthCredential::from_zeroizing(
        parts.access_token,
        parts.refresh_token,
        expires_at_epoch_seconds,
        parts.scope,
    )
    .map_err(|_| OAuthTokenPersistenceError::InvalidTokenSet)?;
    let envelope = encryptor
        .encrypt_oauth(channel_id, credential_id.get(), &secret)
        .map_err(|_| OAuthTokenPersistenceError::Encryption)?;
    // 加密完成后立即清零明文，数据库等待期间只保留密文封套。
    drop(secret);
    Ok(PreparedOAuthCredential {
        channel_id,
        credential_id,
        provider,
        expires_at_epoch_seconds,
        envelope,
        identity,
    })
}

pub(super) fn checked_expiration_epoch_seconds(
    now: SystemTime,
    expires_in: std::time::Duration,
) -> Result<i64, ()> {
    let expires_at = now.checked_add(expires_in).ok_or(())?;
    let since_epoch = expires_at.duration_since(UNIX_EPOCH).map_err(|_| ())?;
    i64::try_from(since_epoch.as_secs()).map_err(|_| ())
}

const fn map_repository_error(error: OAuthCredentialRepositoryError) -> OAuthTokenPersistenceError {
    match error {
        OAuthCredentialRepositoryError::Query => OAuthTokenPersistenceError::RepositoryUnavailable,
        OAuthCredentialRepositoryError::Timeout => OAuthTokenPersistenceError::RepositoryTimeout,
        OAuthCredentialRepositoryError::InvalidConfiguration
        | OAuthCredentialRepositoryError::InvalidProvider
        | OAuthCredentialRepositoryError::InvalidExpiration
        | OAuthCredentialRepositoryError::InvalidIdentity
        | OAuthCredentialRepositoryError::Invariant => OAuthTokenPersistenceError::Invariant,
        _ => OAuthTokenPersistenceError::Invariant,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use af_domain::{ChannelId, CredentialId, CredentialKind, UserId};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

    use super::*;
    use crate::{
        CredentialDecryptor,
        oauth::{OAuthAuthorizationContext, identity::OAuthIdentityMetadata},
    };

    const CHANNEL_ID: i64 = 22;
    const CREDENTIAL_ID: i64 = 33;

    #[test]
    fn prepares_complete_tokens_with_checked_absolute_expiration() {
        let settings = settings();
        let encryptor = CredentialEncryptor::new(&settings).unwrap();
        let decryptor = CredentialDecryptor::new(&settings).unwrap();
        let token_set = token_set(
            context(Some(CHANNEL_ID), Some(CREDENTIAL_ID)),
            Some(Duration::from_secs(60)),
        );
        let token_debug = format!("{token_set:?}");
        for private in ["access-private", "refresh-private", "openid profile"] {
            assert!(!token_debug.contains(private));
        }
        let prepared = prepare_token_set(
            &encryptor,
            token_set,
            UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        )
        .unwrap();

        assert_eq!(prepared.channel_id.get(), CHANNEL_ID);
        assert_eq!(prepared.credential_id.get(), CREDENTIAL_ID);
        assert_eq!(prepared.provider, UpstreamOAuthProvider::Codex);
        assert_eq!(prepared.identity.account_key(), Some("account-marker"));
        assert_eq!(prepared.identity.project_id(), None);
        let complete = decryptor
            .decrypt_oauth_envelope(
                prepared.channel_id,
                prepared.credential_id.get(),
                &prepared.envelope,
            )
            .unwrap();
        assert_eq!(complete.access_token(), "access-private");
        assert_eq!(complete.refresh_token(), Some("refresh-private"));
        assert_eq!(complete.expires_at_epoch_seconds(), Some(1_700_000_060));
        assert_eq!(complete.scope(), Some("openid profile"));

        let runtime = decryptor
            .decrypt_envelope(
                prepared.channel_id,
                prepared.credential_id.get(),
                CredentialKind::Oauth,
                &prepared.envelope,
            )
            .unwrap();
        assert_eq!(runtime.expose_secret(), "access-private");
        let rendered = format!("{complete:?}{runtime:?}{:?}", prepared.envelope);
        for private in [
            "access-private",
            "refresh-private",
            "openid profile",
            "test-key",
        ] {
            assert!(!rendered.contains(private));
        }
    }

    #[test]
    fn rejects_unbound_targets_and_expiration_overflow() {
        let encryptor = CredentialEncryptor::new(&settings()).unwrap();
        assert_eq!(
            prepare_token_set(&encryptor, token_set(context(None, None), None), UNIX_EPOCH,)
                .unwrap_err(),
            OAuthTokenPersistenceError::InvalidContext
        );
        assert_eq!(
            prepare_token_set(
                &encryptor,
                token_set(
                    context(Some(CHANNEL_ID), Some(CREDENTIAL_ID)),
                    Some(Duration::from_secs(u64::MAX)),
                ),
                UNIX_EPOCH,
            )
            .unwrap_err(),
            OAuthTokenPersistenceError::InvalidExpiration
        );
        assert!(
            checked_expiration_epoch_seconds(
                UNIX_EPOCH - Duration::from_secs(2),
                Duration::from_secs(1),
            )
            .is_err()
        );
    }

    fn token_set(
        context: OAuthAuthorizationContext,
        expires_in: Option<Duration>,
    ) -> OAuthTokenSet {
        OAuthTokenSet::for_test_with_identity(
            UpstreamOAuthProvider::Codex,
            context,
            "access-private".to_owned(),
            Some("refresh-private".to_owned()),
            expires_in,
            Some("openid profile".to_owned()),
            OAuthIdentityMetadata::for_test(Some("account-marker".to_owned()), None),
        )
    }

    fn context(channel_id: Option<i64>, credential_id: Option<i64>) -> OAuthAuthorizationContext {
        OAuthAuthorizationContext::new(
            UserId::new(11).unwrap(),
            channel_id.map(|value| ChannelId::new(value).unwrap()),
            credential_id.map(|value| CredentialId::new(value).unwrap()),
        )
        .unwrap()
    }

    fn settings() -> CredentialEncryptionSettings {
        serde_json::from_value(serde_json::json!({
            "key_id": "test-key",
            "key": URL_SAFE_NO_PAD.encode([0x5a; CREDENTIAL_ENCRYPTION_KEY_BYTES])
        }))
        .unwrap()
    }
}
