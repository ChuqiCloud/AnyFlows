use std::fmt;

use thiserror::Error;

/// 非负的内部额度单位。
///
/// 该类型不实现普通算术运算符、`Default`、隐式整数转换或 Serde。
/// 调用方必须显式选择 checked 运算；协议边界应单独决定超过 JavaScript
/// 安全整数范围时的编码方式。
/// 无限额度必须由独立布尔值或 Option 表达，禁止使用负数或最大值作为哨兵；
/// 未知或可能为负的上游余额也不得强行转换为该类型。
///
/// ```compile_fail
/// use af_domain::Quota;
///
/// let _ = Quota(-1);
/// ```
///
/// ```compile_fail
/// use af_domain::Quota;
///
/// let _ = Quota::ZERO + Quota::ZERO;
/// ```
///
/// ```compile_fail
/// use af_domain::Quota;
///
/// let _ = Quota::ZERO * 2_i64;
/// ```
///
/// ```compile_fail
/// use af_domain::Quota;
///
/// let _: i64 = Quota::ZERO.into();
/// ```
///
/// ```compile_fail
/// use af_domain::Quota;
///
/// let _ = Quota::default();
/// ```
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Quota(i64);

impl Quota {
    /// 显式的零额度；不表示无限额度或免费计费结果。
    pub const ZERO: Self = Self(0);

    /// 校验并构造非负额度。
    pub const fn new(units: i64) -> Result<Self, QuotaError> {
        if units < 0 {
            Err(QuotaError::Negative)
        } else {
            Ok(Self(units))
        }
    }

    /// 返回底层整数单位；仅供持久化、协议编码或已审计算法边界使用。
    #[must_use]
    pub const fn units(self) -> i64 {
        self.0
    }

    /// 返回额度是否为零。
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// 使用 checked 加法合并两个非负额度。
    pub const fn checked_add(self, rhs: Self) -> Result<Self, QuotaError> {
        match self.0.checked_add(rhs.0) {
            Some(units) => Ok(Self(units)),
            None => Err(QuotaError::Overflow),
        }
    }

    /// 使用 checked 减法扣减额度，结果不得小于零。
    pub const fn checked_sub(self, rhs: Self) -> Result<Self, QuotaError> {
        if self.0 < rhs.0 {
            Err(QuotaError::Underflow)
        } else {
            Ok(Self(self.0 - rhs.0))
        }
    }

    /// 应用有符号额度调整量，拒绝溢出或负结果。
    pub const fn checked_apply(self, delta: QuotaDelta) -> Result<Self, QuotaError> {
        match self.0.checked_add(delta.0) {
            Some(units) if units >= 0 => Ok(Self(units)),
            Some(_) => Err(QuotaError::Underflow),
            None => Err(QuotaError::Overflow),
        }
    }

    /// 返回相对基准额度的有符号差额，即 `self - baseline`。
    #[must_use]
    pub const fn delta_from(self, baseline: Self) -> QuotaDelta {
        // 两个操作数均非负，差值绝不会达到 i64::MIN，因此分支内运算安全。
        if self.0 >= baseline.0 {
            QuotaDelta(self.0 - baseline.0)
        } else {
            QuotaDelta(-(baseline.0 - self.0))
        }
    }
}

impl TryFrom<i64> for Quota {
    type Error = QuotaError;

    fn try_from(units: i64) -> Result<Self, Self::Error> {
        Self::new(units)
    }
}

impl fmt::Debug for Quota {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Quota(<redacted>)")
    }
}

/// 有符号额度调整量；正数表示补扣，负数表示退还。
///
/// 调整量只描述数值方向，不决定预扣、结算、退款或持久化策略。
/// 有效范围是 `-i64::MAX..=i64::MAX`，与两个合法 Quota 的差值范围一致。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct QuotaDelta(i64);

impl QuotaDelta {
    /// 显式的零调整量。
    pub const ZERO: Self = Self(0);

    /// 校验并构造有符号额度调整量。
    pub const fn new(units: i64) -> Result<Self, QuotaError> {
        if units == i64::MIN {
            Err(QuotaError::InvalidDelta)
        } else {
            Ok(Self(units))
        }
    }

    /// 返回底层整数单位；仅供持久化或已审计算法边界使用。
    #[must_use]
    pub const fn units(self) -> i64 {
        self.0
    }

    /// 返回调整量是否为零。
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// 返回调整量是否为正。
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    /// 返回调整量是否为负。
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }
}

impl fmt::Debug for QuotaDelta {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("QuotaDelta(<redacted>)")
    }
}

/// 额度构造与 checked 运算错误；不保留任何操作数。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum QuotaError {
    /// 外部值为负数，不能表示非负额度。
    #[error("额度不能为负数")]
    Negative,
    /// checked 运算超过 i64 上界。
    #[error("额度运算溢出")]
    Overflow,
    /// 扣减或调整后的结果小于零。
    #[error("额度扣减结果不能为负数")]
    Underflow,
    /// 调整量超出两个合法额度之间可表示的差值范围。
    #[error("额度调整量超出有效范围")]
    InvalidDelta,
}
