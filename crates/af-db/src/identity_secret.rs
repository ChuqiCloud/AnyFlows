use argon2::{
    Argon2,
    password_hash::{PasswordHasher as _, SaltString},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use zeroize::Zeroizing;

use crate::entity::PasswordHash;

const AFF_CODE_ENTROPY_BYTES: usize = 16;

/// 身份密钥材料生成失败的闭合分类，不携带密码或随机内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IdentitySecretError {
    /// 系统随机源不可用。
    Entropy,
    /// Argon2id 输出违反持久化格式约束。
    InvalidHash,
}

/// 使用独立随机盐生成规范化 Argon2id 密码哈希。
pub(crate) fn hash_password(
    password: &Zeroizing<String>,
) -> Result<PasswordHash, IdentitySecretError> {
    let mut salt_bytes = [0_u8; 16];
    getrandom::fill(&mut salt_bytes).map_err(|_| IdentitySecretError::Entropy)?;
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|_| IdentitySecretError::InvalidHash)?;
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| IdentitySecretError::InvalidHash)?
        .to_string();
    PasswordHash::parse(&hash).map_err(|_| IdentitySecretError::InvalidHash)
}

/// 生成不含用户信息的随机邀请码，避免可枚举身份数据进入该字段。
pub(crate) fn generate_aff_code() -> Result<String, IdentitySecretError> {
    let mut entropy = [0_u8; AFF_CODE_ENTROPY_BYTES];
    getrandom::fill(&mut entropy).map_err(|_| IdentitySecretError::Entropy)?;
    Ok(format!("af-{}", URL_SAFE_NO_PAD.encode(entropy)))
}
