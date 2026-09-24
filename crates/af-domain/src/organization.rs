use std::fmt;

use thiserror::Error;

const ORGANIZATION_KEY_BYTES: usize = 16;
const ORGANIZATION_KEY_HEX_BYTES: usize = ORGANIZATION_KEY_BYTES * 2;
const ORGANIZATION_CUSTOM_ROLE_KEY_BYTES: usize = 16;
const ORGANIZATION_CUSTOM_ROLE_KEY_HEX_BYTES: usize = ORGANIZATION_CUSTOM_ROLE_KEY_BYTES * 2;
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

/// 不暴露数据库顺序的非零 128 位企业自定义角色公开标识。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OrganizationCustomRoleKey([u8; ORGANIZATION_CUSTOM_ROLE_KEY_BYTES]);

impl OrganizationCustomRoleKey {
    /// 校验并构造非零企业自定义角色公开标识。
    pub const fn new(
        bytes: [u8; ORGANIZATION_CUSTOM_ROLE_KEY_BYTES],
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

    /// 从数据库和接口使用的 32 位小写十六进制文本恢复标识。
    pub fn from_persistence_key(value: &str) -> Result<Self, OrganizationIdentifierError> {
        if value.len() != ORGANIZATION_CUSTOM_ROLE_KEY_HEX_BYTES {
            return Err(OrganizationIdentifierError::InvalidEncoding);
        }
        let mut bytes = [0_u8; ORGANIZATION_CUSTOM_ROLE_KEY_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex(pair[0]).ok_or(OrganizationIdentifierError::InvalidEncoding)?;
            let low = decode_hex(pair[1]).ok_or(OrganizationIdentifierError::InvalidEncoding)?;
            bytes[index] = (high << 4) | low;
        }
        Self::new(bytes)
    }

    /// 返回数据库和接口共用的稳定持久化文本。
    #[must_use]
    pub fn persistence_key(self) -> String {
        let mut encoded = String::with_capacity(ORGANIZATION_CUSTOM_ROLE_KEY_HEX_BYTES);
        for byte in self.0 {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        encoded
    }
}

impl fmt::Debug for OrganizationCustomRoleKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationCustomRoleKey(<redacted>)")
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
    /// 企业自定义角色状态不属于闭合集合。
    #[error("企业自定义角色状态无效")]
    InvalidCustomRoleStatus,
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

/// 企业自定义角色目录状态；停用后保留定义与权限事实。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationCustomRoleStatus {
    /// 角色可参与后续成员绑定和授权投影。
    Active = 1,
    /// 角色已停用，仅保留历史事实。
    Disabled = 2,
}

impl OrganizationCustomRoleStatus {
    /// 返回数据库使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断角色是否可参与授权投影。
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 企业权限文本解析错误；错误中不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPermissionCodeError {
    /// 权限码不属于服务端闭合集合。
    #[error("企业权限码无效")]
    Unknown,
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
    /// 查看企业部门目录。
    DepartmentsRead,
    /// 创建、移动和禁用企业部门。
    DepartmentsManage,
    /// 查看企业钱包、订单和成本摘要。
    WalletRead,
    /// 充值并管理企业预算。
    WalletManage,
    /// 管理企业、团队与成员预算策略。
    BudgetsManage,
    /// 查看企业授信策略、账期账单和还款核销事实。
    CreditRead,
    /// 管理企业授信策略和账期状态。
    CreditManage,
    /// 查看企业合同价版本与生效区间。
    ContractPricesRead,
    /// 创建和关闭企业合同价版本。
    ContractPricesManage,
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
    /// 查看企业自定义角色目录。
    RolesRead,
    /// 管理企业自定义角色定义。
    RolesManage,
    /// 为企业成员分配自定义角色。
    RolesAssign,
    /// 读取企业 SSO 的脱敏配置。
    SsoRead,
    /// 管理企业 SSO Provider、域名和策略。
    SsoManage,
    /// 管理企业 SSO 恢复入口。
    SsoRecovery,
    /// 查看企业服务账号目录。
    ServiceAccountsRead,
    /// 管理企业服务账号生命周期与密钥。
    ServiceAccountsManage,
    /// 查看企业审批模板与申请。
    ApprovalsRead,
    /// 创建企业审批申请与模板版本。
    ApprovalsCreate,
    /// 管理企业审批模板版本。
    ApprovalsManage,
    /// 提交企业审批决定。
    ApprovalsDecide,
}

const CUSTOM_ROLE_GRANTABLE_PERMISSIONS: &[OrganizationPermission] = &[
    OrganizationPermission::OverviewRead,
    OrganizationPermission::ProfileWrite,
    OrganizationPermission::MembersManage,
    OrganizationPermission::TeamsManage,
    OrganizationPermission::DepartmentsRead,
    OrganizationPermission::WalletRead,
    OrganizationPermission::WalletManage,
    OrganizationPermission::BudgetsManage,
    OrganizationPermission::CreditRead,
    OrganizationPermission::ContractPricesRead,
    OrganizationPermission::ContractPricesManage,
    OrganizationPermission::KeysReadAll,
    OrganizationPermission::KeysManageSelf,
    OrganizationPermission::UsageLogsReadAll,
    OrganizationPermission::UsageLogsReadSelf,
    OrganizationPermission::AuditRead,
    OrganizationPermission::RolesRead,
    OrganizationPermission::ApprovalsRead,
    OrganizationPermission::ApprovalsCreate,
];

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
            Self::DepartmentsRead => "organization.departments.read",
            Self::DepartmentsManage => "organization.departments.manage",
            Self::WalletRead => "organization.wallet.read",
            Self::WalletManage => "organization.wallet.manage",
            Self::BudgetsManage => "organization.budgets.manage",
            Self::CreditRead => "organization.credit.read",
            Self::CreditManage => "organization.credit.manage",
            Self::ContractPricesRead => "organization.contract_prices.read",
            Self::ContractPricesManage => "organization.contract_prices.manage",
            Self::KeysReadAll => "organization.keys.read_all",
            Self::KeysManageSelf => "organization.keys.manage_self",
            Self::UsageLogsReadAll => "organization.usage_logs.read_all",
            Self::UsageLogsReadSelf => "organization.usage_logs.read_self",
            Self::AuditRead => "organization.audit.read",
            Self::RolesRead => "organization.roles.read",
            Self::RolesManage => "organization.roles.manage",
            Self::RolesAssign => "organization.roles.assign",
            Self::SsoRead => "organization.sso.read",
            Self::SsoManage => "organization.sso.manage",
            Self::SsoRecovery => "organization.sso.recovery",
            Self::ServiceAccountsRead => "organization.service_accounts.read",
            Self::ServiceAccountsManage => "organization.service_accounts.manage",
            Self::ApprovalsRead => "organization.approvals.read",
            Self::ApprovalsCreate => "organization.approvals.create",
            Self::ApprovalsManage => "organization.approvals.manage",
            Self::ApprovalsDecide => "organization.approvals.decide",
        }
    }

    /// 从持久化文本恢复闭合权限枚举，未知或平台权限统一拒绝。
    pub fn from_code(value: &str) -> Result<Self, OrganizationPermissionCodeError> {
        match value {
            "organization.overview.read" => Ok(Self::OverviewRead),
            "organization.profile.write" => Ok(Self::ProfileWrite),
            "organization.ownership.transfer" => Ok(Self::OwnershipTransfer),
            "organization.close" => Ok(Self::Close),
            "organization.members.manage" => Ok(Self::MembersManage),
            "organization.teams.manage" => Ok(Self::TeamsManage),
            "organization.departments.read" => Ok(Self::DepartmentsRead),
            "organization.departments.manage" => Ok(Self::DepartmentsManage),
            "organization.wallet.read" => Ok(Self::WalletRead),
            "organization.wallet.manage" => Ok(Self::WalletManage),
            "organization.budgets.manage" => Ok(Self::BudgetsManage),
            "organization.credit.read" => Ok(Self::CreditRead),
            "organization.credit.manage" => Ok(Self::CreditManage),
            "organization.contract_prices.read" => Ok(Self::ContractPricesRead),
            "organization.contract_prices.manage" => Ok(Self::ContractPricesManage),
            "organization.keys.read_all" => Ok(Self::KeysReadAll),
            "organization.keys.manage_self" => Ok(Self::KeysManageSelf),
            "organization.usage_logs.read_all" => Ok(Self::UsageLogsReadAll),
            "organization.usage_logs.read_self" => Ok(Self::UsageLogsReadSelf),
            "organization.audit.read" => Ok(Self::AuditRead),
            "organization.roles.read" => Ok(Self::RolesRead),
            "organization.roles.manage" => Ok(Self::RolesManage),
            "organization.roles.assign" => Ok(Self::RolesAssign),
            "organization.sso.read" => Ok(Self::SsoRead),
            "organization.sso.manage" => Ok(Self::SsoManage),
            "organization.sso.recovery" => Ok(Self::SsoRecovery),
            "organization.service_accounts.read" => Ok(Self::ServiceAccountsRead),
            "organization.service_accounts.manage" => Ok(Self::ServiceAccountsManage),
            "organization.approvals.read" => Ok(Self::ApprovalsRead),
            "organization.approvals.create" => Ok(Self::ApprovalsCreate),
            "organization.approvals.manage" => Ok(Self::ApprovalsManage),
            "organization.approvals.decide" => Ok(Self::ApprovalsDecide),
            _ => Err(OrganizationPermissionCodeError::Unknown),
        }
    }

    /// 判断权限是否允许写入企业自定义角色。
    #[must_use]
    pub const fn is_custom_role_grantable(self) -> bool {
        !matches!(
            self,
            Self::OwnershipTransfer
                | Self::Close
                | Self::RolesManage
                | Self::RolesAssign
                | Self::DepartmentsManage
                | Self::SsoRead
                | Self::SsoManage
                | Self::SsoRecovery
                | Self::ServiceAccountsRead
                | Self::ServiceAccountsManage
                | Self::CreditManage
                | Self::ApprovalsDecide
                | Self::ApprovalsManage
        )
    }

    /// 返回服务端维护的自定义角色可授予权限白名单。
    #[must_use]
    pub const fn custom_role_grantable_permissions() -> &'static [Self] {
        CUSTOM_ROLE_GRANTABLE_PERMISSIONS
    }
}

/// 平台域权限；平台管理员权限不能伪装成企业角色权限。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlatformPermission {
    /// 读取平台用户目录。
    UserDirectoryReadAll,
    /// 读取全站经营看板和性能健康。
    DashboardReadAll,
    /// 读取全站平台管理审计。
    PlatformAuditReadAll,
    /// 跨企业读取企业目录。
    OrganizationsReadAll,
    /// 跨企业读取企业调用日志。
    OrganizationUsageLogsReadAll,
    /// 跨企业读取企业管理审计。
    OrganizationAuditReadAll,
    /// 审批企业开通申请并管理企业生命周期。
    OrganizationsManage,
    /// 管理自定义 OAuth2 Provider 配置。
    CustomOAuth2ProvidersManage,
    /// 管理模型厂商目录。
    ModelProvidersManage,
}

impl PlatformPermission {
    /// 返回对外稳定的平台权限码。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::UserDirectoryReadAll => "platform.user_directory.read_all",
            Self::DashboardReadAll => "platform.dashboard.read_all",
            Self::PlatformAuditReadAll => "platform.audit.read_all",
            Self::OrganizationsReadAll => "platform.organizations.read_all",
            Self::OrganizationUsageLogsReadAll => "platform.organization_usage_logs.read_all",
            Self::OrganizationAuditReadAll => "platform.organization_audit.read_all",
            Self::OrganizationsManage => "platform.organizations.manage",
            Self::CustomOAuth2ProvidersManage => "platform.custom_oauth2_providers.manage",
            Self::ModelProvidersManage => "platform.model_providers.manage",
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
                Permission::DepartmentsRead,
                Permission::DepartmentsManage,
                Permission::WalletRead,
                Permission::WalletManage,
                Permission::BudgetsManage,
                Permission::CreditRead,
                Permission::ContractPricesRead,
                Permission::ContractPricesManage,
                Permission::KeysReadAll,
                Permission::KeysManageSelf,
                Permission::UsageLogsReadAll,
                Permission::UsageLogsReadSelf,
                Permission::AuditRead,
                Permission::RolesRead,
                Permission::RolesManage,
                Permission::RolesAssign,
                Permission::SsoRead,
                Permission::SsoManage,
                Permission::SsoRecovery,
                Permission::ServiceAccountsRead,
                Permission::ServiceAccountsManage,
                Permission::ApprovalsRead,
                Permission::ApprovalsCreate,
                Permission::ApprovalsManage,
                Permission::ApprovalsDecide,
            ],
            Self::Admin => &[
                Permission::OverviewRead,
                Permission::ProfileWrite,
                Permission::MembersManage,
                Permission::TeamsManage,
                Permission::DepartmentsRead,
                Permission::DepartmentsManage,
                Permission::WalletRead,
                Permission::WalletManage,
                Permission::BudgetsManage,
                Permission::CreditRead,
                Permission::CreditManage,
                Permission::ContractPricesRead,
                Permission::ContractPricesManage,
                Permission::KeysReadAll,
                Permission::KeysManageSelf,
                Permission::UsageLogsReadAll,
                Permission::UsageLogsReadSelf,
                Permission::AuditRead,
                Permission::RolesRead,
                Permission::RolesManage,
                Permission::RolesAssign,
                Permission::SsoRead,
                Permission::SsoManage,
                Permission::ServiceAccountsRead,
                Permission::ServiceAccountsManage,
                Permission::ApprovalsRead,
                Permission::ApprovalsCreate,
                Permission::ApprovalsManage,
                Permission::ApprovalsDecide,
            ],
            Self::Billing => &[
                Permission::OverviewRead,
                Permission::WalletRead,
                Permission::WalletManage,
                Permission::BudgetsManage,
                Permission::CreditRead,
                Permission::CreditManage,
                Permission::ContractPricesRead,
                Permission::ContractPricesManage,
                Permission::UsageLogsReadAll,
                Permission::UsageLogsReadSelf,
                Permission::AuditRead,
                Permission::ApprovalsRead,
                Permission::ApprovalsCreate,
            ],
            Self::Member => &[
                Permission::OverviewRead,
                Permission::KeysManageSelf,
                Permission::UsageLogsReadSelf,
                Permission::ApprovalsCreate,
            ],
            Self::Viewer => &[
                Permission::OverviewRead,
                Permission::DepartmentsRead,
                Permission::UsageLogsReadSelf,
                Permission::AuditRead,
                Permission::ApprovalsRead,
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
    OrganizationCustomRoleStatus,
    OrganizationStateCodeError::InvalidCustomRoleStatus,
    {
        1 => OrganizationCustomRoleStatus::Active,
        2 => OrganizationCustomRoleStatus::Disabled,
    }
);
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
