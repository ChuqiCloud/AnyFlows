use std::{error::Error, fmt};

/// 非负令牌数。
///
/// 该类型不实现 `Default`、Serde 或隐式整数转换，协议边界必须显式校验外部值。
///
/// ```compile_fail
/// use af_protocol::TokenCount;
///
/// let _ = TokenCount(-1);
/// ```
///
/// ```compile_fail
/// use af_protocol::TokenCount;
///
/// let _: i64 = TokenCount::ZERO.into();
/// ```
///
/// ```compile_fail
/// use af_protocol::TokenCount;
///
/// let _ = TokenCount::default();
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TokenCount(i64);

impl TokenCount {
    /// 显式的零令牌数，不表示上游未返回用量。
    pub const ZERO: Self = Self(0);

    /// 校验并构造非负令牌数。
    pub const fn new(tokens: i64) -> Result<Self, UsageError> {
        if tokens < 0 {
            Err(UsageError::NegativeTokenCount)
        } else {
            Ok(Self(tokens))
        }
    }

    /// 返回底层令牌数，供协议编码和计费边界使用。
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }

    fn checked_add(self, rhs: Self) -> Result<Self, UsageError> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(UsageError::Overflow)
    }
}

impl TryFrom<i64> for TokenCount {
    type Error = UsageError;

    fn try_from(tokens: i64) -> Result<Self, Self::Error> {
        Self::new(tokens)
    }
}

/// 上游返回的细分令牌用量。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsageDetails {
    cache_read: TokenCount,
    cache_creation_5m: TokenCount,
    cache_creation_1h: TokenCount,
    reasoning: TokenCount,
    audio_input: TokenCount,
    audio_output: TokenCount,
}

impl UsageDetails {
    /// 构造完整的细分令牌用量；缺失的上游字段应显式传入零。
    #[must_use]
    pub const fn new(
        cache_read: TokenCount,
        cache_creation_5m: TokenCount,
        cache_creation_1h: TokenCount,
        reasoning: TokenCount,
        audio_input: TokenCount,
        audio_output: TokenCount,
    ) -> Self {
        Self {
            cache_read,
            cache_creation_5m,
            cache_creation_1h,
            reasoning,
            audio_input,
            audio_output,
        }
    }

    /// 返回缓存命中令牌数。
    #[must_use]
    pub const fn cache_read(&self) -> TokenCount {
        self.cache_read
    }

    /// 返回五分钟缓存写入令牌数。
    #[must_use]
    pub const fn cache_creation_5m(&self) -> TokenCount {
        self.cache_creation_5m
    }

    /// 返回一小时缓存写入令牌数。
    #[must_use]
    pub const fn cache_creation_1h(&self) -> TokenCount {
        self.cache_creation_1h
    }

    /// 返回推理令牌数。
    #[must_use]
    pub const fn reasoning(&self) -> TokenCount {
        self.reasoning
    }

    /// 返回音频输入令牌数。
    #[must_use]
    pub const fn audio_input(&self) -> TokenCount {
        self.audio_input
    }

    /// 返回音频输出令牌数。
    #[must_use]
    pub const fn audio_output(&self) -> TokenCount {
        self.audio_output
    }

    fn checked_cache_tokens(&self) -> Result<TokenCount, UsageError> {
        self.cache_read
            .checked_add(self.cache_creation_5m)?
            .checked_add(self.cache_creation_1h)
    }

    fn checked_inclusive_input_details(&self) -> Result<TokenCount, UsageError> {
        self.checked_cache_tokens()?.checked_add(self.audio_input)
    }

    fn checked_output_details(&self) -> Result<TokenCount, UsageError> {
        self.reasoning.checked_add(self.audio_output)
    }
}

/// 用量数据的来源。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UsageSource {
    /// 上游协议明确返回的用量。
    Upstream,
    /// 本地令牌器估算的用量。
    Estimated,
}

/// 上游输入令牌总量对缓存明细的计入口径。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UsageSemantics {
    /// 输入总量已经包含缓存明细。
    Inclusive,
    /// 缓存明细独立于输入总量，需要额外计入汇总。
    CacheSeparated,
}

/// 一次模型调用的规范化令牌用量。
///
/// 结构中不保存可与明细冲突的总令牌数；调用方必须使用 checked 汇总方法。
/// 上游未返回用量时应使用 `None`，不得构造全零值代替缺失状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    input_tokens: TokenCount,
    output_tokens: TokenCount,
    details: UsageDetails,
    source: UsageSource,
    semantics: UsageSemantics,
}

impl Usage {
    /// 构造并校验规范化用量及其细分项边界。
    pub fn new(
        input_tokens: TokenCount,
        output_tokens: TokenCount,
        details: UsageDetails,
        source: UsageSource,
        semantics: UsageSemantics,
    ) -> Result<Self, UsageError> {
        // Inclusive 口径下缓存和音频输入都是输入总量的子项；分离口径只校验音频子项。
        let input_details = match semantics {
            UsageSemantics::Inclusive => details.checked_inclusive_input_details()?,
            UsageSemantics::CacheSeparated => details.audio_input,
        };
        if input_details > input_tokens {
            return Err(UsageError::InputDetailsExceedTotal);
        }

        if details.checked_output_details()? > output_tokens {
            return Err(UsageError::OutputDetailsExceedTotal);
        }

        Ok(Self {
            input_tokens,
            output_tokens,
            details,
            source,
            semantics,
        })
    }

    /// 返回不含分离缓存明细的输入令牌数。
    #[must_use]
    pub const fn input_tokens(&self) -> TokenCount {
        self.input_tokens
    }

    /// 返回输出令牌数。
    #[must_use]
    pub const fn output_tokens(&self) -> TokenCount {
        self.output_tokens
    }

    /// 返回只读的细分令牌用量。
    #[must_use]
    pub const fn details(&self) -> &UsageDetails {
        &self.details
    }

    /// 返回用量来源。
    #[must_use]
    pub const fn source(&self) -> UsageSource {
        self.source
    }

    /// 返回上游用量口径。
    #[must_use]
    pub const fn semantics(&self) -> UsageSemantics {
        self.semantics
    }

    /// 按上游口径计算输入令牌总量。
    pub fn checked_input_tokens(&self) -> Result<TokenCount, UsageError> {
        match self.semantics {
            UsageSemantics::Inclusive => Ok(self.input_tokens),
            UsageSemantics::CacheSeparated => self
                .input_tokens
                .checked_add(self.details.checked_cache_tokens()?),
        }
    }

    /// 按上游口径计算输入与输出的令牌总量。
    pub fn checked_total_tokens(&self) -> Result<TokenCount, UsageError> {
        self.checked_input_tokens()?.checked_add(self.output_tokens)
    }
}

/// 用量构造与 checked 汇总错误，不保留任何外部数值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageError {
    /// 外部令牌数为负数。
    NegativeTokenCount,
    /// 输入细分项超过输入总量。
    InputDetailsExceedTotal,
    /// 输出细分项超过输出总量。
    OutputDetailsExceedTotal,
    /// checked 累加超过 `i64` 上界。
    Overflow,
}

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NegativeTokenCount => "令牌数不能为负数",
            Self::InputDetailsExceedTotal => "输入令牌明细不能超过输入总量",
            Self::OutputDetailsExceedTotal => "输出令牌明细不能超过输出总量",
            Self::Overflow => "令牌数累加溢出",
        };
        formatter.write_str(message)
    }
}

impl Error for UsageError {}
