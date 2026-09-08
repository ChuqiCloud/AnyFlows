use std::fmt;

use af_config::CredentialEncryptionSettings;
use af_db::{CREDENTIAL_ENVELOPE_NONCE_BYTES, EncryptedCredentialEnvelope};
use af_domain::{OrganizationId, OrganizationInvitationKey};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use zeroize::Zeroize as _;

use crate::credential_decryption::{CredentialKey, is_valid_key_id};

const MAX_INVITATION_EMAIL_BYTES: usize = 320;
const HMAC_BLOCK_BYTES: usize = 64;

/// 企业邀请邮箱与摘要密码学失败；错误不携带邮箱、令牌、密文或 AAD。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationInvitationCipherError {
    #[error("企业邀请加密材料未配置")]
    MissingKey,
    #[error("企业邀请加密材料无效")]
    InvalidKey,
    #[error("企业邀请邮箱无效")]
    InvalidEmail,
    #[error("企业邀请安全随机数不可用")]
    Entropy,
    #[error("企业邀请邮箱加密失败")]
    Encrypt,
    #[error("企业邀请邮箱密文封套无效")]
    InvalidEnvelope,
    #[error("企业邀请邮箱密钥标识不匹配")]
    KeyMismatch,
    #[error("企业邀请邮箱解密失败")]
    Decrypt,
}

/// 已规范化的企业邀请邮箱；默认脱敏并在释放时清零。
pub struct PlainOrganizationInvitationEmail(String);

impl PlainOrganizationInvitationEmail {
    /// 校验调用方已经完成小写、去空白和基本邮箱规范化。
    pub fn new(mut value: String) -> Result<Self, OrganizationInvitationCipherError> {
        if !valid_email(&value) {
            value.zeroize();
            return Err(OrganizationInvitationCipherError::InvalidEmail);
        }
        Ok(Self(value))
    }

    /// 仅向最终投递与仓储校验边界暴露规范化邮箱。
    #[must_use]
    pub fn expose_email(&self) -> &str {
        &self.0
    }
}

impl Drop for PlainOrganizationInvitationEmail {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for PlainOrganizationInvitationEmail {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlainOrganizationInvitationEmail(<redacted>)")
    }
}

/// 已解密企业邀请邮箱；只允许接受校验或受控投递读取。
pub struct DecryptedOrganizationInvitationEmail(String);

impl DecryptedOrganizationInvitationEmail {
    /// 暴露规范化邮箱给最终匹配边界。
    #[must_use]
    pub fn expose_email(&self) -> &str {
        &self.0
    }
}

impl Drop for DecryptedOrganizationInvitationEmail {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for DecryptedOrganizationInvitationEmail {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DecryptedOrganizationInvitationEmail(<redacted>)")
    }
}

/// 使用启动期凭据主密钥并通过独立 AAD 域隔离企业邀请材料。
#[derive(Clone)]
pub struct OrganizationInvitationCipher {
    key_id: String,
    key: CredentialKey,
}

impl OrganizationInvitationCipher {
    /// 从已校验启动配置构造企业邀请密码学边界。
    pub fn new(
        settings: &CredentialEncryptionSettings,
    ) -> Result<Self, OrganizationInvitationCipherError> {
        let key_id = settings
            .key_id()
            .ok_or(OrganizationInvitationCipherError::MissingKey)?;
        let encoded_key = settings
            .key()
            .ok_or(OrganizationInvitationCipherError::MissingKey)?;
        if !is_valid_key_id(key_id) {
            return Err(OrganizationInvitationCipherError::InvalidKey);
        }
        let key = CredentialKey::from_encoded(encoded_key.expose())
            .map_err(|_| OrganizationInvitationCipherError::InvalidKey)?;
        Ok(Self {
            key_id: key_id.to_owned(),
            key,
        })
    }

    /// 生成企业域隔离的邮箱指纹，用于有效邀请去重和接受匹配。
    #[must_use]
    pub fn email_fingerprint(
        &self,
        organization_id: OrganizationId,
        email: &PlainOrganizationInvitationEmail,
    ) -> [u8; 32] {
        keyed_digest(
            self.key.as_slice(),
            &[
                b"anyflows:organization-invitation:v1:email-fingerprint",
                &organization_id.get().to_be_bytes(),
                email.expose_email().as_bytes(),
            ],
        )
    }

    /// 生成用途域、企业域和邀请标识共同绑定的令牌摘要。
    #[must_use]
    pub fn token_digest(
        &self,
        organization_id: OrganizationId,
        invitation_key: OrganizationInvitationKey,
        token_secret: &[u8; 32],
    ) -> [u8; 32] {
        let invitation_key = invitation_key.persistence_key();
        keyed_digest(
            self.key.as_slice(),
            &[
                b"anyflows:organization-invitation:v1:token-digest",
                &organization_id.get().to_be_bytes(),
                invitation_key.as_bytes(),
                token_secret,
            ],
        )
    }

    /// 使用企业和邀请标识绑定的 AAD 加密邮箱。
    pub fn encrypt_email(
        &self,
        organization_id: OrganizationId,
        invitation_key: OrganizationInvitationKey,
        email: &PlainOrganizationInvitationEmail,
    ) -> Result<EncryptedCredentialEnvelope, OrganizationInvitationCipherError> {
        let mut nonce = [0_u8; CREDENTIAL_ENVELOPE_NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| OrganizationInvitationCipherError::Entropy)?;
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| OrganizationInvitationCipherError::InvalidKey)?;
        let aad = invitation_email_aad(organization_id, invitation_key);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: email.expose_email().as_bytes(),
                    aad: &aad,
                },
            )
            .map_err(|_| OrganizationInvitationCipherError::Encrypt)?;
        EncryptedCredentialEnvelope::new(self.key_id.clone(), nonce, ciphertext)
            .map_err(|_| OrganizationInvitationCipherError::InvalidEnvelope)
    }

    /// 验证密钥标识和位置绑定 AAD 后解密邀请邮箱。
    pub fn decrypt_email(
        &self,
        organization_id: OrganizationId,
        invitation_key: OrganizationInvitationKey,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DecryptedOrganizationInvitationEmail, OrganizationInvitationCipherError> {
        if envelope.key_id() != self.key_id {
            return Err(OrganizationInvitationCipherError::KeyMismatch);
        }
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| OrganizationInvitationCipherError::InvalidKey)?;
        let aad = invitation_email_aad(organization_id, invitation_key);
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(envelope.nonce()),
                Payload {
                    msg: envelope.ciphertext(),
                    aad: &aad,
                },
            )
            .map_err(|_| OrganizationInvitationCipherError::Decrypt)?;
        let value = String::from_utf8(plaintext).map_err(|error| {
            let mut bytes = error.into_bytes();
            bytes.zeroize();
            OrganizationInvitationCipherError::InvalidEmail
        })?;
        if !valid_email(&value) {
            let mut value = value;
            value.zeroize();
            return Err(OrganizationInvitationCipherError::InvalidEmail);
        }
        Ok(DecryptedOrganizationInvitationEmail(value))
    }
}

impl fmt::Debug for OrganizationInvitationCipher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationInvitationCipher")
            .field("key_id", &"<redacted>")
            .finish_non_exhaustive()
    }
}

fn invitation_email_aad(
    organization_id: OrganizationId,
    invitation_key: OrganizationInvitationKey,
) -> Vec<u8> {
    format!(
        "anyflows:organization-invitation:v1:organization={}:invitation={}:field=email",
        organization_id.get(),
        invitation_key.persistence_key()
    )
    .into_bytes()
}

fn keyed_digest(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    debug_assert!(key.len() <= HMAC_BLOCK_BYTES);
    let mut inner_pad = [0x36_u8; HMAC_BLOCK_BYTES];
    let mut outer_pad = [0x5c_u8; HMAC_BLOCK_BYTES];
    for (index, byte) in key.iter().copied().enumerate() {
        inner_pad[index] ^= byte;
        outer_pad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    for part in parts {
        inner.update(part);
    }
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner);
    outer.finalize().into()
}

fn valid_email(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_INVITATION_EMAIL_BYTES
        || value.trim() != value
        || value.bytes().any(|byte| byte.is_ascii_control())
        || value.bytes().any(|byte| byte.is_ascii_whitespace())
        || value.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return false;
    }
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && parts.next().is_none()
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

#[cfg(test)]
mod tests {
    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use serde_json::json;

    use super::*;

    #[test]
    fn invitation_material_is_bound_to_organization_and_invitation() {
        let cipher = OrganizationInvitationCipher::new(&settings()).unwrap();
        let organization = OrganizationId::new(7).unwrap();
        let other_organization = OrganizationId::new(8).unwrap();
        let invitation = OrganizationInvitationKey::new([0x11; 16]).unwrap();
        let other_invitation = OrganizationInvitationKey::new([0x22; 16]).unwrap();
        let email = PlainOrganizationInvitationEmail::new("member@example.com".to_owned()).unwrap();
        let envelope = cipher
            .encrypt_email(organization, invitation, &email)
            .unwrap();

        assert_eq!(
            cipher
                .decrypt_email(organization, invitation, &envelope)
                .unwrap()
                .expose_email(),
            "member@example.com"
        );
        assert!(
            cipher
                .decrypt_email(other_organization, invitation, &envelope)
                .is_err()
        );
        assert!(
            cipher
                .decrypt_email(organization, other_invitation, &envelope)
                .is_err()
        );
        assert_ne!(
            cipher.email_fingerprint(organization, &email),
            cipher.email_fingerprint(other_organization, &email)
        );
        assert_ne!(
            cipher.token_digest(organization, invitation, &[0x33; 32]),
            cipher.token_digest(organization, other_invitation, &[0x33; 32])
        );
        let rendered = format!("{cipher:?}{email:?}{envelope:?}");
        assert!(!rendered.contains("member@example.com"));
    }

    #[test]
    fn rejects_non_normalized_email() {
        for value in [
            "Member@example.com",
            "member@example.com ",
            "member@example",
            "member@@example.com",
        ] {
            assert!(PlainOrganizationInvitationEmail::new(value.to_owned()).is_err());
        }
    }

    fn settings() -> CredentialEncryptionSettings {
        serde_json::from_value(json!({
            "key_id": "primary-key",
            "key": URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES])
        }))
        .unwrap()
    }
}
