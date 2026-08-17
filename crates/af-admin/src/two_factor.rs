use std::fmt;

use af_account::{PlainSystemSecret, SystemSecretCipher, SystemSecretKind};
use af_db::EncryptedCredentialEnvelope;
use af_domain::UserId;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use getrandom::fill;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use thiserror::Error;

const TOTP_SECRET_BYTES: usize = 20;
const TOTP_STEP_SECONDS: u64 = 30;
const TOTP_WINDOW_STEPS: i64 = 1;
const TOTP_DIGITS: usize = 6;
const BACKUP_CODE_COUNT: usize = 10;
const BACKUP_CODE_BYTES: usize = 8;
const MAX_BACKUP_CODE_BYTES: usize = 128;
const CONFIG_VERSION: u8 = 1;

/// TOTP 配置的内部失败分类；错误不携带 secret、备份码或密文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum TwoFactorError {
    #[error("二次验证随机数不可用")]
    Entropy,
    #[error("二次验证配置无效")]
    InvalidConfiguration,
    #[error("二次验证密文处理失败")]
    Secret,
    #[error("二次验证码无效")]
    InvalidCode,
}

/// 启用 TOTP 后仅在本次响应中返回的入网材料。
pub struct TwoFactorEnrollment {
    secret: String,
    otpauth_uri: String,
    backup_codes: Vec<String>,
}

impl TwoFactorEnrollment {
    /// 返回需要手动录入或导入认证器的 Base32 secret。
    #[must_use]
    pub fn secret(&self) -> &str {
        &self.secret
    }

    /// 返回认证器可导入的 otpauth URI。
    #[must_use]
    pub fn otpauth_uri(&self) -> &str {
        &self.otpauth_uri
    }

    /// 返回仅此一次展示的备份码集合。
    #[must_use]
    pub fn backup_codes(&self) -> &[String] {
        &self.backup_codes
    }
}

impl fmt::Debug for TwoFactorEnrollment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TwoFactorEnrollment(<redacted>)")
    }
}

/// 登录二次验证结果；备份码成功时携带已移除该码的新密文。
pub(crate) enum LoginFactorResult {
    Totp,
    Backup {
        replacement: EncryptedCredentialEnvelope,
    },
}

/// 生成加密 TOTP 配置和一次性入网材料。
pub(crate) fn prepare_enrollment(
    cipher: &SystemSecretCipher,
    user_id: UserId,
) -> Result<(EncryptedCredentialEnvelope, TwoFactorEnrollment), TwoFactorError> {
    let mut secret_bytes = [0_u8; TOTP_SECRET_BYTES];
    fill(&mut secret_bytes).map_err(|_| TwoFactorError::Entropy)?;
    let secret = base32_encode(&secret_bytes);
    let mut backup_codes = Vec::with_capacity(BACKUP_CODE_COUNT);
    let mut backup_code_hashes = Vec::with_capacity(BACKUP_CODE_COUNT);
    for _ in 0..BACKUP_CODE_COUNT {
        let mut bytes = [0_u8; BACKUP_CODE_BYTES];
        fill(&mut bytes).map_err(|_| TwoFactorError::Entropy)?;
        let code = base32_encode(&bytes);
        backup_code_hashes.push(hash_backup_code(&code));
        backup_codes.push(code);
    }
    let configuration = StoredConfiguration {
        version: CONFIG_VERSION,
        secret: secret.clone(),
        backup_code_hashes,
    };
    let envelope = encrypt_configuration(cipher, user_id, &configuration)?;
    let otpauth_uri = format!(
        "otpauth://totp/AnyFlows:user-{}?secret={secret}&issuer=AnyFlows&digits=6&period=30",
        user_id.get()
    );
    Ok((
        envelope,
        TwoFactorEnrollment {
            secret,
            otpauth_uri,
            backup_codes,
        },
    ))
}

/// 验证 TOTP 或备份码；备份码成功后立即生成不可重放的新封套。
pub(crate) fn verify_login_code(
    cipher: &SystemSecretCipher,
    user_id: UserId,
    envelope: &EncryptedCredentialEnvelope,
    code: &str,
    now_unix_seconds: u64,
) -> Result<LoginFactorResult, TwoFactorError> {
    let configuration = decrypt_configuration(cipher, user_id, envelope)?;
    if is_totp_code(code) && verify_totp(&configuration.secret, code, now_unix_seconds) {
        return Ok(LoginFactorResult::Totp);
    }
    let normalized = normalize_backup_code(code).ok_or(TwoFactorError::InvalidCode)?;
    let digest = hash_backup_code(&normalized);
    let Some(index) = configuration
        .backup_code_hashes
        .iter()
        .position(|candidate| constant_time_equal(candidate.as_bytes(), digest.as_bytes()))
    else {
        return Err(TwoFactorError::InvalidCode);
    };
    let mut replacement = configuration;
    replacement.backup_code_hashes.remove(index);
    let replacement = encrypt_configuration(cipher, user_id, &replacement)?;
    Ok(LoginFactorResult::Backup { replacement })
}

fn encrypt_configuration(
    cipher: &SystemSecretCipher,
    user_id: UserId,
    configuration: &StoredConfiguration,
) -> Result<EncryptedCredentialEnvelope, TwoFactorError> {
    let serialized =
        serde_json::to_string(configuration).map_err(|_| TwoFactorError::InvalidConfiguration)?;
    let plaintext = PlainSystemSecret::new(serialized).map_err(|_| TwoFactorError::Secret)?;
    cipher
        .encrypt(SystemSecretKind::TotpSecret(user_id), &plaintext)
        .map_err(|_| TwoFactorError::Secret)
}

fn decrypt_configuration(
    cipher: &SystemSecretCipher,
    user_id: UserId,
    envelope: &EncryptedCredentialEnvelope,
) -> Result<StoredConfiguration, TwoFactorError> {
    let plaintext = cipher
        .decrypt(SystemSecretKind::TotpSecret(user_id), envelope)
        .map_err(|_| TwoFactorError::Secret)?;
    let configuration: StoredConfiguration = serde_json::from_str(plaintext.expose_secret())
        .map_err(|_| TwoFactorError::InvalidConfiguration)?;
    validate_configuration(&configuration)
}

fn validate_configuration(
    configuration: &StoredConfiguration,
) -> Result<StoredConfiguration, TwoFactorError> {
    if configuration.version != CONFIG_VERSION
        || base32_decode(&configuration.secret)
            .map_or(true, |decoded| decoded.len() != TOTP_SECRET_BYTES)
        || configuration.backup_code_hashes.len() > BACKUP_CODE_COUNT
        || configuration.backup_code_hashes.iter().any(|hash| {
            hash.len() != 43
                || hash
                    .bytes()
                    .any(|byte| !byte.is_ascii_alphanumeric() && byte != b'_' && byte != b'-')
        })
    {
        return Err(TwoFactorError::InvalidConfiguration);
    }
    Ok(configuration.clone())
}

fn verify_totp(secret: &str, code: &str, now_unix_seconds: u64) -> bool {
    let Ok(secret) = base32_decode(secret) else {
        return false;
    };
    let current_step = now_unix_seconds / TOTP_STEP_SECONDS;
    (-TOTP_WINDOW_STEPS..=TOTP_WINDOW_STEPS).any(|offset| {
        let step = if offset.is_negative() {
            current_step.checked_sub(offset.unsigned_abs())
        } else {
            current_step.checked_add(offset as u64)
        };
        step.is_some_and(|step| {
            constant_time_equal(format_totp(&secret, step).as_bytes(), code.as_bytes())
        })
    })
}

fn format_totp(secret: &[u8], counter: u64) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC-SHA1 接受任意非空 secret");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[19] & 0x0f);
    let binary = (u32::from(digest[offset]) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    format!("{:06}", (binary & 0x7fff_ffff) % 1_000_000)
}

fn is_totp_code(code: &str) -> bool {
    code.len() == TOTP_DIGITS && code.bytes().all(|byte| byte.is_ascii_digit())
}

fn normalize_backup_code(code: &str) -> Option<String> {
    if code.len() > MAX_BACKUP_CODE_BYTES {
        return None;
    }
    let normalized: String = code
        .bytes()
        .filter(|byte| *byte != b'-' && !byte.is_ascii_whitespace())
        .map(|byte| byte.to_ascii_uppercase() as char)
        .collect();
    if !(8..=32).contains(&normalized.len())
        || !normalized
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return None;
    }
    Some(normalized)
}

fn hash_backup_code(code: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(code.as_bytes()))
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let difference = left
        .iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        });
    difference == 0
}

fn base32_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut output = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer = 0_u16;
    let mut bits = 0_u8;
    for byte in bytes {
        buffer = (buffer << 8) | u16::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            output.push(ALPHABET[usize::from((buffer >> bits) & 0x1f)] as char);
        }
    }
    if bits > 0 {
        output.push(ALPHABET[usize::from((buffer << (5 - bits)) & 0x1f)] as char);
    }
    output
}

fn base32_decode(value: &str) -> Result<Vec<u8>, TwoFactorError> {
    if value.is_empty() || value.len() > 128 {
        return Err(TwoFactorError::InvalidConfiguration);
    }
    let mut output = Vec::with_capacity(value.len() * 5 / 8);
    let mut buffer = 0_u32;
    let mut bits = 0_u8;
    for byte in value.bytes() {
        let byte = byte.to_ascii_uppercase();
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => return Err(TwoFactorError::InvalidConfiguration),
        };
        buffer = (buffer << 5) | u32::from(digit);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(output)
}

#[derive(Clone, Serialize, Deserialize)]
struct StoredConfiguration {
    version: u8,
    secret: String,
    backup_code_hashes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totp_matches_rfc6238_vectors_and_allows_one_step_skew() {
        let secret = base32_encode(b"12345678901234567890");
        assert_eq!(
            format_totp(&base32_decode(&secret).unwrap(), 1_111_111_111 / 30),
            "050471"
        );
        assert!(verify_totp(&secret, "050471", 1_111_111_111));
        assert!(verify_totp(&secret, "050471", 1_111_111_111 + 30));
        assert!(!verify_totp(&secret, "050471", 1_111_111_111 + 60));
    }

    #[test]
    fn backup_code_hash_is_normalized_and_constant_time_checked() {
        let digest = hash_backup_code("ABCDEFGH");
        assert!(!constant_time_equal(
            digest.as_bytes(),
            hash_backup_code("abcdefgh").as_bytes()
        ));
        assert_eq!(
            normalize_backup_code("abcd-efgh"),
            Some("ABCDEFGH".to_owned())
        );
    }

    #[test]
    fn base32_round_trip_has_no_padding() {
        let value = [0_u8, 1, 2, 3, 254, 255];
        let encoded = base32_encode(&value);
        assert!(!encoded.contains('='));
        assert_eq!(base32_decode(&encoded).unwrap(), value);
    }
}
