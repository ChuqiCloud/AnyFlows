use std::fmt;

use af_config::CredentialEncryptionSettings;
use af_db::{CREDENTIAL_ENVELOPE_NONCE_BYTES, EncryptedCredentialEnvelope};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use thiserror::Error;
use zeroize::Zeroize as _;

use crate::credential_decryption::{CredentialKey, is_valid_key_id};

const MAX_SYSTEM_SECRET_BYTES: usize = 4 * 1_024;

/// 系统密钥用途参与 AAD，避免不同业务域之间搬用密文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemSecretKind {
    /// 系统 SMTP 认证密码。
    SmtpPassword,
}

impl SystemSecretKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::SmtpPassword => "smtp_password",
        }
    }
}

/// 系统运行时密钥加解密失败；不携带明文、密文或密钥标识。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SystemSecretError {
    #[error("系统密钥加密材料未配置")]
    MissingKey,
    #[error("系统密钥加密材料无效")]
    InvalidKey,
    #[error("系统密钥明文无效")]
    InvalidPlaintext,
    #[error("系统安全随机数不可用")]
    Entropy,
    #[error("系统密钥加密失败")]
    Encrypt,
    #[error("系统密钥密文封套无效")]
    InvalidEnvelope,
    #[error("系统密钥密钥标识不匹配")]
    KeyMismatch,
    #[error("系统密钥解密失败")]
    Decrypt,
}

/// 待加密系统密钥；默认脱敏并在释放时清零。
pub struct PlainSystemSecret {
    value: String,
}

impl PlainSystemSecret {
    /// 校验系统密钥明文的有界 UTF-8 形状。
    pub fn new(mut value: String) -> Result<Self, SystemSecretError> {
        if !valid_plaintext(&value) {
            value.zeroize();
            return Err(SystemSecretError::InvalidPlaintext);
        }
        Ok(Self { value })
    }

    fn expose(&self) -> &str {
        &self.value
    }
}

impl Drop for PlainSystemSecret {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

impl fmt::Debug for PlainSystemSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlainSystemSecret(<已脱敏>)")
    }
}

/// 已解密系统密钥；仅投递适配器可读取，释放时立即清零。
pub struct DecryptedSystemSecret {
    value: String,
}

impl DecryptedSystemSecret {
    /// 暴露明文给最终协议适配器；调用方不得克隆或记录。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.value
    }
}

impl Drop for DecryptedSystemSecret {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

impl fmt::Debug for DecryptedSystemSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DecryptedSystemSecret(<已脱敏>)")
    }
}

/// 复用启动期凭据主密钥的系统密钥加解密器，并使用独立 AAD 域隔离。
#[derive(Clone)]
pub struct SystemSecretCipher {
    key_id: String,
    key: CredentialKey,
}

impl SystemSecretCipher {
    /// 从已校验启动配置构造系统密钥加解密器。
    pub fn new(settings: &CredentialEncryptionSettings) -> Result<Self, SystemSecretError> {
        let key_id = settings.key_id().ok_or(SystemSecretError::MissingKey)?;
        let encoded_key = settings.key().ok_or(SystemSecretError::MissingKey)?;
        if !is_valid_key_id(key_id) {
            return Err(SystemSecretError::InvalidKey);
        }
        let key = CredentialKey::from_encoded(encoded_key.expose())
            .map_err(|_| SystemSecretError::InvalidKey)?;
        Ok(Self {
            key_id: key_id.to_owned(),
            key,
        })
    }

    /// 使用系统邮件专用 AAD 生成密文封套。
    pub fn encrypt(
        &self,
        kind: SystemSecretKind,
        secret: &PlainSystemSecret,
    ) -> Result<EncryptedCredentialEnvelope, SystemSecretError> {
        let mut nonce = [0_u8; CREDENTIAL_ENVELOPE_NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| SystemSecretError::Entropy)?;
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| SystemSecretError::InvalidKey)?;
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: secret.expose().as_bytes(),
                    aad: &system_secret_aad(kind),
                },
            )
            .map_err(|_| SystemSecretError::Encrypt)?;
        EncryptedCredentialEnvelope::new(self.key_id.clone(), nonce, ciphertext)
            .map_err(|_| SystemSecretError::InvalidEnvelope)
    }

    /// 验证密钥标识和 AAD 后解密系统邮件密码。
    pub fn decrypt(
        &self,
        kind: SystemSecretKind,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedSystemSecret, SystemSecretError> {
        if envelope.key_id() != self.key_id {
            return Err(SystemSecretError::KeyMismatch);
        }
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| SystemSecretError::InvalidKey)?;
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(envelope.nonce()),
                Payload {
                    msg: envelope.ciphertext(),
                    aad: &system_secret_aad(kind),
                },
            )
            .map_err(|_| SystemSecretError::Decrypt)?;
        let value = String::from_utf8(plaintext).map_err(|error| {
            let mut plaintext = error.into_bytes();
            plaintext.zeroize();
            SystemSecretError::InvalidPlaintext
        })?;
        if !valid_plaintext(&value) {
            let mut value = value;
            value.zeroize();
            return Err(SystemSecretError::InvalidPlaintext);
        }
        Ok(DecryptedSystemSecret { value })
    }
}

impl fmt::Debug for SystemSecretCipher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SystemSecretCipher")
            .field("key_id", &"<已脱敏>")
            .finish_non_exhaustive()
    }
}

fn system_secret_aad(kind: SystemSecretKind) -> Vec<u8> {
    format!(
        "anyflows:system-secret:v1:record=email_settings:1:kind={}",
        kind.as_str()
    )
    .into_bytes()
}

fn valid_plaintext(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SYSTEM_SECRET_BYTES
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use serde_json::json;

    use super::*;

    #[test]
    fn round_trips_smtp_password_without_debug_leakage() {
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let secret = PlainSystemSecret::new("mail secret with spaces".to_owned()).unwrap();
        let envelope = cipher
            .encrypt(SystemSecretKind::SmtpPassword, &secret)
            .unwrap();
        let decrypted = cipher
            .decrypt(SystemSecretKind::SmtpPassword, &envelope)
            .unwrap();
        assert_eq!(decrypted.expose_secret(), "mail secret with spaces");
        let rendered = format!("{cipher:?}{secret:?}{envelope:?}{decrypted:?}");
        assert!(!rendered.contains("mail secret"));
    }

    #[test]
    fn rejects_wrong_key_and_invalid_plaintext() {
        assert!(PlainSystemSecret::new(String::new()).is_err());
        assert!(PlainSystemSecret::new("bad\nsecret".to_owned()).is_err());
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let secret = PlainSystemSecret::new("smtp-password".to_owned()).unwrap();
        let envelope = cipher
            .encrypt(SystemSecretKind::SmtpPassword, &secret)
            .unwrap();
        let other = SystemSecretCipher::new(&settings_with_key_id("other-key")).unwrap();
        assert_eq!(
            other
                .decrypt(SystemSecretKind::SmtpPassword, &envelope)
                .unwrap_err(),
            SystemSecretError::KeyMismatch
        );
    }

    fn settings() -> CredentialEncryptionSettings {
        settings_with_key_id("primary-key")
    }

    fn settings_with_key_id(key_id: &str) -> CredentialEncryptionSettings {
        serde_json::from_value(json!({
            "key_id": key_id,
            "key": URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES])
        }))
        .unwrap()
    }
}
