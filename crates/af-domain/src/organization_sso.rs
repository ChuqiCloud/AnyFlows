use std::{fmt, net::IpAddr};

use thiserror::Error;

const ORGANIZATION_SSO_PROVIDER_KEY_BYTES: usize = 16;
const ORGANIZATION_SSO_PROVIDER_KEY_HEX_BYTES: usize = ORGANIZATION_SSO_PROVIDER_KEY_BYTES * 2;
const MAX_DNS_NAME_BYTES: usize = 253;
const MAX_DNS_LABEL_BYTES: usize = 63;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 企业 SSO Provider 公开标识构造错误；错误中不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoIdentifierError {
    /// 全零标识不能形成有效的 Provider 边界。
    #[error("企业 SSO Provider 公开标识不能全为零")]
    AllZero,
    /// 持久化值不是固定长度的小写十六进制文本。
    #[error("企业 SSO Provider 公开标识格式无效")]
    InvalidEncoding,
}

/// 不暴露数据库顺序的非零 128 位企业 SSO Provider 公开标识。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationSsoProviderKey([u8; ORGANIZATION_SSO_PROVIDER_KEY_BYTES]);

impl OrganizationSsoProviderKey {
    /// 校验并构造非零企业 SSO Provider 公开标识。
    pub const fn new(
        bytes: [u8; ORGANIZATION_SSO_PROVIDER_KEY_BYTES],
    ) -> Result<Self, OrganizationSsoIdentifierError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(OrganizationSsoIdentifierError::AllZero)
    }

    /// 从数据库和接口使用的 32 位小写十六进制文本恢复标识。
    pub fn from_persistence_key(value: &str) -> Result<Self, OrganizationSsoIdentifierError> {
        if value.len() != ORGANIZATION_SSO_PROVIDER_KEY_HEX_BYTES {
            return Err(OrganizationSsoIdentifierError::InvalidEncoding);
        }
        let mut bytes = [0_u8; ORGANIZATION_SSO_PROVIDER_KEY_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high =
                decode_hex(pair[0]).ok_or(OrganizationSsoIdentifierError::InvalidEncoding)?;
            let low = decode_hex(pair[1]).ok_or(OrganizationSsoIdentifierError::InvalidEncoding)?;
            bytes[index] = (high << 4) | low;
        }
        Self::new(bytes)
    }

    /// 返回数据库和接口共用的稳定持久化文本。
    #[must_use]
    pub fn persistence_key(self) -> String {
        let mut encoded = String::with_capacity(ORGANIZATION_SSO_PROVIDER_KEY_HEX_BYTES);
        for byte in self.0 {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        encoded
    }
}

impl TryFrom<[u8; ORGANIZATION_SSO_PROVIDER_KEY_BYTES]> for OrganizationSsoProviderKey {
    type Error = OrganizationSsoIdentifierError;

    fn try_from(bytes: [u8; ORGANIZATION_SSO_PROVIDER_KEY_BYTES]) -> Result<Self, Self::Error> {
        Self::new(bytes)
    }
}

impl fmt::Debug for OrganizationSsoProviderKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoProviderKey(<redacted>)")
    }
}

/// 企业 SSO 域名构造错误；错误中不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoDomainError {
    /// 域名为空或超过 DNS 文本长度上限。
    #[error("企业 SSO 域名长度无效")]
    InvalidLength,
    /// 域名不是已经规范化的小写 ASCII 文本。
    #[error("企业 SSO 域名不是规范的小写 ASCII 文本")]
    NonCanonical,
    /// IP 地址不能作为企业域名认领边界。
    #[error("企业 SSO 域名不能是 IP 地址")]
    IpLiteral,
    /// 单标签主机名不能作为可认领的企业域名。
    #[error("企业 SSO 域名必须包含至少两个标签")]
    SingleLabel,
    /// 标签为空、过长或包含不允许的字符和连字符位置。
    #[error("企业 SSO 域名标签无效")]
    InvalidLabel,
}

/// 已规范化的小写 ASCII 企业域名。
///
/// 该值对象只验证 DNS 语法边界；公共后缀和 DNS 所有权由后续受控验证服务判断。
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationSsoDomain(String);

impl OrganizationSsoDomain {
    /// 接受已经规范化的域名，不在服务端猜测大小写或 Unicode 映射。
    pub fn new(value: impl Into<String>) -> Result<Self, OrganizationSsoDomainError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_DNS_NAME_BYTES {
            return Err(OrganizationSsoDomainError::InvalidLength);
        }
        if !value.is_ascii() || value.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(OrganizationSsoDomainError::NonCanonical);
        }
        if value.parse::<IpAddr>().is_ok() {
            return Err(OrganizationSsoDomainError::IpLiteral);
        }

        let mut labels = value.split('.');
        let first = labels
            .next()
            .ok_or(OrganizationSsoDomainError::SingleLabel)?;
        let second = labels
            .next()
            .ok_or(OrganizationSsoDomainError::SingleLabel)?;
        validate_domain_label(first)?;
        validate_domain_label(second)?;
        for label in labels {
            validate_domain_label(label)?;
        }

        Ok(Self(value))
    }

    /// 返回后续持久化、DNS 查询和接口投影共用的规范域名。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for OrganizationSsoDomain {
    type Error = OrganizationSsoDomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for OrganizationSsoDomain {
    type Error = OrganizationSsoDomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl AsRef<str> for OrganizationSsoDomain {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// 企业 SSO 持久化状态码解析错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoStateCodeError {
    /// 协议不属于 OIDC/SAML 闭合集合。
    #[error("企业 SSO 协议无效")]
    InvalidProtocol,
    /// Provider 状态不属于闭合集合。
    #[error("企业 SSO Provider 状态无效")]
    InvalidProviderStatus,
    /// 域名状态不属于闭合集合。
    #[error("企业 SSO 域名状态无效")]
    InvalidDomainStatus,
    /// 强制策略不属于闭合集合。
    #[error("企业 SSO 强制策略无效")]
    InvalidEnforcementPolicy,
}

/// 企业 SSO Provider 使用的闭合协议集合。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationSsoProtocol {
    /// OpenID Connect Authorization Code 流程。
    Oidc = 1,
    /// SAML 2.0 SP 发起流程。
    Saml = 2,
}

/// 企业 SSO Provider 配置状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationSsoProviderStatus {
    /// 配置尚未完成或尚未通过受控验证。
    Draft = 1,
    /// 配置已验证且可参与后续登录事务。
    Active = 2,
    /// 配置已停用，仅保留历史和审计事实。
    Disabled = 3,
}

/// 企业 SSO 域名所有权状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationSsoDomainStatus {
    /// 等待 DNS TXT 证明。
    Pending = 1,
    /// 服务端已验证 DNS 所有权。
    Verified = 2,
    /// 域名认领已撤销，仅保留历史事实。
    Revoked = 3,
}

/// 企业成员登录使用的 SSO 强制策略。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationSsoEnforcementPolicy {
    /// 企业成员仍可使用站点允许的其他登录方式。
    Optional = 1,
    /// 企业成员必须使用 SSO，启用条件由后续服务端策略原子校验。
    Required = 2,
}

macro_rules! sso_state_code {
    ($type:ty, $error:expr, {$($code:literal => $variant:expr),+ $(,)?}) => {
        impl $type {
            /// 返回持久化使用的稳定数值。
            #[must_use]
            pub const fn code(self) -> i16 {
                self as i16
            }
        }

        impl TryFrom<i16> for $type {
            type Error = OrganizationSsoStateCodeError;

            fn try_from(value: i16) -> Result<Self, Self::Error> {
                match value {
                    $($code => Ok($variant),)+
                    _ => Err($error),
                }
            }
        }
    };
}

sso_state_code!(OrganizationSsoProtocol, OrganizationSsoStateCodeError::InvalidProtocol, {
    1 => OrganizationSsoProtocol::Oidc,
    2 => OrganizationSsoProtocol::Saml,
});
sso_state_code!(
    OrganizationSsoProviderStatus,
    OrganizationSsoStateCodeError::InvalidProviderStatus,
    {
        1 => OrganizationSsoProviderStatus::Draft,
        2 => OrganizationSsoProviderStatus::Active,
        3 => OrganizationSsoProviderStatus::Disabled,
    }
);
sso_state_code!(
    OrganizationSsoDomainStatus,
    OrganizationSsoStateCodeError::InvalidDomainStatus,
    {
        1 => OrganizationSsoDomainStatus::Pending,
        2 => OrganizationSsoDomainStatus::Verified,
        3 => OrganizationSsoDomainStatus::Revoked,
    }
);
sso_state_code!(
    OrganizationSsoEnforcementPolicy,
    OrganizationSsoStateCodeError::InvalidEnforcementPolicy,
    {
        1 => OrganizationSsoEnforcementPolicy::Optional,
        2 => OrganizationSsoEnforcementPolicy::Required,
    }
);

fn validate_domain_label(label: &str) -> Result<(), OrganizationSsoDomainError> {
    if label.is_empty()
        || label.len() > MAX_DNS_LABEL_BYTES
        || label.starts_with('-')
        || label.ends_with('-')
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(OrganizationSsoDomainError::InvalidLabel);
    }
    Ok(())
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
