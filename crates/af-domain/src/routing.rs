use regex::Regex;
use thiserror::Error;

/// 智能路由模型匹配表达式允许的最大 UTF-8 字节数。
pub const MAX_ROUTE_MODEL_PATTERN_BYTES: usize = 255;

/// 智能路由的请求匹配模式。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RouteMode {
    /// 使用精确模型名或 `re:` 正则匹配请求模型。
    Pattern,
    /// 仅按显式对外模型组名匹配，不参与正则覆盖。
    ExplicitGroup,
}

impl RouteMode {
    /// 返回稳定持久化编码。
    #[must_use]
    pub const fn code(self) -> i16 {
        match self {
            Self::Pattern => 1,
            Self::ExplicitGroup => 2,
        }
    }

    /// 返回管理 API 使用的稳定字面值。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pattern => "pattern",
            Self::ExplicitGroup => "explicit_group",
        }
    }
}

impl TryFrom<i16> for RouteMode {
    type Error = RoutePatternError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Pattern),
            2 => Ok(Self::ExplicitGroup),
            _ => Err(RoutePatternError::UnknownMode),
        }
    }
}

/// 智能路由的候选选择策略。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RouteStrategy {
    /// 按优先级和权重选择候选。
    Weighted,
    /// 在同优先级候选间按稳定顺序轮询。
    RoundRobin,
    /// 将健康候选拆分为主池和观察池。
    StableFirst,
}

impl RouteStrategy {
    /// 返回稳定持久化编码。
    #[must_use]
    pub const fn code(self) -> i16 {
        match self {
            Self::Weighted => 1,
            Self::RoundRobin => 2,
            Self::StableFirst => 3,
        }
    }

    /// 返回管理 API 使用的稳定字面值。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Weighted => "weighted",
            Self::RoundRobin => "round_robin",
            Self::StableFirst => "stable_first",
        }
    }
}

impl TryFrom<i16> for RouteStrategy {
    type Error = RoutePatternError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Weighted),
            2 => Ok(Self::RoundRobin),
            3 => Ok(Self::StableFirst),
            _ => Err(RoutePatternError::UnknownStrategy),
        }
    }
}

/// 智能路由模式、策略或模型匹配表达式无效。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RoutePatternError {
    /// 路由模式持久化编码不在闭合集合中。
    #[error("智能路由模式无效")]
    UnknownMode,
    /// 路由策略持久化编码不在闭合集合中。
    #[error("智能路由策略无效")]
    UnknownStrategy,
    /// 模型匹配表达式为空、过长或含控制字符。
    #[error("智能路由模型匹配表达式无效")]
    InvalidPattern,
    /// `re:` 后的正则表达式无法安全编译。
    #[error("智能路由正则表达式无效")]
    InvalidRegex,
}

/// 校验精确模型名或 `re:` 前缀正则，不保留编译后的运行时状态。
pub fn validate_route_model_pattern(value: &str) -> Result<(), RoutePatternError> {
    route_model_pattern_matches(value, "").map(|_| ())
}

/// 使用与写入校验完全一致的规则匹配请求模型，避免运行时重新解释正则语义。
pub fn route_model_pattern_matches(
    value: &str,
    requested_model: &str,
) -> Result<bool, RoutePatternError> {
    if value.is_empty()
        || value.len() > MAX_ROUTE_MODEL_PATTERN_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(RoutePatternError::InvalidPattern);
    }
    let Some(expression) = value.strip_prefix("re:") else {
        return Ok(value == requested_model);
    };
    if expression.is_empty() {
        return Err(RoutePatternError::InvalidRegex);
    }
    Regex::new(expression)
        .map_err(|_| RoutePatternError::InvalidRegex)
        .map(|regex| regex.is_match(requested_model))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_codes_and_patterns_are_closed() {
        assert_eq!(RouteMode::try_from(1), Ok(RouteMode::Pattern));
        assert_eq!(RouteMode::try_from(2), Ok(RouteMode::ExplicitGroup));
        assert_eq!(RouteStrategy::try_from(3), Ok(RouteStrategy::StableFirst));
        assert_eq!(validate_route_model_pattern("gpt-5.5"), Ok(()));
        assert_eq!(validate_route_model_pattern("re:^gpt-5\\.[0-9]+$"), Ok(()));
        assert_eq!(route_model_pattern_matches("gpt-5.5", "gpt-5.5"), Ok(true));
        assert_eq!(
            route_model_pattern_matches("re:^gpt-5\\.[0-9]+$", "gpt-5.5"),
            Ok(true)
        );
        assert_eq!(
            route_model_pattern_matches("re:^gpt-5\\.[0-9]+$", "claude-opus"),
            Ok(false)
        );
        assert_eq!(
            validate_route_model_pattern("re:("),
            Err(RoutePatternError::InvalidRegex)
        );
        assert_eq!(
            validate_route_model_pattern(""),
            Err(RoutePatternError::InvalidPattern)
        );
    }
}
