use std::fmt;

use af_domain::Quota;
use af_protocol::{Usage, UsageError, UsageSemantics};
use rust_decimal::Decimal;
use thiserror::Error;

use crate::{
    BillingExpressionError,
    quota_math::{self, QuotaMathError},
};

/// 定价解析器的最小只读上下文。
///
/// 当前 Ratio 模式只依赖规范化用量。模型价格、分组倍率和已生效的高峰倍率由调用方
/// 先解析为不可变配置快照，再构造对应解析器；本类型不访问数据库、缓存或时钟。
pub struct PricingContext<'a> {
    usage: &'a Usage,
}

impl<'a> PricingContext<'a> {
    /// 从一次已规范化的 token 用量构造定价上下文。
    #[must_use]
    pub const fn new(usage: &'a Usage) -> Self {
        Self { usage }
    }

    /// 返回本次定价使用的规范化 token 用量。
    #[must_use]
    pub const fn usage(&self) -> &'a Usage {
        self.usage
    }
}

impl fmt::Debug for PricingContext<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PricingContext(<redacted>)")
    }
}

/// 把不可变定价上下文解析为额度及审计分解的同步策略。
pub trait PricingResolver: Send + Sync + 'static {
    /// 解析一次调用的最终定价，不执行预扣、结算、退款或任何 IO。
    fn resolve(&self, context: &PricingContext<'_>) -> Result<PriceData, PricingError>;
}

/// 当前已经实现的计费模式。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BillingMode {
    /// 按互斥 token 分项及其每百万 token 单价计费。
    PerToken,
    /// 按请求次数或已校验业务维度直接计费。
    PerCall,
    /// 显式免费；上层仍须记录 usage，但跳过预扣和结算。
    Free,
}

/// 五类互斥 token 分项的真实美元单价，单位均为美元/百万 token。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct TokenPrices {
    input: Decimal,
    output: Decimal,
    cache_read: Decimal,
    cache_creation_5m: Decimal,
    cache_creation_1h: Decimal,
}

impl TokenPrices {
    /// 校验并构造五类非负 token 单价快照。
    pub fn new(
        input: Decimal,
        output: Decimal,
        cache_read: Decimal,
        cache_creation_5m: Decimal,
        cache_creation_1h: Decimal,
    ) -> Result<Self, PricingError> {
        if [
            input,
            output,
            cache_read,
            cache_creation_5m,
            cache_creation_1h,
        ]
        .into_iter()
        .any(|price| price < Decimal::ZERO)
        {
            return Err(PricingError::InvalidPrice);
        }

        Ok(Self {
            input,
            output,
            cache_read,
            cache_creation_5m,
            cache_creation_1h,
        })
    }

    /// 返回普通输入单价。
    #[must_use]
    pub const fn input(&self) -> Decimal {
        self.input
    }

    /// 返回输出单价。
    #[must_use]
    pub const fn output(&self) -> Decimal {
        self.output
    }

    /// 返回缓存读取单价。
    #[must_use]
    pub const fn cache_read(&self) -> Decimal {
        self.cache_read
    }

    /// 返回五分钟缓存创建单价。
    #[must_use]
    pub const fn cache_creation_5m(&self) -> Decimal {
        self.cache_creation_5m
    }

    /// 返回一小时缓存创建单价。
    #[must_use]
    pub const fn cache_creation_1h(&self) -> Decimal {
        self.cache_creation_1h
    }
}

impl fmt::Debug for TokenPrices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TokenPrices(<redacted>)")
    }
}

/// 应用请求三层倍率后的五类精确美元/百万 token 单价。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct EffectiveTokenPrices {
    values: [Decimal; 5],
}

impl EffectiveTokenPrices {
    const fn from_values(values: [Decimal; 5]) -> Self {
        Self { values }
    }

    /// 返回普通输入单价。
    #[must_use]
    pub const fn input(self) -> Decimal {
        self.values[0]
    }

    /// 返回输出单价。
    #[must_use]
    pub const fn output(self) -> Decimal {
        self.values[1]
    }

    /// 返回缓存读取单价。
    #[must_use]
    pub const fn cache_read(self) -> Decimal {
        self.values[2]
    }

    /// 返回五分钟缓存创建单价。
    #[must_use]
    pub const fn cache_creation_5m(self) -> Decimal {
        self.values[3]
    }

    /// 返回一小时缓存创建单价。
    #[must_use]
    pub const fn cache_creation_1h(self) -> Decimal {
        self.values[4]
    }

    /// 按固定顺序返回五类单价，供目录边界做精确字符串序列化。
    #[must_use]
    pub const fn into_values(self) -> [Decimal; 5] {
        self.values
    }
}

impl fmt::Debug for EffectiveTokenPrices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EffectiveTokenPrices(<已脱敏>)")
    }
}

/// 百万分比定点倍率，`1_000_000` 表示 `1.0`。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PricingRatio(i64);

impl PricingRatio {
    /// 显式零倍率；若业务语义为免费，调用方仍应构造 Free 解析器。
    pub const ZERO: Self = Self(0);
    /// 不改变定价的一倍倍率。
    pub const ONE: Self = Self(1_000_000);

    /// 校验并构造非负百万分比倍率。
    pub const fn new(micros: i64) -> Result<Self, PricingError> {
        if micros < 0 {
            Err(PricingError::InvalidRatio)
        } else {
            Ok(Self(micros))
        }
    }

    /// 返回持久化与配置边界使用的百万分比整数值。
    #[must_use]
    pub const fn micros(self) -> i64 {
        self.0
    }
}

impl fmt::Debug for PricingRatio {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PricingRatio(<redacted>)")
    }
}

/// Ratio 模式一次解析已经确定生效的三层倍率快照。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PricingRatios {
    group: PricingRatio,
    group_model: PricingRatio,
    applied_peak: PricingRatio,
}

impl PricingRatios {
    /// 构造分组、分组模型和当前已生效高峰倍率快照。
    #[must_use]
    pub const fn new(
        group: PricingRatio,
        group_model: PricingRatio,
        applied_peak: PricingRatio,
    ) -> Self {
        Self {
            group,
            group_model,
            applied_peak,
        }
    }

    /// 返回分组基础倍率。
    #[must_use]
    pub const fn group(&self) -> PricingRatio {
        self.group
    }

    /// 返回用户分组与计费分组的模型倍率。
    #[must_use]
    pub const fn group_model(&self) -> PricingRatio {
        self.group_model
    }

    /// 返回调用方根据时间窗口预先确定的高峰倍率。
    #[must_use]
    pub const fn applied_peak(&self) -> PricingRatio {
        self.applied_peak
    }

    pub(crate) const fn micros(self) -> [i64; 3] {
        [
            self.group.micros(),
            self.group_model.micros(),
            self.applied_peak.micros(),
        ]
    }
}

impl fmt::Debug for PricingRatios {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PricingRatios(<redacted>)")
    }
}

/// 应用全部倍率后的五类精确美元成本分解。
///
/// 分项不单独转换为 quota，避免重复整数舍入；[`Self::total_usd`] 与最终 quota
/// 使用同一组 checked Decimal 中间结果。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct CostBreakdown {
    input_usd: Decimal,
    output_usd: Decimal,
    cache_read_usd: Decimal,
    cache_creation_5m_usd: Decimal,
    cache_creation_1h_usd: Decimal,
    total_usd: Decimal,
}

impl CostBreakdown {
    const ZERO: Self = Self {
        input_usd: Decimal::ZERO,
        output_usd: Decimal::ZERO,
        cache_read_usd: Decimal::ZERO,
        cache_creation_5m_usd: Decimal::ZERO,
        cache_creation_1h_usd: Decimal::ZERO,
        total_usd: Decimal::ZERO,
    };

    const fn from_components(component_usd: [Decimal; 5], total_usd: Decimal) -> Self {
        let [
            input_usd,
            output_usd,
            cache_read_usd,
            cache_creation_5m_usd,
            cache_creation_1h_usd,
        ] = component_usd;
        Self {
            input_usd,
            output_usd,
            cache_read_usd,
            cache_creation_5m_usd,
            cache_creation_1h_usd,
            total_usd,
        }
    }

    /// 表达式只能可靠提供聚合成本，不能伪造五类固定单价分解。
    const fn from_expression_total(total_usd: Decimal) -> Self {
        Self {
            total_usd,
            ..Self::ZERO
        }
    }

    /// 返回应用倍率后的普通输入美元成本。
    #[must_use]
    pub const fn input_usd(&self) -> Decimal {
        self.input_usd
    }

    /// 返回应用倍率后的输出美元成本。
    #[must_use]
    pub const fn output_usd(&self) -> Decimal {
        self.output_usd
    }

    /// 返回应用倍率后的缓存读取美元成本。
    #[must_use]
    pub const fn cache_read_usd(&self) -> Decimal {
        self.cache_read_usd
    }

    /// 返回应用倍率后的五分钟缓存创建美元成本。
    #[must_use]
    pub const fn cache_creation_5m_usd(&self) -> Decimal {
        self.cache_creation_5m_usd
    }

    /// 返回应用倍率后的一小时缓存创建美元成本。
    #[must_use]
    pub const fn cache_creation_1h_usd(&self) -> Decimal {
        self.cache_creation_1h_usd
    }

    /// 返回五类分项的 checked 美元成本总和。
    #[must_use]
    pub const fn total_usd(&self) -> Decimal {
        self.total_usd
    }
}

impl fmt::Debug for CostBreakdown {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CostBreakdown(<redacted>)")
    }
}

/// 一次定价解析的不可变结果。
#[must_use = "定价结果必须用于预扣、结算或显式免费判断"]
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PriceData {
    quota: Quota,
    billing_mode: BillingMode,
    breakdown: CostBreakdown,
}

impl PriceData {
    const fn free() -> Self {
        Self {
            quota: Quota::ZERO,
            billing_mode: BillingMode::Free,
            breakdown: CostBreakdown::ZERO,
        }
    }

    const fn metered(quota: Quota, breakdown: CostBreakdown) -> Self {
        Self {
            quota,
            billing_mode: BillingMode::PerToken,
            breakdown,
        }
    }

    /// 构造表达式解析得到的按量结果；生命周期仍执行预扣和结算。
    pub(crate) const fn expression(quota: Quota, total_usd: Decimal) -> Self {
        Self {
            quota,
            billing_mode: BillingMode::PerToken,
            breakdown: CostBreakdown::from_expression_total(total_usd),
        }
    }

    /// 返回最终一次舍入后的非负额度。
    #[must_use]
    pub const fn quota(&self) -> Quota {
        self.quota
    }

    /// 返回本次解析采用的计费模式。
    #[must_use]
    pub const fn billing_mode(&self) -> BillingMode {
        self.billing_mode
    }

    /// 返回应用倍率后的精确美元成本分解。
    #[must_use]
    pub const fn breakdown(&self) -> &CostBreakdown {
        &self.breakdown
    }

    /// 返回上层是否应跳过预扣与结算。
    #[must_use]
    pub const fn is_free(&self) -> bool {
        matches!(self.billing_mode, BillingMode::Free)
    }
}

impl fmt::Debug for PriceData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PriceData")
            .field("billing_mode", &self.billing_mode)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RatioMode {
    Free,
    Metered {
        prices: TokenPrices,
        ratios: PricingRatios,
    },
}

/// 使用真实美元/百万 token 单价和定点倍率的纯计算解析器。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct RatioPricingResolver {
    mode: RatioMode,
}

impl RatioPricingResolver {
    /// 构造显式免费解析器；非零 usage 仍返回 Free 且零额度。
    #[must_use]
    pub const fn free() -> Self {
        Self {
            mode: RatioMode::Free,
        }
    }

    /// 从已经校验的单价与倍率快照构造按 token 计量的解析器。
    #[must_use]
    pub const fn metered(prices: TokenPrices, ratios: PricingRatios) -> Self {
        Self {
            mode: RatioMode::Metered { prices, ratios },
        }
    }

    /// 返回目录展示使用的倍率后单价；显式免费与按 token 零价保持不同语义。
    pub fn effective_token_prices(&self) -> Result<Option<EffectiveTokenPrices>, PricingError> {
        let RatioMode::Metered { prices, ratios } = self.mode else {
            return Ok(None);
        };
        let values = quota_math::token_prices_after_ratios(
            [
                prices.input(),
                prices.output(),
                prices.cache_read(),
                prices.cache_creation_5m(),
                prices.cache_creation_1h(),
            ],
            ratios.micros(),
        )?;
        Ok(Some(EffectiveTokenPrices::from_values(values)))
    }
}

impl PricingResolver for RatioPricingResolver {
    fn resolve(&self, context: &PricingContext<'_>) -> Result<PriceData, PricingError> {
        let RatioMode::Metered { prices, ratios } = self.mode else {
            return Ok(PriceData::free());
        };

        let usage = context.usage();
        let details = usage.details();
        let components = [
            (uncached_input_tokens(usage)?, prices.input()),
            (usage.output_tokens().get(), prices.output()),
            (details.cache_read().get(), prices.cache_read()),
            (
                details.cache_creation_5m().get(),
                prices.cache_creation_5m(),
            ),
            (
                details.cache_creation_1h().get(),
                prices.cache_creation_1h(),
            ),
        ];
        let resolved = quota_math::quota_from_token_components(components, ratios.micros())?;
        let breakdown = CostBreakdown::from_components(resolved.component_usd, resolved.total_usd);

        Ok(PriceData::metered(resolved.quota, breakdown))
    }
}

impl fmt::Debug for RatioPricingResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match self.mode {
            RatioMode::Free => BillingMode::Free,
            RatioMode::Metered { .. } => BillingMode::PerToken,
        };
        formatter
            .debug_struct("RatioPricingResolver")
            .field("billing_mode", &mode)
            .finish_non_exhaustive()
    }
}

fn uncached_input_tokens(usage: &Usage) -> Result<i64, UsageError> {
    if usage.semantics() == UsageSemantics::CacheSeparated {
        return Ok(usage.input_tokens().get());
    }

    let details = usage.details();
    [
        details.cache_read(),
        details.cache_creation_5m(),
        details.cache_creation_1h(),
    ]
    .into_iter()
    .try_fold(usage.input_tokens().get(), |remaining, tokens| {
        remaining
            .checked_sub(tokens.get())
            .ok_or(UsageError::InputDetailsExceedTotal)
    })
}

/// 定价配置或解析错误；不保留模型名、token 数、价格、倍率或额度。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PricingError {
    /// 至少一个美元/百万 token 单价为负数。
    #[error("token 单价不能为负数")]
    InvalidPrice,
    /// 百万分比倍率为负数。
    #[error("计费倍率不能为负数")]
    InvalidRatio,
    /// 规范化用量在 checked 运算中违反边界。
    #[error(transparent)]
    Usage(#[from] UsageError),
    /// Decimal 中间运算或最终 quota 换算越界。
    #[error(transparent)]
    Math(#[from] QuotaMathError),
    /// 表达式在受限执行环境中无法生成合法价格。
    #[error(transparent)]
    Expression(#[from] BillingExpressionError),
}
