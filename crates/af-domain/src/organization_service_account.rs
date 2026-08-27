use std::fmt;

use getrandom::fill;
use thiserror::Error;

use crate::{OrganizationId, OrganizationServiceAccountId};

const SERVICE_ACCOUNT_KEY_BYTES: usize = 16;
const SERVICE_ACCOUNT_KEY_HEX_BYTES: usize = SERVICE_ACCOUNT_KEY_BYTES * 2;
const MAX_SERVICE_ACCOUNT_NAME_BYTES: usize = 128;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 企业服务账号公开标识的构造错误；错误信息不携带外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationServiceAccountIdentifierError {
    /// 标识不能全部为零，避免空值与真实主体混淆。
    #[error("企业服务账号公开标识不能全为零")]
    AllZero,
    /// 持久化值必须是固定长度的小写十六进制文本。
    #[error("企业服务账号公开标识格式无效")]
    InvalidEncoding,
    /// 操作系统没有提供安全随机源。
    #[error("企业服务账号公开标识随机数不可用")]
    EntropyUnavailable,
}

/// 不暴露数据库自增顺序的企业服务账号公开标识。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationServiceAccountKey([u8; SERVICE_ACCOUNT_KEY_BYTES]);

impl OrganizationServiceAccountKey {
    /// 生成非零随机公开标识；密钥材料不在此类型中保存。
    pub fn generate() -> Result<Self, OrganizationServiceAccountIdentifierError> {
        let mut bytes = [0_u8; SERVICE_ACCOUNT_KEY_BYTES];
        fill(&mut bytes)
            .map_err(|_| OrganizationServiceAccountIdentifierError::EntropyUnavailable)?;
        Self::new(bytes)
    }

    /// 校验固定长度且非全零的公开标识。
    pub const fn new(
        bytes: [u8; SERVICE_ACCOUNT_KEY_BYTES],
    ) -> Result<Self, OrganizationServiceAccountIdentifierError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(OrganizationServiceAccountIdentifierError::AllZero)
    }

    /// 从数据库恢复小写十六进制公开标识。
    pub fn from_persistence_key(
        value: &str,
    ) -> Result<Self, OrganizationServiceAccountIdentifierError> {
        if value.len() != SERVICE_ACCOUNT_KEY_HEX_BYTES {
            return Err(OrganizationServiceAccountIdentifierError::InvalidEncoding);
        }
        let mut bytes = [0_u8; SERVICE_ACCOUNT_KEY_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex(pair[0])
                .ok_or(OrganizationServiceAccountIdentifierError::InvalidEncoding)?;
            let low = decode_hex(pair[1])
                .ok_or(OrganizationServiceAccountIdentifierError::InvalidEncoding)?;
            bytes[index] = (high << 4) | low;
        }
        Self::new(bytes)
    }

    /// 返回数据库和管理接口共用的稳定文本。
    #[must_use]
    pub fn persistence_key(self) -> String {
        let mut encoded = String::with_capacity(SERVICE_ACCOUNT_KEY_HEX_BYTES);
        for byte in self.0 {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        encoded
    }
}

impl TryFrom<[u8; SERVICE_ACCOUNT_KEY_BYTES]> for OrganizationServiceAccountKey {
    type Error = OrganizationServiceAccountIdentifierError;

    fn try_from(bytes: [u8; SERVICE_ACCOUNT_KEY_BYTES]) -> Result<Self, Self::Error> {
        Self::new(bytes)
    }
}

impl fmt::Debug for OrganizationServiceAccountKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationServiceAccountKey(<redacted>)")
    }
}

/// 企业服务账号名称的构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationServiceAccountNameError {
    /// 名称不能为空或超过固定字节上限。
    #[error("企业服务账号名称长度无效")]
    InvalidLength,
    /// 名称不能带首尾空白或控制字符。
    #[error("企业服务账号名称包含非法字符")]
    InvalidCharacters,
}

/// 受限的服务账号显示名称；名称不参与权限或主体寻址。
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationServiceAccountName(String);

impl OrganizationServiceAccountName {
    /// 校验名称边界，不做大小写或 Unicode 归一化。
    pub fn new(value: impl Into<String>) -> Result<Self, OrganizationServiceAccountNameError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_SERVICE_ACCOUNT_NAME_BYTES {
            return Err(OrganizationServiceAccountNameError::InvalidLength);
        }
        if value.trim() != value || value.chars().any(char::is_control) {
            return Err(OrganizationServiceAccountNameError::InvalidCharacters);
        }
        Ok(Self(value))
    }

    /// 返回管理端和持久化层共用的名称文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for OrganizationServiceAccountName {
    type Error = OrganizationServiceAccountNameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for OrganizationServiceAccountName {
    type Error = OrganizationServiceAccountNameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl AsRef<str> for OrganizationServiceAccountName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Debug for OrganizationServiceAccountName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationServiceAccountName(<redacted>)")
    }
}

/// 企业服务账号生命周期状态；Revoked 是不可恢复的终态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationServiceAccountStatus {
    /// 可签发和使用后续受控密钥。
    Active = 1,
    /// 暂停运行但保留主体、权限和审计事实。
    Suspended = 2,
    /// 永久撤销，不能恢复或重新使用。
    Revoked = 3,
}

impl OrganizationServiceAccountStatus {
    /// 返回数据库使用的稳定状态码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 从数据库恢复闭合状态集合。
    pub const fn from_code(value: i16) -> Result<Self, OrganizationServiceAccountLifecycleError> {
        match value {
            1 => Ok(Self::Active),
            2 => Ok(Self::Suspended),
            3 => Ok(Self::Revoked),
            _ => Err(OrganizationServiceAccountLifecycleError::InvalidStatus),
        }
    }

    /// 判断服务账号是否可以参与新的运行时调用。
    #[must_use]
    pub const fn is_usable(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 企业服务账号作用域；所有后续存储和授权查询都必须携带企业与主体双边界。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OrganizationServiceAccountLocator {
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
}

impl OrganizationServiceAccountLocator {
    /// 构造已通过类型校验的企业服务账号定位器。
    #[must_use]
    pub const fn new(
        organization_id: OrganizationId,
        service_account_id: OrganizationServiceAccountId,
    ) -> Self {
        Self {
            organization_id,
            service_account_id,
        }
    }

    /// 返回企业边界。
    #[must_use]
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }

    /// 返回服务账号主体标识。
    #[must_use]
    pub const fn service_account_id(self) -> OrganizationServiceAccountId {
        self.service_account_id
    }

    /// 仅允许同一企业的定位器参与后续资源操作。
    #[must_use]
    pub const fn belongs_to(self, organization_id: OrganizationId) -> bool {
        self.organization_id.get() == organization_id.get()
    }
}

/// 不绑定自然人身份的企业服务账号记录。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationServiceAccount {
    locator: OrganizationServiceAccountLocator,
    public_key: OrganizationServiceAccountKey,
    name: OrganizationServiceAccountName,
    status: OrganizationServiceAccountStatus,
}

impl OrganizationServiceAccount {
    /// 创建处于 Active 状态的新服务账号；自然人、成员角色和钱包不在记录中。
    #[must_use]
    pub const fn new(
        locator: OrganizationServiceAccountLocator,
        public_key: OrganizationServiceAccountKey,
        name: OrganizationServiceAccountName,
    ) -> Self {
        Self {
            locator,
            public_key,
            name,
            status: OrganizationServiceAccountStatus::Active,
        }
    }

    /// 从持久化状态恢复服务账号，状态值必须来自闭合集合。
    pub const fn from_persistence(
        locator: OrganizationServiceAccountLocator,
        public_key: OrganizationServiceAccountKey,
        name: OrganizationServiceAccountName,
        status: OrganizationServiceAccountStatus,
    ) -> Self {
        Self {
            locator,
            public_key,
            name,
            status,
        }
    }

    /// 返回企业与主体的双重定位器。
    #[must_use]
    pub const fn locator(&self) -> OrganizationServiceAccountLocator {
        self.locator
    }

    /// 返回不具备数据库顺序的公开标识。
    #[must_use]
    pub const fn public_key(&self) -> OrganizationServiceAccountKey {
        self.public_key
    }

    /// 返回显示名称。
    #[must_use]
    pub fn name(&self) -> &OrganizationServiceAccountName {
        &self.name
    }

    /// 返回当前生命周期状态。
    #[must_use]
    pub const fn status(&self) -> OrganizationServiceAccountStatus {
        self.status
    }
}

impl fmt::Debug for OrganizationServiceAccount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationServiceAccount")
            .field("locator", &self.locator)
            .field("public_key", &self.public_key)
            .field("name", &self.name)
            .field("status", &self.status)
            .finish()
    }
}

/// 服务账号生命周期恢复和非法状态错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationServiceAccountLifecycleError {
    /// 数据库中的状态码不属于闭合集合。
    #[error("企业服务账号生命周期状态无效")]
    InvalidStatus,
    /// Revoked 主体不可恢复或重新激活。
    #[error("企业服务账号已撤销且不可恢复")]
    RevokedImmutable,
}

/// 生命周期变更的结构化结果；调用方必须显式处理拒绝。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationServiceAccountLifecycleDisposition {
    /// 目标状态与当前状态一致。
    Noop,
    /// 暂停 Active 主体。
    Suspend,
    /// 恢复 Suspended 主体。
    Resume,
    /// 撤销 Active 或 Suspended 主体。
    Revoke,
    /// 变更被不变量拒绝。
    Reject(OrganizationServiceAccountLifecycleError),
}

/// 计算服务账号生命周期下一步，不执行数据库或权限写入。
#[must_use]
pub const fn classify_organization_service_account_lifecycle(
    current: OrganizationServiceAccountStatus,
    target: OrganizationServiceAccountStatus,
) -> OrganizationServiceAccountLifecycleDisposition {
    match (current, target) {
        (OrganizationServiceAccountStatus::Revoked, OrganizationServiceAccountStatus::Revoked)
        | (OrganizationServiceAccountStatus::Active, OrganizationServiceAccountStatus::Active)
        | (
            OrganizationServiceAccountStatus::Suspended,
            OrganizationServiceAccountStatus::Suspended,
        ) => OrganizationServiceAccountLifecycleDisposition::Noop,
        (OrganizationServiceAccountStatus::Active, OrganizationServiceAccountStatus::Suspended) => {
            OrganizationServiceAccountLifecycleDisposition::Suspend
        }
        (OrganizationServiceAccountStatus::Suspended, OrganizationServiceAccountStatus::Active) => {
            OrganizationServiceAccountLifecycleDisposition::Resume
        }
        (
            OrganizationServiceAccountStatus::Active | OrganizationServiceAccountStatus::Suspended,
            OrganizationServiceAccountStatus::Revoked,
        ) => OrganizationServiceAccountLifecycleDisposition::Revoke,
        (
            OrganizationServiceAccountStatus::Revoked,
            OrganizationServiceAccountStatus::Active | OrganizationServiceAccountStatus::Suspended,
        ) => OrganizationServiceAccountLifecycleDisposition::Reject(
            OrganizationServiceAccountLifecycleError::RevokedImmutable,
        ),
    }
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
