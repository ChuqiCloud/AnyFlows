use std::fmt;

use af_config::{
    CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings,
    MAX_CREDENTIAL_ENCRYPTION_KEY_ID_BYTES,
};
use af_db::{ChannelProbeTargetRecord, EncryptedCredentialEnvelope};
use af_domain::{ChannelId, CredentialKind};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use serde::{Deserialize, Deserializer, de::Error as _};
use thiserror::Error;
use zeroize::Zeroize as _;

/// 解密后单个 API Key 或 access token 允许占用的最大字节数。
pub const MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES: usize = 16 * 1_024;

/// 凭据解密失败；错误不包含 key_id、nonce、密文或明文。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CredentialDecryptionError {
    /// 启动配置缺少完整密钥材料。
    #[error("凭据解密密钥未配置")]
    MissingKey,
    /// 启动密钥标识或密钥材料无效。
    #[error("凭据解密密钥无效")]
    InvalidKey,
    /// 密文封套声明的 key_id 与当前启动密钥不一致。
    #[error("凭据密文密钥标识不匹配")]
    KeyMismatch,
    /// 当前切片尚不支持数据库声明的凭据类型。
    #[error("凭据类型暂不支持探活")]
    UnsupportedKind,
    /// 密文绑定参数不满足持久化不变量。
    #[error("凭据密文绑定参数无效")]
    InvalidBinding,
    /// AEAD 认证或解密失败。
    #[error("凭据密文解密失败")]
    Decrypt,
    /// 解密后的 JSON 结构或 secret 字段无效。
    #[error("凭据明文结构无效")]
    InvalidPlaintext,
    /// 数据库声明的凭据类型与明文 JSON kind 不一致。
    #[error("凭据明文类型不匹配")]
    KindMismatch,
}

/// 单密钥 v1 凭据解密器；内部密钥在释放时清零。
#[derive(Clone)]
pub struct CredentialDecryptor {
    key_id: String,
    key: DecryptionKey,
}

impl CredentialDecryptor {
    /// 从启动配置构造解密器，并立即解码 Base64URL 密钥材料。
    pub fn new(settings: &CredentialEncryptionSettings) -> Result<Self, CredentialDecryptionError> {
        let key_id = settings
            .key_id()
            .ok_or(CredentialDecryptionError::MissingKey)?;
        let encoded_key = settings
            .key()
            .ok_or(CredentialDecryptionError::MissingKey)?;
        if !is_valid_key_id(key_id) {
            return Err(CredentialDecryptionError::InvalidKey);
        }
        let key = DecryptionKey::from_encoded(encoded_key.expose())?;
        Ok(Self {
            key_id: key_id.to_owned(),
            key,
        })
    }

    /// 解密数据库探活目标中的凭据封套。
    pub fn decrypt_target(
        &self,
        target: &ChannelProbeTargetRecord,
    ) -> Result<DecryptedCredential, CredentialDecryptionError> {
        self.decrypt_envelope(
            target.channel_id(),
            target.credential_id(),
            target.credential_kind(),
            target.envelope(),
        )
    }

    /// 解密单个凭据封套；测试和未来写入链路可复用同一 AAD 约定。
    pub fn decrypt_envelope(
        &self,
        channel_id: ChannelId,
        credential_id: i64,
        credential_kind: CredentialKind,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedCredential, CredentialDecryptionError> {
        if !matches!(
            credential_kind,
            CredentialKind::ApiKey | CredentialKind::Oauth
        ) {
            return Err(CredentialDecryptionError::UnsupportedKind);
        }
        if credential_id <= 0 {
            return Err(CredentialDecryptionError::InvalidBinding);
        }
        if envelope.key_id() != self.key_id {
            return Err(CredentialDecryptionError::KeyMismatch);
        }

        let aad = credential_plaintext_aad(channel_id, credential_id, credential_kind);
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| CredentialDecryptionError::InvalidKey)?;
        let mut plaintext = cipher
            .decrypt(
                XNonce::from_slice(envelope.nonce()),
                Payload {
                    msg: envelope.ciphertext(),
                    aad: &aad,
                },
            )
            .map_err(|_| CredentialDecryptionError::Decrypt)?;
        let wire = serde_json::from_slice::<PlainCredentialWire>(&plaintext)
            .map_err(|_| CredentialDecryptionError::InvalidPlaintext);
        plaintext.zeroize();

        let (plaintext_kind, secret) = match wire? {
            PlainCredentialWire::ApiKey { api_key } => (CredentialKind::ApiKey, api_key),
            PlainCredentialWire::Oauth { access_token } => (CredentialKind::Oauth, access_token),
        };
        if plaintext_kind != credential_kind {
            return Err(CredentialDecryptionError::KindMismatch);
        }
        Ok(DecryptedCredential {
            kind: plaintext_kind,
            secret,
        })
    }
}

impl fmt::Debug for CredentialDecryptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialDecryptor")
            .field("key_id", &"<已脱敏>")
            .finish_non_exhaustive()
    }
}

/// 构造 AEAD AAD，绑定渠道、凭据与数据库声明类型，避免密文跨记录搬用。
#[must_use]
pub fn credential_plaintext_aad(
    channel_id: ChannelId,
    credential_id: i64,
    credential_kind: CredentialKind,
) -> Vec<u8> {
    format!(
        "anyflows:credential:v1:channel={}:credential={}:kind={}",
        channel_id.get(),
        credential_id,
        credential_kind.as_str()
    )
    .into_bytes()
}

fn is_valid_key_id(key_id: &str) -> bool {
    !key_id.is_empty()
        && key_id.len() <= MAX_CREDENTIAL_ENCRYPTION_KEY_ID_BYTES
        && key_id.trim() == key_id
        && key_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[derive(Clone)]
struct DecryptionKey([u8; CREDENTIAL_ENCRYPTION_KEY_BYTES]);

impl DecryptionKey {
    fn from_encoded(value: &str) -> Result<Self, CredentialDecryptionError> {
        let mut decoded = [0_u8; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let valid = URL_SAFE_NO_PAD
            .decode_slice(value, &mut decoded)
            .is_ok_and(|length| length == CREDENTIAL_ENCRYPTION_KEY_BYTES);
        if !valid {
            decoded.zeroize();
            return Err(CredentialDecryptionError::InvalidKey);
        }
        Ok(Self(decoded))
    }

    fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for DecryptionKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum PlainCredentialWire {
    #[serde(rename = "api_key")]
    ApiKey { api_key: SecretText },
    #[serde(rename = "oauth")]
    Oauth { access_token: SecretText },
}

/// 已解密但默认脱敏并在释放时清零的凭据。
pub struct DecryptedCredential {
    kind: CredentialKind,
    secret: SecretText,
}

impl DecryptedCredential {
    /// 返回明文声明的凭据类型。
    #[must_use]
    pub const fn kind(&self) -> CredentialKind {
        self.kind
    }

    /// 返回认证载荷；调用方只应短暂用于构造上游认证头。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.secret.expose()
    }
}

impl fmt::Debug for DecryptedCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DecryptedCredential")
            .field("kind", &self.kind())
            .field("secret", &"<已脱敏>")
            .finish()
    }
}

/// 明文 secret 字段；校验边界与当前适配器凭据保持一致。
struct SecretText(String);

impl SecretText {
    fn new(mut value: String) -> Result<Self, CredentialDecryptionError> {
        if value.is_empty()
            || value.len() > MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES
            || value.trim() != value
            || !value.is_ascii()
            || value.bytes().any(|byte| byte.is_ascii_control())
        {
            value.zeroize();
            return Err(CredentialDecryptionError::InvalidPlaintext);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SecretText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl Drop for SecretText {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SecretText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<已脱敏>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_config::CredentialEncryptionSettings;
    use serde_json::json;

    const CHANNEL_ID: i64 = 7;
    const CREDENTIAL_ID: i64 = 11;

    #[test]
    fn decrypts_api_key_and_redacts_debug_output() {
        let key = [0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let decryptor = decryptor(&key, "primary-key");
        let envelope = encrypted_envelope(
            &key,
            "primary-key",
            CredentialKind::ApiKey,
            br#"{"kind":"api_key","api_key":"sk-test-secret"}"#,
        );

        let credential = decryptor
            .decrypt_envelope(
                channel_id(),
                CREDENTIAL_ID,
                CredentialKind::ApiKey,
                &envelope,
            )
            .unwrap();

        assert_eq!(credential.kind(), CredentialKind::ApiKey);
        assert_eq!(credential.expose_secret(), "sk-test-secret");
        for rendered in [format!("{decryptor:?}"), format!("{credential:?}")] {
            assert!(!rendered.contains("primary-key"));
            assert!(!rendered.contains("sk-test-secret"));
        }
    }

    #[test]
    fn decrypts_oauth_access_token() {
        let key = [0x24; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let decryptor = decryptor(&key, "oauth-key");
        let envelope = encrypted_envelope(
            &key,
            "oauth-key",
            CredentialKind::Oauth,
            br#"{"kind":"oauth","access_token":"ya29.test-token"}"#,
        );

        let credential = decryptor
            .decrypt_envelope(
                channel_id(),
                CREDENTIAL_ID,
                CredentialKind::Oauth,
                &envelope,
            )
            .unwrap();

        assert_eq!(credential.kind(), CredentialKind::Oauth);
        assert_eq!(credential.expose_secret(), "ya29.test-token");
    }

    #[test]
    fn rejects_wrong_key_id_tampered_aad_and_kind_mismatch() {
        let key = [0x66; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let decryptor = decryptor(&key, "primary-key");
        let envelope = encrypted_envelope(
            &key,
            "primary-key",
            CredentialKind::ApiKey,
            br#"{"kind":"api_key","api_key":"sk-test-secret"}"#,
        );
        let wrong_key = encrypted_envelope(
            &key,
            "other-key",
            CredentialKind::ApiKey,
            br#"{"kind":"api_key","api_key":"sk-test-secret"}"#,
        );
        let kind_mismatch = encrypted_envelope(
            &key,
            "primary-key",
            CredentialKind::Oauth,
            br#"{"kind":"api_key","api_key":"sk-test-secret"}"#,
        );

        assert_eq!(
            decryptor
                .decrypt_envelope(
                    channel_id(),
                    CREDENTIAL_ID,
                    CredentialKind::ApiKey,
                    &wrong_key,
                )
                .unwrap_err(),
            CredentialDecryptionError::KeyMismatch
        );
        assert_eq!(
            decryptor
                .decrypt_envelope(
                    channel_id(),
                    CREDENTIAL_ID + 1,
                    CredentialKind::ApiKey,
                    &envelope,
                )
                .unwrap_err(),
            CredentialDecryptionError::Decrypt
        );
        assert_eq!(
            decryptor
                .decrypt_envelope(
                    channel_id(),
                    CREDENTIAL_ID,
                    CredentialKind::Oauth,
                    &kind_mismatch,
                )
                .unwrap_err(),
            CredentialDecryptionError::KindMismatch
        );
    }

    #[test]
    fn rejects_invalid_configuration_and_plaintext_shape() {
        let missing = serde_json::from_value::<CredentialEncryptionSettings>(json!({})).unwrap();
        assert_eq!(
            CredentialDecryptor::new(&missing).unwrap_err(),
            CredentialDecryptionError::MissingKey
        );

        let invalid_key = serde_json::from_value::<CredentialEncryptionSettings>(json!({
            "key_id": "primary-key",
            "key": "not-base64url"
        }))
        .unwrap();
        assert_eq!(
            CredentialDecryptor::new(&invalid_key).unwrap_err(),
            CredentialDecryptionError::InvalidKey
        );

        let key = [0x77; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let decryptor = decryptor(&key, "primary-key");
        for invalid_plaintext in [
            br#"{"kind":"api_key","api_key":" secret"}"#.as_slice(),
            br#"{"kind":"api_key","api_key":"sk-test","extra":true}"#.as_slice(),
            r#"{"kind":"oauth","access_token":"非ASCII"}"#.as_bytes(),
        ] {
            let envelope = encrypted_envelope(
                &key,
                "primary-key",
                CredentialKind::ApiKey,
                invalid_plaintext,
            );
            assert_eq!(
                decryptor
                    .decrypt_envelope(
                        channel_id(),
                        CREDENTIAL_ID,
                        CredentialKind::ApiKey,
                        &envelope,
                    )
                    .unwrap_err(),
                CredentialDecryptionError::InvalidPlaintext
            );
        }
    }

    fn decryptor(key: &[u8; CREDENTIAL_ENCRYPTION_KEY_BYTES], key_id: &str) -> CredentialDecryptor {
        let settings = serde_json::from_value::<CredentialEncryptionSettings>(json!({
            "key_id": key_id,
            "key": URL_SAFE_NO_PAD.encode(key)
        }))
        .unwrap();
        CredentialDecryptor::new(&settings).unwrap()
    }

    fn encrypted_envelope(
        key: &[u8; CREDENTIAL_ENCRYPTION_KEY_BYTES],
        key_id: &str,
        aad_kind: CredentialKind,
        plaintext: &[u8],
    ) -> EncryptedCredentialEnvelope {
        let nonce = [0x5a; 24];
        let cipher = XChaCha20Poly1305::new_from_slice(key).unwrap();
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: &credential_plaintext_aad(channel_id(), CREDENTIAL_ID, aad_kind),
                },
            )
            .unwrap();
        EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext).unwrap()
    }

    fn channel_id() -> ChannelId {
        ChannelId::new(CHANNEL_ID).unwrap()
    }
}
