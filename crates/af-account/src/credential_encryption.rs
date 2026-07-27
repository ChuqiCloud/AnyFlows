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
use zeroize::Zeroize as _;

use crate::credential_decryption::{
    CredentialDecryptionError, CredentialKey, MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES,
    credential_plaintext_aad, is_valid_key_id,
};

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
            },
            _ => return Err(CredentialEncryptionError::UnsupportedKind),
        };
        let mut plaintext =
            serde_json::to_vec(&wire).map_err(|_| CredentialEncryptionError::InvalidPlaintext)?;
        let mut nonce = [0_u8; CREDENTIAL_ENVELOPE_NONCE_BYTES];
        if getrandom::fill(&mut nonce).is_err() {
            plaintext.zeroize();
            return Err(CredentialEncryptionError::Entropy);
        }
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| CredentialEncryptionError::InvalidKey)?;
        let result = cipher.encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: &credential_plaintext_aad(channel_id, credential_id, secret.kind()),
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
    Oauth { access_token: &'a str },
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
