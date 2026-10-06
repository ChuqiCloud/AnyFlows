use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroizing;

const API_KEY_PREFIX: &str = "sk-af-";
const API_KEY_ENTROPY_BYTES: usize = 32;
const API_KEY_RANDOM_TEXT_LENGTH: usize = 43;
const API_KEY_TEXT_LENGTH: usize = API_KEY_PREFIX.len() + API_KEY_RANDOM_TEXT_LENGTH;
const API_KEY_DISPLAY_RANDOM_LENGTH: usize = 12;

/// 客户端提交的规范 AnyFlows API Key。
///
/// 该类型拥有并在释放时清零自己的明文副本，不实现 `Clone`、`Display` 或序列化；
/// 调用方只能通过显式方法在鉴权或首次展示边界读取明文。
pub struct PresentedApiKey(Zeroizing<String>);

impl PresentedApiKey {
    /// 严格解析 `sk-af-` 加 32 字节 Base64URL 无填充随机段的规范格式。
    pub fn parse(value: &str) -> Result<Self, ApiKeyParseError> {
        if value.len() != API_KEY_TEXT_LENGTH || !value.starts_with(API_KEY_PREFIX) {
            return Err(ApiKeyParseError::InvalidFormat);
        }
        let random_text = &value[API_KEY_PREFIX.len()..];
        let mut decoded = Zeroizing::new([0_u8; API_KEY_ENTROPY_BYTES]);
        let decoded_length = URL_SAFE_NO_PAD
            .decode_slice(random_text, &mut *decoded)
            .map_err(|_| ApiKeyParseError::InvalidFormat)?;
        if decoded_length != API_KEY_ENTROPY_BYTES {
            return Err(ApiKeyParseError::InvalidFormat);
        }
        Ok(Self(Zeroizing::new(value.to_owned())))
    }

    /// 显式借用 API Key 明文；禁止写入日志、错误、缓存键或持久化实体。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }

    /// 对完整规范明文计算可索引的 SHA-256 哈希。
    #[must_use]
    pub fn digest(&self) -> ApiKeyDigest {
        let digest = Sha256::digest(self.0.as_bytes());
        ApiKeyDigest(lower_hex(&digest))
    }

    /// 生成只用于管理界面识别、不得用于鉴权查询的明文前缀。
    #[must_use]
    pub fn display_prefix(&self) -> ApiKeyPrefix {
        let end = API_KEY_PREFIX.len() + API_KEY_DISPLAY_RANDOM_LENGTH;
        ApiKeyPrefix(self.0[..end].to_owned())
    }
}

impl fmt::Debug for PresentedApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 规范 API Key 的持久化 SHA-256 十六进制哈希。
#[derive(Clone, Eq, PartialEq)]
pub struct ApiKeyDigest(String);

impl ApiKeyDigest {
    /// 返回与 `tokens.key_hash CHAR(64)` 对齐的小写十六进制文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKeyDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 管理界面可展示的 API Key 明文前缀。
#[derive(Clone, Eq, PartialEq)]
pub struct ApiKeyPrefix(String);

impl ApiKeyPrefix {
    /// 返回固定 `sk-af-` 加前 12 个随机字符的展示文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKeyPrefix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 一次性签发结果；数据库只保存哈希和展示前缀，明文只应返回一次。
pub struct IssuedApiKey {
    key: PresentedApiKey,
    digest: ApiKeyDigest,
    display_prefix: ApiKeyPrefix,
}

impl IssuedApiKey {
    /// 使用操作系统 CSPRNG 生成 256 位随机 API Key。
    pub fn generate() -> Result<Self, ApiKeyGenerationError> {
        let mut entropy = Zeroizing::new([0_u8; API_KEY_ENTROPY_BYTES]);
        getrandom::fill(&mut *entropy).map_err(|_| ApiKeyGenerationError::EntropyUnavailable)?;
        Ok(Self::from_entropy(&entropy))
    }

    /// 返回只应在签发响应中展示一次的明文。
    #[must_use]
    pub const fn key(&self) -> &PresentedApiKey {
        &self.key
    }

    /// 返回应写入 `tokens.key_hash` 的哈希。
    #[must_use]
    pub const fn digest(&self) -> &ApiKeyDigest {
        &self.digest
    }

    /// 返回应写入 `tokens.key_prefix` 的展示前缀。
    #[must_use]
    pub const fn display_prefix(&self) -> &ApiKeyPrefix {
        &self.display_prefix
    }

    fn from_entropy(entropy: &[u8; API_KEY_ENTROPY_BYTES]) -> Self {
        let mut text = Zeroizing::new(String::with_capacity(API_KEY_TEXT_LENGTH));
        text.push_str(API_KEY_PREFIX);
        URL_SAFE_NO_PAD.encode_string(entropy, &mut text);
        let key = PresentedApiKey(text);
        let digest = key.digest();
        let display_prefix = key.display_prefix();
        Self {
            key,
            digest,
            display_prefix,
        }
    }
}

impl fmt::Debug for IssuedApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 客户端 API Key 不符合规范格式。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ApiKeyParseError {
    /// 输入不是唯一支持的规范格式。
    #[error("API Key 格式无效")]
    InvalidFormat,
}

/// API Key 签发失败。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ApiKeyGenerationError {
    /// 操作系统无法提供安全随机数；调用方必须拒绝签发。
    #[error("无法获取安全随机数")]
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
pub(super) fn issued_from_test_entropy(entropy: [u8; API_KEY_ENTROPY_BYTES]) -> IssuedApiKey {
    IssuedApiKey::from_entropy(&entropy)
}
