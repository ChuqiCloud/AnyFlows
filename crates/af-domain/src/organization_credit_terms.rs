use thiserror::Error;

use crate::{OrganizationId, Quota};

/// 企业授信币种必须是三位大写 ASCII 代码。
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OrganizationCreditCurrency(String);

impl OrganizationCreditCurrency {
    /// 校验并构造规范化币种代码。
    pub fn new(value: impl Into<String>) -> Result<Self, OrganizationCreditTermsError> {
        let value = value.into();
        if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(OrganizationCreditTermsError::InvalidCurrency);
        }
        Ok(Self(value))
    }

    /// 返回持久化和接口投影共用的币种代码。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for OrganizationCreditCurrency {
    type Error = OrganizationCreditTermsError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for OrganizationCreditCurrency {
    type Error = OrganizationCreditTermsError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// 企业账期周期；自然月周期以月末为结算边界，不在本切片生成账单。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationCreditTermPeriod {
    /// 从生效时间起计算的固定天数账期。
    NetDays(u16),
    /// 每个自然月月末结算的账期。
    EndOfMonth,
}

impl OrganizationCreditTermPeriod {
    /// 账期天数的服务端上限，避免异常配置形成超长授信窗口。
    pub const MAX_NET_DAYS: u16 = 3_660;

    /// 构造并校验固定天数账期。
    pub const fn net_days(days: u16) -> Result<Self, OrganizationCreditTermsError> {
        if days == 0 || days > Self::MAX_NET_DAYS {
            Err(OrganizationCreditTermsError::InvalidPeriod)
        } else {
            Ok(Self::NetDays(days))
        }
    }

    /// 从持久化编码恢复账期周期。
    pub const fn from_persistence(
        code: i16,
        days: Option<u16>,
    ) -> Result<Self, OrganizationCreditTermsError> {
        match (code, days) {
            (1, Some(value)) => Self::net_days(value),
            (2, None) => Ok(Self::EndOfMonth),
            _ => Err(OrganizationCreditTermsError::InvalidPeriod),
        }
    }

    /// 返回稳定的持久化周期编码。
    #[must_use]
    pub const fn code(self) -> i16 {
        match self {
            Self::NetDays(_) => 1,
            Self::EndOfMonth => 2,
        }
    }

    /// 返回固定天数；月末周期返回空值。
    #[must_use]
    pub const fn net_days_value(self) -> Option<u16> {
        match self {
            Self::NetDays(days) => Some(days),
            Self::EndOfMonth => None,
        }
    }
}

/// 企业授信状态的闭合集合。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationCreditTermStatus {
    /// 允许后续消费链路读取授信事实。
    Active = 1,
    /// 暂停新增授信消费，但保留历史事实。
    Paused = 2,
    /// 永久关闭该版本策略，不允许恢复。
    Closed = 3,
}

impl OrganizationCreditTermStatus {
    /// 返回稳定的持久化状态码。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断状态是否允许新的授信消费读取。
    #[must_use]
    pub const fn allows_consumption(self) -> bool {
        matches!(self, Self::Active)
    }
}

impl TryFrom<i16> for OrganizationCreditTermStatus {
    type Error = OrganizationCreditTermsError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Active),
            2 => Ok(Self::Paused),
            3 => Ok(Self::Closed),
            _ => Err(OrganizationCreditTermsError::InvalidStatus),
        }
    }
}

/// 领域层授信额度；底层复用非负整数额度，不允许负钱包语义。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationCreditLimit(Quota);

impl OrganizationCreditLimit {
    /// 校验并构造非负授信额度；零表示未授予额度。
    pub const fn new(units: i64) -> Result<Self, OrganizationCreditTermsError> {
        match Quota::new(units) {
            Ok(quota) => Ok(Self(quota)),
            Err(_) => Err(OrganizationCreditTermsError::NegativeCreditLimit),
        }
    }

    /// 返回持久化使用的整数额度。
    #[must_use]
    pub const fn units(self) -> i64 {
        self.0.units()
    }

    /// 返回底层非负额度值对象。
    #[must_use]
    pub const fn quota(self) -> Quota {
        self.0
    }
}

/// 企业授信策略领域错误；不携带原始输入，避免错误日志泄露配置。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditTermsError {
    /// 币种不是三位大写 ASCII 代码。
    #[error("企业授信币种无效")]
    InvalidCurrency,
    /// 授信额度为负数。
    #[error("企业授信额度不能为负数")]
    NegativeCreditLimit,
    /// 账期编码或天数超出闭合范围。
    #[error("企业账期周期无效")]
    InvalidPeriod,
    /// 状态编码不属于 Active/Paused/Closed。
    #[error("企业授信状态无效")]
    InvalidStatus,
    /// 生效时间必须是非负 Unix 秒，结束时间必须严格晚于开始时间。
    #[error("企业授信生效区间无效")]
    InvalidEffectiveRange,
    /// 版本必须是正整数。
    #[error("企业授信版本必须大于零")]
    InvalidVersion,
    /// Closed 是终态，不能再次开启。
    #[error("企业授信关闭状态不能恢复")]
    InvalidTransition,
}

/// 企业授信/账期策略的不可变领域快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationCreditTermsSnapshot {
    organization_id: OrganizationId,
    currency: OrganizationCreditCurrency,
    credit_limit: OrganizationCreditLimit,
    period: OrganizationCreditTermPeriod,
    status: OrganizationCreditTermStatus,
    effective_from: i64,
    effective_until: Option<i64>,
    version: i64,
}

impl OrganizationCreditTermsSnapshot {
    /// 校验并构造授信策略事实；本类型不执行扣款、记账或账单生成。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        currency: OrganizationCreditCurrency,
        credit_limit: OrganizationCreditLimit,
        period: OrganizationCreditTermPeriod,
        status: OrganizationCreditTermStatus,
        effective_from: i64,
        effective_until: Option<i64>,
        version: i64,
    ) -> Result<Self, OrganizationCreditTermsError> {
        if effective_from < 0 || effective_until.is_some_and(|until| until <= effective_from) {
            return Err(OrganizationCreditTermsError::InvalidEffectiveRange);
        }
        if version <= 0 {
            return Err(OrganizationCreditTermsError::InvalidVersion);
        }
        Ok(Self {
            organization_id,
            currency,
            credit_limit,
            period,
            status,
            effective_from,
            effective_until,
            version,
        })
    }

    /// 返回该策略所属企业，调用方必须用请求上下文中的同企业标识匹配。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 校验状态推进；关闭后不可恢复，暂停可恢复为活动。
    pub const fn validate_status_transition(
        current: OrganizationCreditTermStatus,
        next: OrganizationCreditTermStatus,
    ) -> Result<(), OrganizationCreditTermsError> {
        let valid = match current {
            OrganizationCreditTermStatus::Active => matches!(
                next,
                OrganizationCreditTermStatus::Active
                    | OrganizationCreditTermStatus::Paused
                    | OrganizationCreditTermStatus::Closed
            ),
            OrganizationCreditTermStatus::Paused => matches!(
                next,
                OrganizationCreditTermStatus::Active
                    | OrganizationCreditTermStatus::Paused
                    | OrganizationCreditTermStatus::Closed
            ),
            OrganizationCreditTermStatus::Closed => {
                matches!(next, OrganizationCreditTermStatus::Closed)
            }
        };
        if valid {
            Ok(())
        } else {
            Err(OrganizationCreditTermsError::InvalidTransition)
        }
    }

    #[must_use]
    pub fn currency(&self) -> &OrganizationCreditCurrency {
        &self.currency
    }

    #[must_use]
    pub const fn credit_limit(&self) -> OrganizationCreditLimit {
        self.credit_limit
    }

    #[must_use]
    pub const fn period(&self) -> OrganizationCreditTermPeriod {
        self.period
    }

    #[must_use]
    pub const fn status(&self) -> OrganizationCreditTermStatus {
        self.status
    }

    #[must_use]
    pub const fn effective_from(&self) -> i64 {
        self.effective_from
    }

    #[must_use]
    pub const fn effective_until(&self) -> Option<i64> {
        self.effective_until
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(status: OrganizationCreditTermStatus) -> OrganizationCreditTermsSnapshot {
        OrganizationCreditTermsSnapshot::new(
            OrganizationId::new(1).expect("企业标识有效"),
            OrganizationCreditCurrency::new("USD").expect("币种有效"),
            OrganizationCreditLimit::new(10_000).expect("额度有效"),
            OrganizationCreditTermPeriod::net_days(30).expect("账期有效"),
            status,
            100,
            Some(200),
            1,
        )
        .expect("策略有效")
    }

    #[test]
    fn 授信边界拒绝负数和非法账期() {
        assert_eq!(
            OrganizationCreditLimit::new(-1),
            Err(OrganizationCreditTermsError::NegativeCreditLimit)
        );
        assert_eq!(
            OrganizationCreditTermPeriod::net_days(0),
            Err(OrganizationCreditTermsError::InvalidPeriod)
        );
        assert_eq!(
            OrganizationCreditCurrency::new("usd"),
            Err(OrganizationCreditTermsError::InvalidCurrency)
        );
    }

    #[test]
    fn 生效区间和状态终态保持关闭() {
        assert_eq!(
            OrganizationCreditTermsSnapshot::new(
                OrganizationId::new(1).expect("企业标识有效"),
                OrganizationCreditCurrency::new("USD").expect("币种有效"),
                OrganizationCreditLimit::new(1).expect("额度有效"),
                OrganizationCreditTermPeriod::EndOfMonth,
                OrganizationCreditTermStatus::Active,
                10,
                Some(10),
                1,
            ),
            Err(OrganizationCreditTermsError::InvalidEffectiveRange)
        );
        assert_eq!(
            OrganizationCreditTermsSnapshot::validate_status_transition(
                OrganizationCreditTermStatus::Closed,
                OrganizationCreditTermStatus::Active,
            ),
            Err(OrganizationCreditTermsError::InvalidTransition)
        );
        assert_eq!(
            snapshot(OrganizationCreditTermStatus::Paused).status(),
            OrganizationCreditTermStatus::Paused
        );
    }
}
