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
    /// 人民币整数分按每元额度换算为内部额度。
    CnyMinorConversion,
    /// 按 token 数和每百万 token 单价换算额度。
    TokenConversion,
    /// 按多个 token 分项和倍率解析最终定价。
    PricingResolution,
    /// 非负额度与业务因子相乘。
    Multiplication,
}

impl QuotaMathOperation {
    /// 返回 usage 审计记录使用的稳定名称。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UsdConversion => "quota_from_usd",
            Self::CnyMinorConversion => "quota_from_cny_minor",
            Self::TokenConversion => "quota_from_tokens",
            Self::PricingResolution => "pricing_resolution",
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

/// 用户可控计费乘数的业务上限校验错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum QuotaFactorError {
    /// 调用方提供的业务上限为负数。
    #[error("计费乘数上限无效")]
    InvalidLimit,
    /// 用户乘数为负数或超过业务上限。
    #[error("计费乘数超出允许范围")]
    FactorOutOfRange,
    /// 通过上限校验后的额度乘法越界。
    #[error(transparent)]
    Math(#[from] QuotaMathError),
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

/// 将人民币整数分按每元额度换算为内部额度。
///
/// 公式为 `amount_minor * quota_per_cny / 100`，只使用 checked 整数运算；结果按中点远离零
/// 舍入。负的每元额度违反非负额度约束，任何中间值或最终 `i64` 越界都返回可审计的溢出错误。
pub fn quota_from_cny_minor(
    amount_minor: u64,
    quota_per_cny: i64,
) -> Result<Quota, QuotaMathError> {
    if quota_per_cny < 0 {
        return Err(QuotaError::Negative.into());
    }

    let operation = QuotaMathOperation::CnyMinorConversion;
    let weighted = i128::from(amount_minor)
        .checked_mul(i128::from(quota_per_cny))
        .ok_or_else(|| QuotaClamp::overflow(operation))?;
    // 输入均非负，中点远离零等价于除法前增加半个分母。
    let rounded = weighted
        .checked_add(50)
        .ok_or_else(|| QuotaClamp::overflow(operation))?
        / 100;
    let units = i64::try_from(rounded).map_err(|_| QuotaClamp::overflow(operation))?;
    Quota::new(units).map_err(Into::into)
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

/// Ratio 模式固定的五类 token 分项计算结果。
pub(crate) struct TokenPricingResult {
    pub(crate) quota: Quota,
    pub(crate) component_usd: [Decimal; 5],
    pub(crate) total_usd: Decimal,
}

/// 表达式定价完成倍率换算后的内部结果。
pub(crate) struct ExpressionPricingResult {
    pub(crate) quota: Quota,
    pub(crate) base_usd: Decimal,
    pub(crate) total_usd: Decimal,
}

/// 把表达式返回的 token 加权美元单价换算为最终额度。
///
/// 表达式系数单位为美元/百万 token，因此先应用请求倍率，再统一除以一百万；最终额度
/// 仍只在 [`quota_from_usd_for`] 中舍入一次。
pub(crate) fn quota_from_expression_cost(
    weighted_cost: Decimal,
    ratio_micros: [i64; 3],
) -> Result<ExpressionPricingResult, QuotaMathError> {
    if weighted_cost < Decimal::ZERO || ratio_micros.into_iter().any(|ratio| ratio < 0) {
        return Err(QuotaError::Negative.into());
    }

    let base_usd = weighted_cost
        .checked_div(Decimal::from(TOKENS_PER_MILLION))
        .ok_or_else(|| QuotaMathError::Clamp(pricing_overflow()))?;
    let total_usd = apply_pricing_ratios(weighted_cost, ratio_micros)?;
    let quota = quota_from_usd_for(total_usd, QuotaMathOperation::PricingResolution)?;
    Ok(ExpressionPricingResult {
        quota,
        base_usd,
        total_usd,
    })
}

/// 按真实美元单价和百万分比倍率计算五类 token 的最终额度。
///
/// 分项 Decimal 成本全部完成 checked 运算后才汇总，并且只在最终总金额处执行一次
/// quota 整数舍入。该入口只服务定价解析器，不暴露配置或持久化语义。
pub(crate) fn quota_from_token_components(
    components: [(i64, Decimal); 5],
    ratio_micros: [i64; 3],
) -> Result<TokenPricingResult, QuotaMathError> {
    if ratio_micros.into_iter().any(|ratio| ratio < 0)
        || components
            .into_iter()
            .any(|(tokens, price)| tokens < 0 || price < Decimal::ZERO)
    {
        return Err(QuotaError::Negative.into());
    }

    if ratio_micros.into_iter().any(|ratio| ratio == 0) {
        return Ok(TokenPricingResult {
            quota: Quota::ZERO,
            component_usd: [Decimal::ZERO; 5],
            total_usd: Decimal::ZERO,
        });
    }

    let mut weighted_components = [Decimal::ZERO; 5];
    let mut weighted_total = Decimal::ZERO;
    for (index, (tokens, price)) in components.into_iter().enumerate() {
        let weighted = Decimal::from(tokens)
            .checked_mul(price)
            .ok_or_else(pricing_overflow)?;
        weighted_total = weighted_total
            .checked_add(weighted)
            .ok_or_else(pricing_overflow)?;
        weighted_components[index] = weighted;
    }

    // 倍率可能把低于 Decimal 最小精度的单价重新放大，必须推迟百万 token 除法。
    let total_usd = apply_pricing_ratios(weighted_total, ratio_micros)?;
    let mut component_usd = [Decimal::ZERO; 5];
    for (index, weighted) in weighted_components.into_iter().enumerate() {
        component_usd[index] = apply_pricing_ratios(weighted, ratio_micros)?;
    }

    let quota = quota_from_usd_for(total_usd, QuotaMathOperation::PricingResolution)?;
    Ok(TokenPricingResult {
        quota,
        component_usd,
        total_usd,
    })
}

/// 将五类美元/百万 token 基础单价应用与请求结算相同的三层倍率。
///
/// 本入口只返回精确 Decimal 展示价，不执行 quota 整数舍入；实际请求仍在五类成本
/// 汇总后只舍入一次。倍率为零时保留按 token 模式并返回五项零价。
pub(crate) fn token_prices_after_ratios(
    prices: [Decimal; 5],
    ratio_micros: [i64; 3],
) -> Result<[Decimal; 5], QuotaMathError> {
    if ratio_micros.into_iter().any(|ratio| ratio < 0)
        || prices.into_iter().any(|price| price < Decimal::ZERO)
    {
        return Err(QuotaError::Negative.into());
    }
    if ratio_micros.into_iter().any(|ratio| ratio == 0) {
        return Ok([Decimal::ZERO; 5]);
    }

    let mut effective = [Decimal::ZERO; 5];
    for (index, price) in prices.into_iter().enumerate() {
        let weighted = Decimal::from(TOKENS_PER_MILLION)
            .checked_mul(price)
            .ok_or_else(pricing_overflow)?;
        effective[index] = apply_pricing_ratios(weighted, ratio_micros)?;
    }
    Ok(effective)
}

fn apply_pricing_ratios(
    weighted_cost: Decimal,
    ratio_micros: [i64; 3],
) -> Result<Decimal, QuotaMathError> {
    let mut cost = weighted_cost;
    for ratio in ratio_micros {
        cost = cost
            .checked_mul(Decimal::new(ratio, 6))
            .ok_or_else(pricing_overflow)?;
    }

    cost.checked_div(Decimal::from(TOKENS_PER_MILLION))
        .ok_or_else(|| pricing_overflow().into())
}

/// 把已经过业务边界校验的美元金额应用三层请求倍率并转换为额度。
pub(crate) fn quota_from_usd_with_ratios(
    usd: Decimal,
    ratio_micros: [i64; 3],
) -> Result<Quota, QuotaMathError> {
    if usd < Decimal::ZERO || ratio_micros.into_iter().any(|ratio| ratio < 0) {
        return Err(QuotaError::Negative.into());
    }
    // 公共倍率函数会在末尾消除一次百万尺度；美元金额必须先补齐同一固定点尺度。
    let weighted = Decimal::from(TOKENS_PER_MILLION)
        .checked_mul(usd)
        .ok_or_else(pricing_overflow)?;
    let effective = apply_pricing_ratios(weighted, ratio_micros)?;
    quota_from_usd_for(effective, QuotaMathOperation::PricingResolution)
}

const fn pricing_overflow() -> QuotaClamp {
    QuotaClamp::overflow(QuotaMathOperation::PricingResolution)
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

/// 校验用户可控乘数的业务上限后再执行 checked 额度乘法。
///
/// PerCall 图片张数、视频时长、分辨率等外部输入必须调用本入口，禁止先乘后截断。
/// 上限由对应业务协议固定提供，不得来自同一个用户请求。
pub fn checked_mul_quota_with_limit(
    base: Quota,
    factor: i64,
    maximum_factor: i64,
) -> Result<Quota, QuotaFactorError> {
    if maximum_factor < 0 {
        return Err(QuotaFactorError::InvalidLimit);
    }
    if factor < 0 || factor > maximum_factor {
        return Err(QuotaFactorError::FactorOutOfRange);
    }
    checked_mul_quota(base, factor).map_err(Into::into)
}
