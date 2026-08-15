use std::fmt;

use thiserror::Error;

const ORGANIZATION_KEY_BYTES: usize = 16;
const ORGANIZATION_KEY_HEX_BYTES: usize = ORGANIZATION_KEY_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 企业公开标识构造错误；错误中不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationIdentifierError {
    /// 全零标识不能作为稳定的企业边界。
    #[error("企业公开标识不能全为零")]
    AllZero,
    /// 持久化值不是固定长度的小写十六进制文本。
    #[error("企业公开标识格式无效")]
    InvalidEncoding,
}

/// 不暴露数据库顺序的非零 128 位企业公开标识。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationKey([u8; ORGANIZATION_KEY_BYTES]);

impl OrganizationKey {
    /// 校验并构造非零企业公开标识。
    pub const fn new(
        bytes: [u8; ORGANIZATION_KEY_BYTES],
    ) -> Result<Self, OrganizationIdentifierError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(OrganizationIdentifierError::AllZero)
    }

    /// 从数据库使用的 32 位小写十六进制文本恢复标识。
    pub fn from_persistence_key(value: &str) -> Result<Self, OrganizationIdentifierError> {
        if value.len() != ORGANIZATION_KEY_HEX_BYTES {
            return Err(OrganizationIdentifierError::InvalidEncoding);
        }
        let mut bytes = [0_u8; ORGANIZATION_KEY_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex(pair[0]).ok_or(OrganizationIdentifierError::InvalidEncoding)?;
            let low = decode_hex(pair[1]).ok_or(OrganizationIdentifierError::InvalidEncoding)?;
            bytes[index] = (high << 4) | low;
        }
        Self::new(bytes)
    }

    /// 返回数据库和对外协议共用的稳定持久化文本。
    #[must_use]
    pub fn persistence_key(self) -> String {
        let mut encoded = String::with_capacity(ORGANIZATION_KEY_HEX_BYTES);
        for byte in self.0 {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        encoded
    }
}

impl fmt::Debug for OrganizationKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationKey(<redacted>)")
    }
}

/// 企业状态、成员角色和团队状态的持久化值错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationStateCodeError {
    /// 企业状态不属于闭合集合。
    #[error("企业状态无效")]
    InvalidOrganizationStatus,
    /// 企业角色不属于固定五角色集合。
    #[error("企业角色无效")]
    InvalidRole,
    /// 成员状态不属于闭合集合。
    #[error("企业成员状态无效")]
    InvalidMembershipStatus,
    /// 团队状态不属于闭合集合。
    #[error("企业团队状态无效")]
    InvalidTeamStatus,
}

/// 企业生命周期状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationStatus {
    /// 企业可正常参与后续业务。
    Active = 1,
    /// 企业暂时停用，但保留成员、团队和历史事实。
    Suspended = 2,
    /// 企业已关闭，不再恢复为活动状态。
    Closed = 3,
}

impl OrganizationStatus {
    /// 返回数据库使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断企业是否处于活动状态。
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 企业内固定角色；与平台用户角色完全隔离。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationRole {
    /// 企业所有者，受最后 Owner 不变量保护。
    Owner = 1,
    /// 企业日常管理者。
    Admin = 2,
    /// 企业账务管理者。
    Billing = 3,
    /// 企业普通成员。
    Member = 4,
    /// 企业只读成员。
    Viewer = 5,
}

impl OrganizationRole {
    /// 返回数据库使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断角色是否承担企业所有权。
    #[must_use]
    pub const fn is_owner(self) -> bool {
        matches!(self, Self::Owner)
    }
}

/// 企业成员关系生命周期状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationMembershipStatus {
    /// 成员可进入当前企业空间。
    Active = 1,
    /// 成员暂时停用，但保留归属和审计事实。
    Suspended = 2,
    /// 成员已从当前企业移除。
    Removed = 3,
}

impl OrganizationMembershipStatus {
    /// 返回数据库使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断成员关系是否有效。
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 企业团队生命周期状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationTeamStatus {
    /// 团队可接受活动成员归属。
    Active = 1,
    /// 团队已禁用并停止接受成员。
    Disabled = 2,
}

impl OrganizationTeamStatus {
    /// 返回数据库使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断团队是否可用。
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

macro_rules! state_code_conversion {
    ($type:ty, $error:expr, {$($code:literal => $variant:expr),+ $(,)?}) => {
        impl TryFrom<i16> for $type {
            type Error = OrganizationStateCodeError;

            fn try_from(value: i16) -> Result<Self, Self::Error> {
                match value {
                    $($code => Ok($variant),)+
                    _ => Err($error),
                }
            }
        }
    };
}

state_code_conversion!(OrganizationStatus, OrganizationStateCodeError::InvalidOrganizationStatus, {
    1 => OrganizationStatus::Active,
    2 => OrganizationStatus::Suspended,
    3 => OrganizationStatus::Closed,
});
state_code_conversion!(OrganizationRole, OrganizationStateCodeError::InvalidRole, {
    1 => OrganizationRole::Owner,
    2 => OrganizationRole::Admin,
    3 => OrganizationRole::Billing,
    4 => OrganizationRole::Member,
    5 => OrganizationRole::Viewer,
});
state_code_conversion!(
    OrganizationMembershipStatus,
    OrganizationStateCodeError::InvalidMembershipStatus,
    {
        1 => OrganizationMembershipStatus::Active,
        2 => OrganizationMembershipStatus::Suspended,
        3 => OrganizationMembershipStatus::Removed,
    }
);
state_code_conversion!(OrganizationTeamStatus, OrganizationStateCodeError::InvalidTeamStatus, {
    1 => OrganizationTeamStatus::Active,
    2 => OrganizationTeamStatus::Disabled,
});

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
