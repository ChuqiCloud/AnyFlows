use std::fmt;

use getrandom::fill;
use sha2::{Digest, Sha256};
use thiserror::Error;

const SCIM_TOKEN_ENTROPY_BYTES: usize = 32;
const SCIM_TOKEN_PREFIX: &str = "scim_";
const SCIM_TOKEN_HEX_BYTES: usize = SCIM_TOKEN_ENTROPY_BYTES * 2;
const SCIM_TOKEN_TOTAL_BYTES: usize = SCIM_TOKEN_PREFIX.len() + SCIM_TOKEN_HEX_BYTES;
const SCIM_TOKEN_DIGEST_CONTEXT: &[u8] = b"anyflows-enterprise-scim-token-v1";
const SCIM_TOKEN_KEY_BYTES: usize = 16;
const SCIM_TOKEN_KEY_HEX_BYTES: usize = SCIM_TOKEN_KEY_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 企业 SCIM 令牌构造错误；错误中不携带令牌或摘要原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimTokenError {
    /// 令牌不是固定前缀加 256 位小写十六进制随机数。
    #[error("企业 SCIM 令牌格式无效")]
    InvalidEncoding,
    /// 操作系统无法提供安全随机数。
    #[error("企业 SCIM 令牌随机数不可用")]
    EntropyUnavailable,
}

/// 仅在内存中保留的高熵 SCIM 令牌；持久化层只能保存摘要。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationScimToken(String);

impl OrganizationScimToken {
    /// 生成带固定前缀的 256 位随机令牌。
    pub fn generate() -> Result<Self, OrganizationScimTokenError> {
        let mut bytes = [0_u8; SCIM_TOKEN_ENTROPY_BYTES];
        fill(&mut bytes).map_err(|_| OrganizationScimTokenError::EntropyUnavailable)?;
        let mut value = String::with_capacity(SCIM_TOKEN_TOTAL_BYTES);
        value.push_str(SCIM_TOKEN_PREFIX);
        for byte in bytes {
            value.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            value.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        Self::new(value)
    }

    /// 从受控输入恢复令牌；调用方不得将返回值写入数据库或日志。
    pub fn new(value: impl Into<String>) -> Result<Self, OrganizationScimTokenError> {
        let value = value.into();
        let Some(entropy) = value.strip_prefix(SCIM_TOKEN_PREFIX) else {
            return Err(OrganizationScimTokenError::InvalidEncoding);
        };
        if value.len() != SCIM_TOKEN_TOTAL_BYTES
            || entropy.len() != SCIM_TOKEN_HEX_BYTES
            || !entropy
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || entropy.bytes().all(|byte| byte == b'0')
        {
            return Err(OrganizationScimTokenError::InvalidEncoding);
        }
        Ok(Self(value))
    }

    /// 返回仅用于一次创建响应或摘要计算的令牌原文。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 返回可展示的固定前缀，不包含随机材料。
    #[must_use]
    pub const fn prefix(&self) -> &'static str {
        SCIM_TOKEN_PREFIX
    }

    /// 计算带领域分隔的 SHA-256 摘要；数据库只保存该摘要文本。
    #[must_use]
    pub fn digest(&self) -> OrganizationScimTokenDigest {
        let mut hasher = Sha256::new();
        hasher.update(SCIM_TOKEN_DIGEST_CONTEXT);
        hasher.update([0]);
        hasher.update(self.0.as_bytes());
        OrganizationScimTokenDigest::from_bytes(hasher.finalize().into())
    }
}

impl fmt::Debug for OrganizationScimToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimToken(<redacted>)")
    }
}

/// SCIM 令牌的固定 256 位持久化摘要。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationScimTokenDigest(String);

impl OrganizationScimTokenDigest {
    fn from_bytes(bytes: [u8; 32]) -> Self {
        let mut value = String::with_capacity(64);
        for byte in bytes {
            value.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            value.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        Self(value)
    }

    /// 从数据库恢复规范摘要文本。
    pub fn from_persistence_key(
        value: impl Into<String>,
    ) -> Result<Self, OrganizationScimTokenError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(OrganizationScimTokenError::InvalidEncoding);
        }
        Ok(Self(value))
    }

    /// 返回数据库查询使用的摘要文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OrganizationScimTokenDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimTokenDigest(<redacted>)")
    }
}

/// 不暴露数据库自增序号的 SCIM 令牌公开标识。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationScimTokenKey(String);

impl OrganizationScimTokenKey {
    /// 生成非零随机令牌标识。
    pub fn generate() -> Result<Self, OrganizationScimTokenError> {
        let mut bytes = [0_u8; SCIM_TOKEN_KEY_BYTES];
        fill(&mut bytes).map_err(|_| OrganizationScimTokenError::EntropyUnavailable)?;
        let mut value = String::with_capacity(SCIM_TOKEN_KEY_HEX_BYTES);
        for byte in bytes {
            value.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            value.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        Self::new(value)
    }

    /// 从持久化文本恢复公开标识。
    pub fn new(value: impl Into<String>) -> Result<Self, OrganizationScimTokenError> {
        let value = value.into();
        if value.len() != SCIM_TOKEN_KEY_HEX_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || value.bytes().all(|byte| byte == b'0')
        {
            return Err(OrganizationScimTokenError::InvalidEncoding);
        }
        Ok(Self(value))
    }

    /// 返回接口与数据库共用的稳定文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OrganizationScimTokenKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimTokenKey(<redacted>)")
    }
}

/// SCIM 目录资源的固定种类，不允许扩展为任意路径。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationScimResourceKind {
    /// 企业成员资源。
    Users,
    /// 企业团队资源。
    Groups,
}

/// 令牌允许访问的企业目录资源集合。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OrganizationScimResourceSet(u8);

impl OrganizationScimResourceSet {
    const USERS: u8 = 0b01;
    const GROUPS: u8 = 0b10;

    /// 仅允许用户资源。
    #[must_use]
    pub const fn users() -> Self {
        Self(Self::USERS)
    }

    /// 仅允许组资源。
    #[must_use]
    pub const fn groups() -> Self {
        Self(Self::GROUPS)
    }

    /// 同时允许用户和组资源。
    #[must_use]
    pub const fn users_and_groups() -> Self {
        Self(Self::USERS | Self::GROUPS)
    }

    /// 从持久化位掩码恢复资源集合，拒绝未知位和空集合。
    pub const fn from_code(code: i16) -> Result<Self, OrganizationScimTokenError> {
        if code >= 1 && code <= 3 {
            Ok(Self(code as u8))
        } else {
            Err(OrganizationScimTokenError::InvalidEncoding)
        }
    }

    /// 返回数据库使用的稳定位掩码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self.0 as i16
    }

    /// 判断令牌是否允许访问指定目录资源。
    #[must_use]
    pub const fn contains(self, kind: OrganizationScimResourceKind) -> bool {
        let bit = match kind {
            OrganizationScimResourceKind::Users => Self::USERS,
            OrganizationScimResourceKind::Groups => Self::GROUPS,
        };
        self.0 & bit != 0
    }
}

/// 企业 SCIM 令牌生命周期状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationScimTokenStatus {
    /// 令牌可用于后续受控同步请求。
    Active = 1,
    /// 令牌已撤销且不可恢复。
    Revoked = 2,
}

impl OrganizationScimTokenStatus {
    /// 返回数据库使用的稳定状态码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 从数据库状态码恢复固定集合。
    pub const fn from_code(code: i16) -> Result<Self, OrganizationScimTokenError> {
        match code {
            1 => Ok(Self::Active),
            2 => Ok(Self::Revoked),
            _ => Err(OrganizationScimTokenError::InvalidEncoding),
        }
    }
}
