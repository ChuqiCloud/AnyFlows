//! 额度换算的唯一数学入口。
//!
//! 本模块只负责确定性的 checked 运算，不执行审计、持久化或余额判断。

use af_domain::{Quota, QuotaError};
use rust_decimal::{Decimal, RoundingStrategy};
use thiserror::Error;

/// 每美元对应的内部额度单位数。
pub const QUOTA_PER_USD: i64 = 500_000;

const TOKENS_PER_MILLION: i64 = 1_000_000;

/// 额度运算的稳定审计入口名称。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaMathOperation {
    /// 美元金额换算为内部额度。
    UsdConversion,
    /// 按 token 数和每百万 token 单价换算额度。
    TokenConversion,
    /// 非负额度与业务因子相乘。
    Multiplication,
}

impl QuotaMathOperation {
    /// 返回 usage 审计记录使用的稳定名称。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UsdConversion => "quota_from_usd",
            Self::TokenConversion => "quota_from_tokens",
            Self::Multiplication => "checked_mul_quota",
        }
    }
}

/// 额度结果被拒绝的稳定边界分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaClampKind {
    /// 结果超过 `Quota` 可表示的 `i64` 上界。
    Overflow,
}

impl QuotaClampKind {
    /// 返回 usage 审计记录使用的稳定分类。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Overflow => "overflow",
        }
    }
}

/// 一次额度越界的结构化审计标记。
///
/// AnyFlows 不返回饱和值，而是拒绝本次计费结果；调用方仍可把该标记写入
/// `usage_log.other.quota_saturation`。标记只包含闭合分类，不保存原始金额或乘数。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("额度运算超出可表示范围")]
pub struct QuotaClamp {
    operation: QuotaMathOperation,
    kind: QuotaClampKind,
}

impl QuotaClamp {
    const fn overflow(operation: QuotaMathOperation) -> Self {
        Self {
            operation,
            kind: QuotaClampKind::Overflow,
        }
    }

    /// 返回发生越界的额度运算入口。
    #[must_use]
    pub const fn operation(self) -> QuotaMathOperation {
        self.operation
    }

    /// 返回稳定越界分类。
    #[must_use]
    pub const fn kind(self) -> QuotaClampKind {
        self.kind
    }
}

/// 集中额度运算错误，并保留可选的结构化审计标记。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum QuotaMathError {
    /// 输入违反非负额度值对象约束。
    #[error(transparent)]
    Quota(#[from] QuotaError),
    /// 运算越界，结果已被拒绝且必须进入管理员审计。
    #[error(transparent)]
    Clamp(#[from] QuotaClamp),
}

impl QuotaMathError {
    /// 返回统一领域额度错误，供上层稳定分类。
    #[must_use]
    pub const fn quota_error(self) -> QuotaError {
        match self {
            Self::Quota(error) => error,
            Self::Clamp(_) => QuotaError::Overflow,
        }
    }

    /// 返回需要写入 usage 管理员字段的越界审计标记。
    #[must_use]
    pub const fn clamp(self) -> Option<QuotaClamp> {
        match self {
            Self::Quota(_) => None,
            Self::Clamp(clamp) => Some(clamp),
        }
    }
}

/// 将美元金额换算为内部额度。
///
/// 换算结果按中点远离零舍入到整数。负金额包装为 [`QuotaError::Negative`]，
/// Decimal 中间运算或最终 `i64` 转换越界返回带审计标记的 [`QuotaMathError::Clamp`]。
pub fn quota_from_usd(usd: Decimal) -> Result<Quota, QuotaMathError> {
    quota_from_usd_for(usd, QuotaMathOperation::UsdConversion)
}

fn quota_from_usd_for(
    usd: Decimal,
    operation: QuotaMathOperation,
) -> Result<Quota, QuotaMathError> {
    if usd < Decimal::ZERO {
        return Err(QuotaError::Negative.into());
    }

    let quota = usd
        .checked_mul(Decimal::from(QUOTA_PER_USD))
        .ok_or_else(|| QuotaClamp::overflow(operation))?;
    quota_from_decimal(quota, operation)
}

fn quota_from_decimal(
    quota: Decimal,
    operation: QuotaMathOperation,
) -> Result<Quota, QuotaMathError> {
    let quota = quota.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
    let units = i64::try_from(quota).map_err(|_| QuotaClamp::overflow(operation))?;

    Quota::new(units).map_err(Into::into)
}

/// 按每百万 token 的美元单价换算 token 消耗额度。
///
/// token 数或单价为负时返回 [`QuotaError::Negative`]；乘法与后续美元换算
/// 均使用 checked 运算。调用方仍须在进入本函数前限制用户可控的 token 上限。
pub fn quota_from_tokens(tokens: i64, price_per_mtok: Decimal) -> Result<Quota, QuotaMathError> {
    if tokens < 0 || price_per_mtok < Decimal::ZERO {
        return Err(QuotaError::Negative.into());
    }

    // 先进入额度尺度再除以百万，避免 Decimal 精度边界上的中间除法产生二次舍入。
    let quota = Decimal::from(tokens)
        .checked_mul(price_per_mtok)
        .ok_or_else(|| QuotaClamp::overflow(QuotaMathOperation::TokenConversion))?
        .checked_mul(Decimal::from(QUOTA_PER_USD))
        .ok_or_else(|| QuotaClamp::overflow(QuotaMathOperation::TokenConversion))?
        .checked_div(Decimal::from(TOKENS_PER_MILLION))
        .ok_or_else(|| QuotaClamp::overflow(QuotaMathOperation::TokenConversion))?;

    quota_from_decimal(quota, QuotaMathOperation::TokenConversion)
}

/// 使用非负整数因子放大额度。
///
/// 负因子包装为 [`QuotaError::Negative`]，乘法越过 `i64` 上界返回带审计标记的
/// [`QuotaMathError::Clamp`]。该函数不替代业务上限校验，用户可控因子必须先由
/// 调用方限制到对应业务允许的最大值。
pub fn checked_mul_quota(base: Quota, factor: i64) -> Result<Quota, QuotaMathError> {
    if factor < 0 {
        return Err(QuotaError::Negative.into());
    }

    let units = base
        .units()
        .checked_mul(factor)
        .ok_or_else(|| QuotaClamp::overflow(QuotaMathOperation::Multiplication))?;

    Quota::new(units).map_err(Into::into)
}
