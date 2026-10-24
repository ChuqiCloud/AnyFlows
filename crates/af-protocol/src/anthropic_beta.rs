use std::{collections::HashSet, fmt};

/// Anthropic beta 列表输入及规范化结果的最大字节数。
pub const MAX_ANTHROPIC_BETA_BYTES: usize = 8_192;

/// Anthropic beta 列表只允许有界的 ASCII HTTP token。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnthropicBetaError {
    /// 列表含有不允许的空格、控制字节或非 ASCII 值。
    InvalidValue,
    /// 原始值或规范化列表超过固定大小上限。
    TooLarge,
}

impl fmt::Display for AnthropicBetaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidValue => "Anthropic beta 列表无效",
            Self::TooLarge => "Anthropic beta 列表超过大小限制",
        })
    }
}

impl std::error::Error for AnthropicBetaError {}

/// 验证、去重并规范化单个 Anthropic beta 列表。
pub fn normalize_anthropic_beta(value: &str) -> Result<String, AnthropicBetaError> {
    normalize_anthropic_beta_values([value])
}

/// 合并重复 Header 行和逗号列表，保留未知 beta 的拼写及首次出现顺序。
pub fn normalize_anthropic_beta_values<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Result<String, AnthropicBetaError> {
    let mut tokens = Vec::new();
    let mut seen = HashSet::new();
    let mut input_bytes = 0_usize;
    let mut output_bytes = 0_usize;
    for value in values {
        input_bytes = input_bytes
            .checked_add(value.len())
            .ok_or(AnthropicBetaError::TooLarge)?;
        if input_bytes > MAX_ANTHROPIC_BETA_BYTES {
            return Err(AnthropicBetaError::TooLarge);
        }
        for token in value.split(',') {
            let token = token.trim_matches([' ', '\t']);
            if token.is_empty() {
                continue;
            }
            if !token.bytes().all(is_http_token_byte) {
                return Err(AnthropicBetaError::InvalidValue);
            }
            if seen.insert(token) {
                output_bytes = output_bytes
                    .checked_add(token.len())
                    .and_then(|bytes| bytes.checked_add(usize::from(!tokens.is_empty())))
                    .ok_or(AnthropicBetaError::TooLarge)?;
                if output_bytes > MAX_ANTHROPIC_BETA_BYTES {
                    return Err(AnthropicBetaError::TooLarge);
                }
                tokens.push(token);
            }
        }
    }
    Ok(tokens.join(","))
}

fn is_http_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'..=b'\'' | b'*' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_beta_values_preserve_unknown_names_and_are_idempotent() {
        let beta = normalize_anthropic_beta_values([
            " compact-2026-09-04,Provider_Extension-2099-01-01 ",
            "Provider_Extension-2099-01-01,\tcompact-2026-01-12,,",
        ])
        .unwrap();
        assert_eq!(
            beta,
            "compact-2026-09-04,Provider_Extension-2099-01-01,compact-2026-01-12"
        );
        assert_eq!(normalize_anthropic_beta(&beta).unwrap(), beta);
        assert_eq!(normalize_anthropic_beta(" , \t,").unwrap(), "");
    }

    #[test]
    fn beta_lists_reject_invalid_bytes_and_bound_input_and_merged_output() {
        for value in [
            "secret beta",
            "secret\rbeta",
            "secret\nbeta",
            "secret\0beta",
            "测试",
        ] {
            assert_eq!(
                normalize_anthropic_beta(value),
                Err(AnthropicBetaError::InvalidValue)
            );
        }
        let exact = "x".repeat(MAX_ANTHROPIC_BETA_BYTES);
        assert_eq!(normalize_anthropic_beta(&exact).unwrap(), exact);
        let oversized = "x".repeat(MAX_ANTHROPIC_BETA_BYTES + 1);
        assert_eq!(
            normalize_anthropic_beta(&oversized),
            Err(AnthropicBetaError::TooLarge)
        );
        let half_a = "a".repeat(MAX_ANTHROPIC_BETA_BYTES / 2);
        let half_b = "b".repeat(MAX_ANTHROPIC_BETA_BYTES / 2);
        assert_eq!(
            normalize_anthropic_beta_values([half_a.as_str(), half_b.as_str()]),
            Err(AnthropicBetaError::TooLarge)
        );
    }
}
