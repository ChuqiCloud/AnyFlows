use std::fmt;

use thiserror::Error;

const ORGANIZATION_KEY_BYTES: usize = 16;
const ORGANIZATION_KEY_HEX_BYTES: usize = ORGANIZATION_KEY_BYTES * 2;
const ORGANIZATION_INVITATION_KEY_BYTES: usize = 16;
const ORGANIZATION_INVITATION_KEY_HEX_BYTES: usize = ORGANIZATION_INVITATION_KEY_BYTES * 2;
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

/// 不暴露数据库顺序的非零 128 位企业邀请公开标识。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationInvitationKey([u8; ORGANIZATION_INVITATION_KEY_BYTES]);

impl OrganizationInvitationKey {
    /// 校验并构造非零企业邀请标识。
    pub const fn new(
        bytes: [u8; ORGANIZATION_INVITATION_KEY_BYTES],
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

    /// 从数据库和邀请令牌使用的 32 位小写十六进制文本恢复标识。
    pub fn from_persistence_key(value: &str) -> Result<Self, OrganizationIdentifierError> {
        if value.len() != ORGANIZATION_INVITATION_KEY_HEX_BYTES {
            return Err(OrganizationIdentifierError::InvalidEncoding);
        }
        let mut bytes = [0_u8; ORGANIZATION_INVITATION_KEY_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex(pair[0]).ok_or(OrganizationIdentifierError::InvalidEncoding)?;
            let low = decode_hex(pair[1]).ok_or(OrganizationIdentifierError::InvalidEncoding)?;
            bytes[index] = (high << 4) | low;
        }
        Self::new(bytes)
    }

    /// 返回数据库和邀请令牌共用的稳定持久化文本。
    #[must_use]
    pub fn persistence_key(self) -> String {
        let mut encoded = String::with_capacity(ORGANIZATION_INVITATION_KEY_HEX_BYTES);
        for byte in self.0 {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        encoded
    }
}

impl fmt::Debug for OrganizationInvitationKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationInvitationKey(<redacted>)")
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
    /// 邀请状态不属于闭合集合。
    #[error("企业邀请状态无效")]
    InvalidInvitationStatus,
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

/// 企业空间内固定的权限码；与平台管理员权限保持独立。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationPermission {
    /// 查看企业概览与成员目录。
    OverviewRead,
    /// 修改企业资料。
    ProfileWrite,
    /// 转移企业所有权。
    OwnershipTransfer,
    /// 关闭企业。
    Close,
    /// 管理成员生命周期与角色。
    MembersManage,
    /// 管理团队及成员归属。
    TeamsManage,
    /// 查看企业钱包、订单和成本摘要。
    WalletRead,
    /// 充值并管理企业预算。
    WalletManage,
    /// 管理企业、团队与成员预算策略。
    BudgetsManage,
    /// 查看企业全部 API Key 脱敏目录。
    KeysReadAll,
    /// 创建或停用当前成员自己的企业 Key。
    KeysManageSelf,
    /// 查看企业完整调用日志。
    UsageLogsReadAll,
    /// 查看当前成员自己的调用日志。
    UsageLogsReadSelf,
    /// 查看企业管理审计记录。
    AuditRead,
}

impl OrganizationPermission {
    /// 返回对外稳定的权限码，路由和审计只依赖该文本。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::OverviewRead => "organization.overview.read",
            Self::ProfileWrite => "organization.profile.write",
            Self::OwnershipTransfer => "organization.ownership.transfer",
            Self::Close => "organization.close",
            Self::MembersManage => "organization.members.manage",
            Self::TeamsManage => "organization.teams.manage",
            Self::WalletRead => "organization.wallet.read",
            Self::WalletManage => "organization.wallet.manage",
            Self::BudgetsManage => "organization.budgets.manage",
            Self::KeysReadAll => "organization.keys.read_all",
            Self::KeysManageSelf => "organization.keys.manage_self",
            Self::UsageLogsReadAll => "organization.usage_logs.read_all",
            Self::UsageLogsReadSelf => "organization.usage_logs.read_self",
            Self::AuditRead => "organization.audit.read",
        }
    }
}

/// 平台域权限；平台管理员权限不能伪装成企业角色权限。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlatformPermission {
    /// 跨企业读取企业目录。
    OrganizationsReadAll,
}

impl PlatformPermission {
    /// 返回对外稳定的平台权限码。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::OrganizationsReadAll => "platform.organizations.read_all",
        }
    }
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

    /// 返回固定角色权限矩阵，不读取平台角色或客户端输入。
    #[must_use]
    pub const fn permissions(self) -> &'static [OrganizationPermission] {
        use OrganizationPermission as Permission;
        match self {
            Self::Owner => &[
                Permission::OverviewRead,
                Permission::ProfileWrite,
                Permission::OwnershipTransfer,
                Permission::Close,
                Permission::MembersManage,
                Permission::TeamsManage,
                Permission::WalletRead,
                Permission::WalletManage,
                Permission::BudgetsManage,
                Permission::KeysReadAll,
                Permission::KeysManageSelf,
                Permission::UsageLogsReadAll,
                Permission::UsageLogsReadSelf,
                Permission::AuditRead,
            ],
            Self::Admin => &[
                Permission::OverviewRead,
                Permission::ProfileWrite,
                Permission::MembersManage,
                Permission::TeamsManage,
                Permission::WalletRead,
                Permission::WalletManage,
                Permission::BudgetsManage,
                Permission::KeysReadAll,
                Permission::KeysManageSelf,
                Permission::UsageLogsReadAll,
                Permission::UsageLogsReadSelf,
                Permission::AuditRead,
            ],
            Self::Billing => &[
                Permission::OverviewRead,
                Permission::WalletRead,
                Permission::WalletManage,
                Permission::BudgetsManage,
                Permission::UsageLogsReadAll,
                Permission::UsageLogsReadSelf,
                Permission::AuditRead,
            ],
            Self::Member => &[
                Permission::OverviewRead,
                Permission::KeysManageSelf,
                Permission::UsageLogsReadSelf,
            ],
            Self::Viewer => &[
                Permission::OverviewRead,
                Permission::UsageLogsReadSelf,
                Permission::AuditRead,
            ],
        }
    }

    /// 判断固定角色是否包含指定企业权限。
    #[must_use]
    pub fn allows(self, permission: OrganizationPermission) -> bool {
        self.permissions().contains(&permission)
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

/// 企业邀请生命周期状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationInvitationStatus {
    /// 邀请尚未消费且可能仍占用席位。
    Pending = 1,
    /// 邀请已经由匹配用户单次消费。
    Accepted = 2,
    /// 邀请由企业管理员撤销或被重发替换。
    Revoked = 3,
    /// 邀请超过受信过期时间后关闭。
    Expired = 4,
}

impl OrganizationInvitationStatus {
    /// 返回数据库使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断邀请是否仍可参与接受事务。
    #[must_use]
    pub const fn is_pending(self) -> bool {
        matches!(self, Self::Pending)
    }
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
state_code_conversion!(
    OrganizationInvitationStatus,
    OrganizationStateCodeError::InvalidInvitationStatus,
    {
        1 => OrganizationInvitationStatus::Pending,
        2 => OrganizationInvitationStatus::Accepted,
        3 => OrganizationInvitationStatus::Revoked,
        4 => OrganizationInvitationStatus::Expired,
    }
);

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
