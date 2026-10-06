use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const SHARE_TOKEN_PREFIX: &str = "sh-af-";
const SHARE_TOKEN_ENTROPY_BYTES: usize = 32;
const SHARE_TOKEN_RANDOM_TEXT_LENGTH: usize = 43;
const SHARE_TOKEN_TEXT_LENGTH: usize = SHARE_TOKEN_PREFIX.len() + SHARE_TOKEN_RANDOM_TEXT_LENGTH;

/// 只在创建响应或公开读取边界短暂持有的分享令牌。
pub struct PresentedPlaygroundShareToken(Zeroizing<String>);

impl PresentedPlaygroundShareToken {
    /// 消费并严格校验规范分享令牌，释放时清零其明文缓冲区。
    pub fn parse_owned(value: String) -> Result<Self, PlaygroundShareTokenError> {
        if value.len() != SHARE_TOKEN_TEXT_LENGTH || !value.starts_with(SHARE_TOKEN_PREFIX) {
            return Err(PlaygroundShareTokenError::InvalidFormat);
        }
        let random_text = &value[SHARE_TOKEN_PREFIX.len()..];
        let mut decoded = Zeroizing::new([0_u8; SHARE_TOKEN_ENTROPY_BYTES]);
        let decoded_length = URL_SAFE_NO_PAD
            .decode_slice(random_text, &mut *decoded)
            .map_err(|_| PlaygroundShareTokenError::InvalidFormat)?;
        if decoded_length != SHARE_TOKEN_ENTROPY_BYTES {
            return Err(PlaygroundShareTokenError::InvalidFormat);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    /// 返回只允许进入一次性响应或摘要计算的令牌明文。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }

    /// 计算与数据库 CHAR(64) 对齐的小写 SHA-256 摘要。
    #[must_use]
    pub fn digest(&self) -> PlaygroundShareTokenDigest {
        PlaygroundShareTokenDigest(lower_hex(&Sha256::digest(self.0.as_bytes())))
    }
}

impl fmt::Debug for PresentedPlaygroundShareToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

impl Serialize for PresentedPlaygroundShareToken {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.expose_secret())
    }
}

/// 只供仓储定位使用且禁止调试输出的分享令牌摘要。
pub struct PlaygroundShareTokenDigest(String);

impl PlaygroundShareTokenDigest {
    /// 返回规范小写十六进制摘要。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PlaygroundShareTokenDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 使用操作系统密码学随机源生成的一次性分享令牌。
pub struct IssuedPlaygroundShareToken {
    token: PresentedPlaygroundShareToken,
    digest: PlaygroundShareTokenDigest,
}

impl IssuedPlaygroundShareToken {
    /// 生成 256 位随机分享令牌及其持久化摘要。
    pub fn generate() -> Result<Self, PlaygroundShareTokenError> {
        let mut entropy = Zeroizing::new([0_u8; SHARE_TOKEN_ENTROPY_BYTES]);
        getrandom::fill(&mut *entropy)
            .map_err(|_| PlaygroundShareTokenError::EntropyUnavailable)?;
        Ok(Self::from_entropy(&entropy))
    }

    /// 返回只应在创建响应中展示一次的令牌。
    #[must_use]
    pub const fn token(&self) -> &PresentedPlaygroundShareToken {
        &self.token
    }

    /// 返回只应写入数据库的摘要。
    #[must_use]
    pub const fn digest(&self) -> &PlaygroundShareTokenDigest {
        &self.digest
    }

    /// 消费签发结果并把令牌所有权交给响应对象。
    #[must_use]
    pub fn into_token(self) -> PresentedPlaygroundShareToken {
        self.token
    }

    fn from_entropy(entropy: &[u8; SHARE_TOKEN_ENTROPY_BYTES]) -> Self {
        let mut text = Zeroizing::new(String::with_capacity(SHARE_TOKEN_TEXT_LENGTH));
        text.push_str(SHARE_TOKEN_PREFIX);
        URL_SAFE_NO_PAD.encode_string(entropy, &mut text);
        let token = PresentedPlaygroundShareToken(text);
        let digest = token.digest();
        Self { token, digest }
    }
}

impl fmt::Debug for IssuedPlaygroundShareToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 分享令牌生成或解析错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaygroundShareTokenError {
    /// 明文不是唯一支持的规范格式。
    InvalidFormat,
    /// 操作系统无法提供安全随机数。
    EntropyUnavailable,
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
pub(super) fn issued_from_test_entropy(
    entropy: [u8; SHARE_TOKEN_ENTROPY_BYTES],
) -> IssuedPlaygroundShareToken {
    IssuedPlaygroundShareToken::from_entropy(&entropy)
}
