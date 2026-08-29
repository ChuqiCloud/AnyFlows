use thiserror::Error;

use crate::Quota;

/// 自定义预算周期的最短秒数，避免创建高频窗口放大数据库写入。
pub const MIN_ORGANIZATION_BUDGET_FIXED_PERIOD_SECONDS: u32 = 60;
/// 自定义预算周期的最长秒数，首版限制为 366 天。
pub const MAX_ORGANIZATION_BUDGET_FIXED_PERIOD_SECONDS: u32 = 366 * 24 * 60 * 60;

/// 企业预算策略输入或持久化值错误；错误中不保留额度与主体标识。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationBudgetPolicyError {
    /// 预算作用域不属于企业、团队、成员或部门闭合集合。
    #[error("企业预算作用域无效")]
    InvalidScope,
    /// 周期编码与固定周期秒数不匹配。
    #[error("企业预算周期无效")]
    InvalidPeriod,
    /// 自定义固定周期不在允许范围内。
    #[error("企业预算固定周期无效")]
    InvalidFixedPeriod,
    /// 有限预算必须严格大于零；无限预算使用空值表达。
    #[error("企业预算有限额度必须大于零")]
    ZeroLimit,
}

/// 企业预算策略的闭合作用域。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationBudgetScopeKind {
    /// 整个企业共享的消费上限。
    Organization = 1,
    /// 企业内单个团队共享的消费上限。
    Team = 2,
    /// 企业内单个成员独享的消费上限。
    Member = 3,
    /// 企业内单个部门（从根到叶路径中的一层）共享的消费上限。
    Department = 4,
}

impl OrganizationBudgetScopeKind {
    /// 返回持久化使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

impl TryFrom<i16> for OrganizationBudgetScopeKind {
    type Error = OrganizationBudgetPolicyError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Organization),
            2 => Ok(Self::Team),
            3 => Ok(Self::Member),
            4 => Ok(Self::Department),
            _ => Err(OrganizationBudgetPolicyError::InvalidScope),
        }
    }
}

/// 企业预算窗口周期；自然周期均以 UTC 计算。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationBudgetPeriod {
    /// UTC 自然日。
    UtcDay,
    /// UTC 周一开始的自然周。
    UtcWeek,
    /// UTC 自然月。
    UtcMonth,
    /// 从策略生效时间锚定的固定秒数周期。
    Fixed { seconds: u32 },
}

impl OrganizationBudgetPeriod {
    /// 构造受限固定周期。
    pub const fn fixed(seconds: u32) -> Result<Self, OrganizationBudgetPolicyError> {
        if seconds < MIN_ORGANIZATION_BUDGET_FIXED_PERIOD_SECONDS
            || seconds > MAX_ORGANIZATION_BUDGET_FIXED_PERIOD_SECONDS
        {
            Err(OrganizationBudgetPolicyError::InvalidFixedPeriod)
        } else {
            Ok(Self::Fixed { seconds })
        }
    }

    /// 从数据库的闭合编码与可空固定秒数恢复周期。
    pub const fn from_persistence(
        code: i16,
        fixed_seconds: Option<u32>,
    ) -> Result<Self, OrganizationBudgetPolicyError> {
        match (code, fixed_seconds) {
            (1, None) => Ok(Self::UtcDay),
            (2, None) => Ok(Self::UtcWeek),
            (3, None) => Ok(Self::UtcMonth),
            (4, Some(seconds)) => Self::fixed(seconds),
            _ => Err(OrganizationBudgetPolicyError::InvalidPeriod),
        }
    }

    /// 返回持久化使用的稳定周期编码。
    #[must_use]
    pub const fn code(self) -> i16 {
        match self {
            Self::UtcDay => 1,
            Self::UtcWeek => 2,
            Self::UtcMonth => 3,
            Self::Fixed { .. } => 4,
        }
    }

    /// 返回固定周期秒数；自然周期返回空值。
    #[must_use]
    pub const fn fixed_seconds(self) -> Option<u32> {
        match self {
            Self::Fixed { seconds } => Some(seconds),
            Self::UtcDay | Self::UtcWeek | Self::UtcMonth => None,
        }
    }
}

/// 可空企业预算上限；空值显式表示该策略层不限制。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationBudgetLimit(Option<Quota>);

impl OrganizationBudgetLimit {
    /// 校验有限预算严格大于零并保留无限预算空值。
    pub const fn new(limit: Option<Quota>) -> Result<Self, OrganizationBudgetPolicyError> {
        match limit {
            Some(value) if value.is_zero() => Err(OrganizationBudgetPolicyError::ZeroLimit),
            _ => Ok(Self(limit)),
        }
    }

    /// 返回持久化边界使用的可空额度。
    #[must_use]
    pub const fn quota(self) -> Option<Quota> {
        self.0
    }
}
