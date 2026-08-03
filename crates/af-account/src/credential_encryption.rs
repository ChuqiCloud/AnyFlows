use std::fmt;

use af_config::CredentialEncryptionSettings;
use af_db::{CREDENTIAL_ENVELOPE_NONCE_BYTES, EncryptedCredentialEnvelope};
use af_domain::{ChannelId, CredentialKind};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use serde::Serialize;
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

use crate::credential_decryption::{
    CredentialDecryptionError, CredentialKey, MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES,
    credential_plaintext_aad, is_valid_key_id,
};
use crate::oauth::validate_scope;

/// 凭据加密失败；错误不包含密钥、nonce、密文或明文。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CredentialEncryptionError {
    /// 启动配置缺少完整密钥材料。
    #[error("凭据加密密钥未配置")]
    MissingKey,
    /// 启动密钥标识或密钥材料无效。
    #[error("凭据加密密钥无效")]
    InvalidKey,
    /// 当前写入链路不支持该凭据类型。
    #[error("凭据类型暂不支持写入")]
    UnsupportedKind,
    /// AAD 使用的渠道或凭据标识无效。
    #[error("凭据密文绑定参数无效")]
    InvalidBinding,
    /// 明文凭据违反长度或字符边界。
    #[error("凭据明文无效")]
    InvalidPlaintext,
    /// 系统安全随机数不可用。
    #[error("凭据加密随机数不可用")]
    Entropy,
    /// AEAD 加密失败。
    #[error("凭据加密失败")]
    Encrypt,
    /// 加密结果无法形成有效密文封套。
    #[error("凭据密文封套无效")]
    InvalidEnvelope,
}

/// 待加密的 API Key 或 OAuth access token；默认脱敏并在释放时清零。
pub struct PlainCredentialSecret {
    kind: CredentialKind,
    value: String,
}

impl PlainCredentialSecret {
    /// 校验当前运行时已支持的凭据类型和明文边界。
    pub fn new(kind: CredentialKind, mut value: String) -> Result<Self, CredentialEncryptionError> {
        if !matches!(kind, CredentialKind::ApiKey | CredentialKind::Oauth) {
            value.zeroize();
            return Err(CredentialEncryptionError::UnsupportedKind);
        }
        if value.is_empty()
            || value.len() > MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES
            || value.trim() != value
            || !value.is_ascii()
            || value.bytes().any(|byte| byte.is_ascii_control())
        {
            value.zeroize();
            return Err(CredentialEncryptionError::InvalidPlaintext);
        }
        Ok(Self { kind, value })
    }

    /// 返回凭据声明类型，不暴露明文。
    #[must_use]
    pub const fn kind(&self) -> CredentialKind {
        self.kind
    }

    fn expose(&self) -> &str {
        &self.value
    }
}

impl Drop for PlainCredentialSecret {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

impl fmt::Debug for PlainCredentialSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlainCredentialSecret")
            .field("kind", &self.kind)
            .field("value", &"<已脱敏>")
            .finish()
    }
}

/// 待加密的完整 OAuth token 集合；敏感文本默认脱敏并在释放时清零。
pub struct PlainOAuthCredential {
    access_token: Zeroizing<String>,
    refresh_token: Option<Zeroizing<String>>,
    expires_at_epoch_seconds: Option<i64>,
    scope: Option<Zeroizing<String>>,
}

impl PlainOAuthCredential {
    /// 校验 token、绝对过期时间和 scope 后接管全部明文所有权。
    pub fn new(
        access_token: String,
        refresh_token: Option<String>,
        expires_at_epoch_seconds: Option<i64>,
        scope: Option<String>,
    ) -> Result<Self, CredentialEncryptionError> {
        Self::from_zeroizing(
            Zeroizing::new(access_token),
            refresh_token.map(Zeroizing::new),
            expires_at_epoch_seconds,
            scope.map(Zeroizing::new),
        )
    }

    pub(crate) fn from_zeroizing(
        access_token: Zeroizing<String>,
        refresh_token: Option<Zeroizing<String>>,
        expires_at_epoch_seconds: Option<i64>,
        scope: Option<Zeroizing<String>>,
    ) -> Result<Self, CredentialEncryptionError> {
        if !is_valid_oauth_token(access_token.as_str())
            || refresh_token
                .as_ref()
                .is_some_and(|value| !is_valid_oauth_token(value.as_str()))
            || expires_at_epoch_seconds.is_some_and(|value| value < 0)
            || scope
                .as_ref()
                .is_some_and(|value| validate_scope(value.as_str()).is_err())
        {
            return Err(CredentialEncryptionError::InvalidPlaintext);
        }
        Ok(Self {
            access_token,
            refresh_token,
            expires_at_epoch_seconds,
            scope,
        })
    }
}

impl fmt::Debug for PlainOAuthCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlainOAuthCredential")
            .field("access_token", &"<已脱敏>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<已脱敏>"),
            )
            .field("expires_at_epoch_seconds", &self.expires_at_epoch_seconds)
            .field("scope", &self.scope.as_ref().map(|_| "<已脱敏>"))
            .finish()
    }
}

/// 单密钥 v1 凭据加密器；与生产解密器共享配置和 AAD 契约。
pub struct CredentialEncryptor {
    key_id: String,
    key: CredentialKey,
}

impl CredentialEncryptor {
    /// 从启动配置构造加密器，并立即解码 Base64URL 密钥材料。
    pub fn new(settings: &CredentialEncryptionSettings) -> Result<Self, CredentialEncryptionError> {
        let key_id = settings
            .key_id()
            .ok_or(CredentialEncryptionError::MissingKey)?;
        let encoded_key = settings
            .key()
            .ok_or(CredentialEncryptionError::MissingKey)?;
        if !is_valid_key_id(key_id) {
            return Err(CredentialEncryptionError::InvalidKey);
        }
        let key = CredentialKey::from_encoded(encoded_key.expose()).map_err(map_key_error)?;
        Ok(Self {
            key_id: key_id.to_owned(),
            key,
        })
    }

    /// 使用真实数据库标识生成绑定渠道、凭据和类型的密文封套。
    pub fn encrypt(
        &self,
        channel_id: ChannelId,
        credential_id: i64,
        secret: &PlainCredentialSecret,
    ) -> Result<EncryptedCredentialEnvelope, CredentialEncryptionError> {
        if credential_id <= 0 {
            return Err(CredentialEncryptionError::InvalidBinding);
        }
        let wire = match secret.kind() {
            CredentialKind::ApiKey => PlainCredentialWire::ApiKey {
                api_key: secret.expose(),
            },
            CredentialKind::Oauth => PlainCredentialWire::Oauth {
                access_token: secret.expose(),
                refresh_token: None,
                expires_at_epoch_seconds: None,
                scope: None,
            },
            _ => return Err(CredentialEncryptionError::UnsupportedKind),
        };
        self.encrypt_wire(channel_id, credential_id, secret.kind(), &wire)
    }

    /// 使用既有 v1 封套加密完整 OAuth token 集合。
    pub fn encrypt_oauth(
        &self,
        channel_id: ChannelId,
        credential_id: i64,
        secret: &PlainOAuthCredential,
    ) -> Result<EncryptedCredentialEnvelope, CredentialEncryptionError> {
        let wire = PlainCredentialWire::Oauth {
            access_token: secret.access_token.as_str(),
            refresh_token: secret.refresh_token.as_ref().map(|value| value.as_str()),
            expires_at_epoch_seconds: secret.expires_at_epoch_seconds,
            scope: secret.scope.as_ref().map(|value| value.as_str()),
        };
        self.encrypt_wire(channel_id, credential_id, CredentialKind::Oauth, &wire)
    }

    fn encrypt_wire(
        &self,
        channel_id: ChannelId,
        credential_id: i64,
        credential_kind: CredentialKind,
        wire: &PlainCredentialWire<'_>,
    ) -> Result<EncryptedCredentialEnvelope, CredentialEncryptionError> {
        if credential_id <= 0 {
            return Err(CredentialEncryptionError::InvalidBinding);
        }
        let mut plaintext =
            serde_json::to_vec(wire).map_err(|_| CredentialEncryptionError::InvalidPlaintext)?;
        let mut nonce = [0_u8; CREDENTIAL_ENVELOPE_NONCE_BYTES];
        if getrandom::fill(&mut nonce).is_err() {
            plaintext.zeroize();
            return Err(CredentialEncryptionError::Entropy);
        }
        let Ok(cipher) = XChaCha20Poly1305::new_from_slice(self.key.as_slice()) else {
            plaintext.zeroize();
            return Err(CredentialEncryptionError::InvalidKey);
        };
        let result = cipher.encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: &credential_plaintext_aad(channel_id, credential_id, credential_kind),
            },
        );
        plaintext.zeroize();
        let ciphertext = result.map_err(|_| CredentialEncryptionError::Encrypt)?;
        EncryptedCredentialEnvelope::new(self.key_id.clone(), nonce, ciphertext)
            .map_err(|_| CredentialEncryptionError::InvalidEnvelope)
    }
}

impl fmt::Debug for CredentialEncryptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialEncryptor")
            .field("key_id", &"<已脱敏>")
            .finish_non_exhaustive()
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum PlainCredentialWire<'a> {
    #[serde(rename = "api_key")]
    ApiKey { api_key: &'a str },
    #[serde(rename = "oauth")]
    Oauth {
        access_token: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        refresh_token: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at_epoch_seconds: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<&'a str>,
    },
}

fn is_valid_oauth_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

fn map_key_error(error: CredentialDecryptionError) -> CredentialEncryptionError {
    match error {
        CredentialDecryptionError::MissingKey => CredentialEncryptionError::MissingKey,
        _ => CredentialEncryptionError::InvalidKey,
    }
}

#[cfg(test)]
mod tests {
    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use serde_json::json;

    use super::*;
    use crate::CredentialDecryptor;

    const CHANNEL_ID: i64 = 7;
    const CREDENTIAL_ID: i64 = 11;

    #[test]
    fn encrypts_supported_secrets_for_the_production_decryptor() {
        let settings = settings();
        let encryptor = CredentialEncryptor::new(&settings).unwrap();
        let decryptor = CredentialDecryptor::new(&settings).unwrap();

        for (kind, value) in [
            (CredentialKind::ApiKey, "sk-private"),
            (CredentialKind::Oauth, "ya29.private"),
        ] {
            let secret = PlainCredentialSecret::new(kind, value.to_owned()).unwrap();
            let envelope = encryptor
                .encrypt(channel_id(), CREDENTIAL_ID, &secret)
                .unwrap();
            let decrypted = decryptor
                .decrypt_envelope(channel_id(), CREDENTIAL_ID, kind, &envelope)
                .unwrap();
            assert_eq!(decrypted.expose_secret(), value);
            assert!(!format!("{encryptor:?}{secret:?}{envelope:?}").contains(value));
        }
    }

    #[test]
    fn rejects_unsupported_or_invalid_plaintext_and_wrong_binding() {
        assert_eq!(
            PlainCredentialSecret::new(CredentialKind::Bedrock, "secret".to_owned()).unwrap_err(),
            CredentialEncryptionError::UnsupportedKind
        );
        assert_eq!(
            PlainCredentialSecret::new(CredentialKind::ApiKey, " secret".to_owned()).unwrap_err(),
            CredentialEncryptionError::InvalidPlaintext
        );
        let encryptor = CredentialEncryptor::new(&settings()).unwrap();
        let secret =
            PlainCredentialSecret::new(CredentialKind::ApiKey, "secret".to_owned()).unwrap();
        assert_eq!(
            encryptor.encrypt(channel_id(), 0, &secret).unwrap_err(),
            CredentialEncryptionError::InvalidBinding
        );
        for invalid in [
            PlainOAuthCredential::new("access token".to_owned(), None, None, None),
            PlainOAuthCredential::new("access".to_owned(), None, Some(-1), None),
            PlainOAuthCredential::new(
                "access".to_owned(),
                None,
                None,
                Some("openid  profile".to_owned()),
            ),
        ] {
            assert_eq!(
                invalid.unwrap_err(),
                CredentialEncryptionError::InvalidPlaintext
            );
        }
    }

    #[test]
    fn encrypts_complete_oauth_tokens_and_keeps_runtime_access_only() {
        let settings = settings();
        let encryptor = CredentialEncryptor::new(&settings).unwrap();
        let decryptor = CredentialDecryptor::new(&settings).unwrap();
        let secret = PlainOAuthCredential::new(
            "access-private".to_owned(),
            Some("refresh-private".to_owned()),
            Some(1_800_000_000),
            Some("openid profile".to_owned()),
        )
        .unwrap();
        let envelope = encryptor
            .encrypt_oauth(channel_id(), CREDENTIAL_ID, &secret)
            .unwrap();

        let oauth = decryptor
            .decrypt_oauth_envelope(channel_id(), CREDENTIAL_ID, &envelope)
            .unwrap();
        assert_eq!(oauth.access_token(), "access-private");
        assert_eq!(oauth.refresh_token(), Some("refresh-private"));
        assert_eq!(oauth.expires_at_epoch_seconds(), Some(1_800_000_000));
        assert_eq!(oauth.scope(), Some("openid profile"));

        let runtime = decryptor
            .decrypt_envelope(
                channel_id(),
                CREDENTIAL_ID,
                CredentialKind::Oauth,
                &envelope,
            )
            .unwrap();
        assert_eq!(runtime.expose_secret(), "access-private");
        let rendered = format!("{secret:?}{oauth:?}{runtime:?}{envelope:?}");
        for private in [
            "access-private",
            "refresh-private",
            "openid profile",
            "primary-key",
        ] {
            assert!(!rendered.contains(private));
        }
    }

    fn settings() -> CredentialEncryptionSettings {
        serde_json::from_value(json!({
            "key_id": "primary-key",
            "key": URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES])
        }))
        .unwrap()
    }

    fn channel_id() -> ChannelId {
        ChannelId::new(CHANNEL_ID).unwrap()
    }
}
