//! 表达式计费的纯计算内核。
//!
//! 本模块只编译和执行不可变计费表达式，不访问请求正文、Header、数据库、缓存、时钟或
//! 网络。生产定价快照与持久化接线由后续切片完成。

use std::{
    fmt,
    sync::{Arc, Mutex},
};

use af_domain::Quota;
use af_protocol::{TokenCount, Usage, UsageDetails, UsageError, UsageSemantics, UsageSource};
use rhai::{
    AST, ASTNode, Engine, EvalAltResult, Expr, ImmutableString, OptimizationLevel, Position, Scope,
};
use rust_decimal::Decimal;
use thiserror::Error;

use crate::{
    PriceData, PricingContext, PricingError, PricingRatio, PricingRatios, PricingResolver,
    quota_math::{self, QuotaMathError},
};

/// 单条计费表达式允许的最大 UTF-8 字节数。
pub const MAX_BILLING_EXPRESSION_BYTES: usize = 8 * 1024;

const MAX_OPERATIONS: u64 = 2_048;
const MAX_CALL_LEVELS: usize = 8;
const MAX_EXPR_DEPTH: usize = 32;
const MAX_FUNCTION_EXPR_DEPTH: usize = 16;
const MAX_STRING_BYTES: usize = 256;
const MAX_ARRAY_ITEMS: usize = 16;
const MAX_MAP_ITEMS: usize = 16;
const MAX_VARIABLES: usize = 16;
const MAX_TIER_NAME_BYTES: usize = 64;
const V1_PREFIX: &str = "v1:";
const DISABLED_SYMBOLS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=", "**=", "<<=", ">>=", "&=", "|=", "^=", "fn", "private",
    "let", "const", "static", "while", "loop", "for", "do", "try", "catch", "throw", "import",
    "export", "eval", "print", "debug", "this",
];
const ALLOWED_FUNCTIONS: &[&str] = &[
    "tier", "+", "-", "*", "/", "%", "**", "==", "!=", ">", ">=", "<", "<=", "!",
];

type TierCapture = Arc<Mutex<Option<String>>>;

/// 当前表达式语义版本。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExpressionVersion {
    /// 首版文本、缓存与上下文长度变量语义。
    V1,
}

/// 表达式可引用的闭合计费变量。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExpressionVariable {
    /// 未被独立变量扣减后的输入 token，表达式标识符为 `p`。
    Input,
    /// 输出 token，表达式标识符为 `c`。
    Output,
    /// 缓存读取 token，表达式标识符为 `cr`。
    CacheRead,
    /// 五分钟缓存创建 token，表达式标识符为 `cc`。
    CacheCreation5m,
    /// 一小时缓存创建 token，表达式标识符为 `cc1h`。
    CacheCreation1h,
    /// 不做分项扣减的完整输入上下文长度，表达式标识符为 `len`。
    ContextLength,
}

impl ExpressionVariable {
    const fn bit(self) -> u16 {
        match self {
            Self::Input => 1 << 0,
            Self::Output => 1 << 1,
            Self::CacheRead => 1 << 2,
            Self::CacheCreation5m => 1 << 3,
            Self::CacheCreation1h => 1 << 4,
            Self::ContextLength => 1 << 5,
        }
    }

    fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "p" => Some(Self::Input),
            "c" => Some(Self::Output),
            "cr" => Some(Self::CacheRead),
            "cc" => Some(Self::CacheCreation5m),
            "cc1h" => Some(Self::CacheCreation1h),
            "len" => Some(Self::ContextLength),
            _ => None,
        }
    }
}

/// AST 内省得到的变量使用快照。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExpressionVariableUsage(u16);

impl ExpressionVariableUsage {
    const NONE: Self = Self(0);

    /// 返回表达式是否引用指定变量。
    #[must_use]
    pub const fn uses(self, variable: ExpressionVariable) -> bool {
        self.0 & variable.bit() != 0
    }

    fn insert(&mut self, variable: ExpressionVariable) {
        self.0 |= variable.bit();
    }
}

/// 已编译且完成安全策略检查的计费表达式。
#[derive(Clone)]
pub struct BillingExpression {
    version: ExpressionVersion,
    ast: AST,
    variables: ExpressionVariableUsage,
    ratio_micros: [i64; 3],
}

/// 已校验的版本化表达式定义，可作为模型价格快照的持久化边界。
///
/// 定义只保存受限正文和语义版本；每次请求仍需结合已冻结的倍率生成独立执行快照，
/// 避免把可变配置或跨请求运行状态放进计费生命周期。
#[derive(Clone, Eq, PartialEq)]
pub struct BillingExpressionDefinition {
    version: ExpressionVersion,
    source: Arc<str>,
}

/// 管理或工具侧构造表达式用量时使用的缓存计入口径。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BillingExpressionUsageSemantics {
    /// 输入总量已经包含缓存明细。
    Inclusive,
    /// 缓存明细独立于输入总量。
    CacheSeparated,
}

impl From<BillingExpressionUsageSemantics> for UsageSemantics {
    fn from(value: BillingExpressionUsageSemantics) -> Self {
        match value {
            BillingExpressionUsageSemantics::Inclusive => Self::Inclusive,
            BillingExpressionUsageSemantics::CacheSeparated => Self::CacheSeparated,
        }
    }
}

/// 由闭合整数边界构造的表达式用量，避免上层直接依赖协议内部类型。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BillingExpressionUsage {
    usage: Usage,
    context_length_tokens: i64,
}

impl BillingExpressionUsage {
    /// 构造五类 Token 用量，并完成缓存口径与 checked 汇总校验。
    pub fn new(
        input_tokens: i64,
        output_tokens: i64,
        cache_read_tokens: i64,
        cache_creation_5m_tokens: i64,
        cache_creation_1h_tokens: i64,
        semantics: BillingExpressionUsageSemantics,
    ) -> Result<Self, BillingExpressionError> {
        let input = TokenCount::new(input_tokens)?;
        let output = TokenCount::new(output_tokens)?;
        let cache_read = TokenCount::new(cache_read_tokens)?;
        let cache_creation_5m = TokenCount::new(cache_creation_5m_tokens)?;
        let cache_creation_1h = TokenCount::new(cache_creation_1h_tokens)?;
        let usage = Usage::new(
            input,
            output,
            UsageDetails::new(
                cache_read,
                cache_creation_5m,
                cache_creation_1h,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
            ),
            UsageSource::Estimated,
            semantics.into(),
        )?;
        let context_length_tokens = usage.checked_input_tokens()?.get();
        Ok(Self {
            usage,
            context_length_tokens,
        })
    }

    /// 返回按缓存口径 checked 汇总后的完整输入上下文长度。
    #[must_use]
    pub const fn context_length_tokens(&self) -> i64 {
        self.context_length_tokens
    }
}

impl fmt::Debug for BillingExpressionUsage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BillingExpressionUsage(<已校验>)")
    }
}

impl BillingExpressionDefinition {
    /// 校验并构造版本化表达式定义；无前缀正文按 V1 处理。
    pub fn new(source: String) -> Result<Self, BillingExpressionError> {
        let validated = BillingExpression::compile(&source, unit_pricing_ratios())?;
        Ok(Self {
            version: validated.version(),
            source: Arc::from(source.trim()),
        })
    }

    /// 返回固定的表达式语义版本。
    #[must_use]
    pub const fn version(&self) -> ExpressionVersion {
        self.version
    }

    /// 使用请求级倍率生成不可变执行快照。
    pub fn compile(
        &self,
        ratios: PricingRatios,
    ) -> Result<BillingExpression, BillingExpressionError> {
        BillingExpression::compile(&self.source, ratios)
    }
}

impl fmt::Debug for BillingExpressionDefinition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingExpressionDefinition")
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

impl BillingExpression {
    /// 编译表达式并固定本次请求适用的三层倍率快照。
    ///
    /// 无前缀表达式与 `v1:` 均按 V1 解释；表达式正文和 Rhai 原始错误不会进入公共错误。
    pub fn compile(source: &str, ratios: PricingRatios) -> Result<Self, BillingExpressionError> {
        if source.len() > MAX_BILLING_EXPRESSION_BYTES {
            return Err(BillingExpressionError::SourceTooLong);
        }
        let (version, expression) = parse_version(source)?;
        if expression.trim().is_empty() {
            return Err(BillingExpressionError::Empty);
        }

        let mut engine = sandbox_engine();
        register_compile_tier(&mut engine);
        let scope = variable_scope(
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        );
        let ast = engine
            .compile_expression_with_scope(&scope, expression)
            .map_err(|_| BillingExpressionError::Compile)?;
        let inspection = inspect_ast(&ast);
        if inspection.has_unsupported_function {
            return Err(BillingExpressionError::UnsupportedFeature);
        }
        if !inspection.has_tier {
            return Err(BillingExpressionError::MissingTier);
        }

        Ok(Self {
            version,
            ast,
            variables: inspection.variables,
            ratio_micros: [
                ratios.group().micros(),
                ratios.group_model().micros(),
                ratios.applied_peak().micros(),
            ],
        })
    }

    /// 返回表达式的兼容语义版本。
    #[must_use]
    pub const fn version(&self) -> ExpressionVersion {
        self.version
    }

    /// 返回编译期 AST 内省得到的变量使用快照。
    #[must_use]
    pub const fn variables(&self) -> ExpressionVariableUsage {
        self.variables
    }

    /// 用规范化用量执行表达式并换算为最终整数额度。
    pub fn evaluate(
        &self,
        usage: &Usage,
    ) -> Result<BillingExpressionResult, BillingExpressionError> {
        let variables = BillingExpressionVariables::from_usage(usage, self.variables)?;
        let tier_capture = Arc::new(Mutex::new(None));
        let mut engine = sandbox_engine();
        register_runtime_tier(&mut engine, Arc::clone(&tier_capture));
        let mut scope = variable_scope(
            Decimal::from(variables.input_tokens),
            Decimal::from(variables.output_tokens),
            Decimal::from(variables.cache_read_tokens),
            Decimal::from(variables.cache_creation_5m_tokens),
            Decimal::from(variables.cache_creation_1h_tokens),
            Decimal::from(variables.context_length_tokens),
        );
        let weighted_cost = engine
            .eval_ast_with_scope::<Decimal>(&mut scope, &self.ast)
            .map_err(|_| BillingExpressionError::Evaluation)?;
        if weighted_cost < Decimal::ZERO {
            return Err(BillingExpressionError::NegativeResult);
        }
        let matched_tier = tier_capture
            .lock()
            .map_err(|_| BillingExpressionError::Evaluation)?
            .clone()
            .ok_or(BillingExpressionError::MissingTier)?;
        let resolved = quota_math::quota_from_expression_cost(weighted_cost, self.ratio_micros)?;

        Ok(BillingExpressionResult {
            quota: resolved.quota,
            base_usd: resolved.base_usd,
            total_usd: resolved.total_usd,
            matched_tier,
            variables,
        })
    }

    /// 执行由计费层闭合构造的用量，供管理试算等非协议入口复用生产语义。
    pub fn evaluate_prepared_usage(
        &self,
        usage: &BillingExpressionUsage,
    ) -> Result<BillingExpressionResult, BillingExpressionError> {
        self.evaluate(&usage.usage)
    }
}

impl fmt::Debug for BillingExpression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingExpression")
            .field("version", &self.version)
            .field("variables", &self.variables)
            .finish_non_exhaustive()
    }
}

/// 把请求级已编译表达式接入统一定价生命周期的纯计算解析器。
#[derive(Clone)]
pub(crate) struct ExpressionPricingResolver {
    expression: BillingExpression,
}

impl ExpressionPricingResolver {
    /// 包装已经固定语义版本和请求倍率的表达式快照。
    #[must_use]
    pub(crate) const fn new(expression: BillingExpression) -> Self {
        Self { expression }
    }
}

impl PricingResolver for ExpressionPricingResolver {
    fn resolve(&self, context: &PricingContext<'_>) -> Result<PriceData, PricingError> {
        let resolved = self.expression.evaluate(context.usage())?;
        Ok(PriceData::expression(
            resolved.quota(),
            resolved.total_usd(),
        ))
    }
}

impl fmt::Debug for ExpressionPricingResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExpressionPricingResolver")
            .finish_non_exhaustive()
    }
}

/// 一次表达式执行完成后的定价结果。
#[must_use = "表达式定价结果必须用于预扣、结算或审计"]
#[derive(Clone, Eq, PartialEq)]
pub struct BillingExpressionResult {
    quota: Quota,
    base_usd: Decimal,
    total_usd: Decimal,
    matched_tier: String,
    variables: BillingExpressionVariables,
}

impl BillingExpressionResult {
    /// 返回最终一次舍入后的非负额度。
    #[must_use]
    pub const fn quota(&self) -> Quota {
        self.quota
    }

    /// 返回应用倍率前的精确美元成本。
    #[must_use]
    pub const fn base_usd(&self) -> Decimal {
        self.base_usd
    }

    /// 返回应用倍率后的精确美元成本。
    #[must_use]
    pub const fn total_usd(&self) -> Decimal {
        self.total_usd
    }

    /// 返回本次实际执行分支命中的 tier 名称。
    #[must_use]
    pub fn matched_tier(&self) -> &str {
        &self.matched_tier
    }

    /// 返回表达式本次实际读取的规范化变量值。
    #[must_use]
    pub const fn variables(&self) -> BillingExpressionVariables {
        self.variables
    }
}

impl fmt::Debug for BillingExpressionResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingExpressionResult")
            .field("quota", &self.quota)
            .field("matched_tier", &self.matched_tier)
            .finish_non_exhaustive()
    }
}

/// 表达式配置或执行错误；所有变体均不携带表达式、用量或 Rhai 原始消息。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BillingExpressionError {
    /// 表达式正文为空。
    #[error("计费表达式不能为空")]
    Empty,
    /// 表达式超过固定安全边界。
    #[error("计费表达式过长")]
    SourceTooLong,
    /// 表达式声明了当前不支持的语义版本。
    #[error("计费表达式版本不受支持")]
    UnsupportedVersion,
    /// Rhai 无法在闭合语法环境中编译表达式。
    #[error("计费表达式无法编译")]
    Compile,
    /// 表达式调用了首版白名单以外的能力。
    #[error("计费表达式包含不受支持的能力")]
    UnsupportedFeature,
    /// 表达式未声明或执行任何 tier。
    #[error("计费表达式必须命中一个 tier")]
    MissingTier,
    /// Rhai 执行失败、超过沙箱限制或命中多个 tier。
    #[error("计费表达式执行失败")]
    Evaluation,
    /// 表达式返回负成本。
    #[error("计费表达式结果不能为负数")]
    NegativeResult,
    /// 规范化用量在 checked 运算中违反边界。
    #[error(transparent)]
    Usage(#[from] UsageError),
    /// Decimal 中间运算或最终 quota 换算越界。
    #[error(transparent)]
    Math(#[from] QuotaMathError),
}

/// 表达式执行时使用的规范化整数变量，供管理试算展示与生产审计复用。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BillingExpressionVariables {
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_5m_tokens: i64,
    cache_creation_1h_tokens: i64,
    context_length_tokens: i64,
}

impl BillingExpressionVariables {
    fn from_usage(usage: &Usage, variables: ExpressionVariableUsage) -> Result<Self, UsageError> {
        let details = usage.details();
        let input = normalized_input_tokens(usage, variables)?;
        let context_length = usage.checked_input_tokens()?.get();
        Ok(Self {
            input_tokens: input,
            output_tokens: usage.output_tokens().get(),
            cache_read_tokens: details.cache_read().get(),
            cache_creation_5m_tokens: details.cache_creation_5m().get(),
            cache_creation_1h_tokens: details.cache_creation_1h().get(),
            context_length_tokens: context_length,
        })
    }

    #[must_use]
    pub const fn input_tokens(self) -> i64 {
        self.input_tokens
    }

    #[must_use]
    pub const fn output_tokens(self) -> i64 {
        self.output_tokens
    }

    #[must_use]
    pub const fn cache_read_tokens(self) -> i64 {
        self.cache_read_tokens
    }

    #[must_use]
    pub const fn cache_creation_5m_tokens(self) -> i64 {
        self.cache_creation_5m_tokens
    }

    #[must_use]
    pub const fn cache_creation_1h_tokens(self) -> i64 {
        self.cache_creation_1h_tokens
    }

    #[must_use]
    pub const fn context_length_tokens(self) -> i64 {
        self.context_length_tokens
    }
}

impl fmt::Debug for BillingExpressionVariables {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BillingExpressionVariables(<已规范化>)")
    }
}

struct AstInspection {
    variables: ExpressionVariableUsage,
    has_tier: bool,
    has_unsupported_function: bool,
}

fn parse_version(source: &str) -> Result<(ExpressionVersion, &str), BillingExpressionError> {
    let source = source.trim();
    if let Some(expression) = source.strip_prefix(V1_PREFIX) {
        return Ok((ExpressionVersion::V1, expression));
    }
    if source
        .split_once(':')
        .is_some_and(|(prefix, _)| prefix.starts_with('v'))
    {
        return Err(BillingExpressionError::UnsupportedVersion);
    }
    Ok((ExpressionVersion::V1, source))
}

const fn unit_pricing_ratios() -> PricingRatios {
    PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE)
}

fn sandbox_engine() -> Engine {
    let mut engine = Engine::new();
    engine
        .set_max_operations(MAX_OPERATIONS)
        .set_max_call_levels(MAX_CALL_LEVELS)
        .set_max_expr_depths(MAX_EXPR_DEPTH, MAX_FUNCTION_EXPR_DEPTH)
        .set_max_string_size(MAX_STRING_BYTES)
        .set_max_array_size(MAX_ARRAY_ITEMS)
        .set_max_map_size(MAX_MAP_ITEMS)
        .set_max_variables(MAX_VARIABLES)
        // AST 内省依赖原始变量与 tier 调用，禁止常量折叠改写计费语义。
        .set_optimization_level(OptimizationLevel::None)
        .set_strict_variables(true);
    for symbol in DISABLED_SYMBOLS {
        engine.disable_symbol(*symbol);
    }
    engine
}

fn register_compile_tier(engine: &mut Engine) {
    engine.register_fn("tier", |_: ImmutableString, value: Decimal| value);
    engine.register_fn("tier", |_: ImmutableString, value: i64| {
        Decimal::from(value)
    });
}

fn register_runtime_tier(engine: &mut Engine, capture: TierCapture) {
    let decimal_capture = Arc::clone(&capture);
    engine.register_fn(
        "tier",
        move |name: ImmutableString, value: Decimal| -> Result<Decimal, Box<EvalAltResult>> {
            record_tier(&decimal_capture, name, value)
        },
    );
    engine.register_fn(
        "tier",
        move |name: ImmutableString, value: i64| -> Result<Decimal, Box<EvalAltResult>> {
            record_tier(&capture, name, Decimal::from(value))
        },
    );
}

fn record_tier(
    capture: &TierCapture,
    name: ImmutableString,
    value: Decimal,
) -> Result<Decimal, Box<EvalAltResult>> {
    let name = name.as_str();
    if name.is_empty() || name.len() > MAX_TIER_NAME_BYTES {
        return Err(runtime_error());
    }
    let mut matched = capture.lock().map_err(|_| runtime_error())?;
    if matched.is_some() {
        return Err(runtime_error());
    }
    *matched = Some(name.to_owned());
    Ok(value)
}

fn runtime_error() -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime("billing_expression_rejected".into(), Position::NONE).into()
}

fn variable_scope(
    input: Decimal,
    output: Decimal,
    cache_read: Decimal,
    cache_creation_5m: Decimal,
    cache_creation_1h: Decimal,
    context_length: Decimal,
) -> Scope<'static> {
    let mut scope = Scope::new();
    scope.push_constant("p", input);
    scope.push_constant("c", output);
    scope.push_constant("cr", cache_read);
    scope.push_constant("cc", cache_creation_5m);
    scope.push_constant("cc1h", cache_creation_1h);
    scope.push_constant("len", context_length);
    scope
}

fn inspect_ast(ast: &AST) -> AstInspection {
    let mut inspection = AstInspection {
        variables: ExpressionVariableUsage::NONE,
        has_tier: false,
        has_unsupported_function: false,
    };
    ast.walk(&mut |path| {
        match path.last() {
            Some(ASTNode::Expr(Expr::Variable(variable, ..))) => {
                if let Some(variable) = ExpressionVariable::from_identifier(variable.1.as_str()) {
                    inspection.variables.insert(variable);
                }
            }
            Some(ASTNode::Expr(Expr::FnCall(call, ..))) => {
                let name = call.name.as_str();
                inspection.has_tier |= name == "tier";
                inspection.has_unsupported_function |= !ALLOWED_FUNCTIONS.contains(&name);
            }
            Some(ASTNode::Expr(Expr::MethodCall(..))) => {
                inspection.has_unsupported_function = true;
            }
            _ => {}
        }
        true
    });
    inspection
}

fn normalized_input_tokens(
    usage: &Usage,
    variables: ExpressionVariableUsage,
) -> Result<i64, UsageError> {
    if usage.semantics() == UsageSemantics::CacheSeparated {
        return Ok(usage.input_tokens().get());
    }

    let details = usage.details();
    [
        (ExpressionVariable::CacheRead, details.cache_read().get()),
        (
            ExpressionVariable::CacheCreation5m,
            details.cache_creation_5m().get(),
        ),
        (
            ExpressionVariable::CacheCreation1h,
            details.cache_creation_1h().get(),
        ),
    ]
    .into_iter()
    .filter(|(variable, _)| variables.uses(*variable))
    .try_fold(usage.input_tokens().get(), |remaining, (_, tokens)| {
        remaining
            .checked_sub(tokens)
            .ok_or(UsageError::InputDetailsExceedTotal)
    })
}
