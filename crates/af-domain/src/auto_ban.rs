use std::{collections::BTreeSet, fmt};

use thiserror::Error;

use crate::UpstreamServerStatus;

/// 单个渠道允许配置的自动禁用规则总数上限。
pub const MAX_CHANNEL_AUTO_BAN_RULES: usize = 64;
/// 单条自动禁用关键词允许的最大 UTF-8 字节数。
pub const MAX_CHANNEL_AUTO_BAN_KEYWORD_BYTES: usize = 256;
/// 单个渠道全部自动禁用关键词允许占用的最大 UTF-8 字节数。
pub const MAX_CHANNEL_AUTO_BAN_KEYWORD_TOTAL_BYTES: usize = 8 * 1_024;

/// 渠道自动禁用规则配置错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ChannelAutoBanRulesError {
    /// 状态码与关键词合计超过固定容量。
    #[error("渠道自动禁用规则数量超过上限")]
    TooManyRules,
    /// 状态码不在精确 5xx 范围内。
    #[error("渠道自动禁用状态码无效")]
    InvalidStatusCode,
    /// 关键词为空、含控制字符或超过单条上限。
    #[error("渠道自动禁用关键词无效")]
    InvalidKeyword,
    /// 全部关键词超过固定总字节上限。
    #[error("渠道自动禁用关键词总长度超过上限")]
    TooManyKeywordBytes,
}

/// 已校验、可跨持久化与运行时边界传递的渠道自动禁用规则。
#[derive(Clone, Default, Eq, PartialEq)]
pub struct ChannelAutoBanRules {
    server_statuses: Vec<UpstreamServerStatus>,
    keywords: Vec<String>,
}

impl ChannelAutoBanRules {
    /// 校验容量、精确 5xx 状态码和关键词后创建规范化规则集。
    pub fn new(
        status_codes: Vec<u16>,
        keywords: Vec<String>,
    ) -> Result<Self, ChannelAutoBanRulesError> {
        if status_codes
            .len()
            .checked_add(keywords.len())
            .is_none_or(|count| count > MAX_CHANNEL_AUTO_BAN_RULES)
        {
            return Err(ChannelAutoBanRulesError::TooManyRules);
        }

        let server_statuses = status_codes
            .into_iter()
            .map(|status| {
                UpstreamServerStatus::new(status).ok_or(ChannelAutoBanRulesError::InvalidStatusCode)
            })
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect();

        let mut total_keyword_bytes = 0_usize;
        let mut normalized_keywords = BTreeSet::new();
        for keyword in keywords {
            let keyword = keyword.trim();
            if keyword.is_empty()
                || keyword.chars().any(char::is_control)
                || keyword.len() > MAX_CHANNEL_AUTO_BAN_KEYWORD_BYTES
            {
                return Err(ChannelAutoBanRulesError::InvalidKeyword);
            }
            let normalized = keyword.to_lowercase();
            if normalized.len() > MAX_CHANNEL_AUTO_BAN_KEYWORD_BYTES {
                return Err(ChannelAutoBanRulesError::InvalidKeyword);
            }
            total_keyword_bytes = total_keyword_bytes
                .checked_add(normalized.len())
                .ok_or(ChannelAutoBanRulesError::TooManyKeywordBytes)?;
            if total_keyword_bytes > MAX_CHANNEL_AUTO_BAN_KEYWORD_TOTAL_BYTES {
                return Err(ChannelAutoBanRulesError::TooManyKeywordBytes);
            }
            normalized_keywords.insert(normalized);
        }

        Ok(Self {
            server_statuses,
            keywords: normalized_keywords.into_iter().collect(),
        })
    }

    /// 返回排序去重后的精确 5xx 状态码规则。
    #[must_use]
    pub fn server_statuses(&self) -> &[UpstreamServerStatus] {
        &self.server_statuses
    }

    /// 返回小写规范化、排序去重后的关键词规则。
    #[must_use]
    pub fn keywords(&self) -> &[String] {
        &self.keywords
    }

    /// 返回当前规则集是否为空；空规则集不会产生自动禁用副作用。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.server_statuses.is_empty() && self.keywords.is_empty()
    }
}

impl fmt::Debug for ChannelAutoBanRules {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelAutoBanRules")
            .field("server_status_count", &self.server_statuses.len())
            .field("keyword_count", &self.keywords.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_validate_normalize_and_redact_debug_output() {
        const CANARY: &str = "Workspace-Disabled-Canary";
        let rules = ChannelAutoBanRules::new(
            vec![503, 500, 503],
            vec![format!("  {CANARY}  "), CANARY.to_lowercase()],
        )
        .unwrap();

        assert_eq!(
            rules
                .server_statuses()
                .iter()
                .map(|status| status.get())
                .collect::<Vec<_>>(),
            vec![500, 503]
        );
        assert_eq!(rules.keywords(), &[CANARY.to_lowercase()]);
        let rendered = format!("{rules:?}");
        assert!(!rendered.contains(CANARY));
        assert!(!rendered.contains(&CANARY.to_lowercase()));
    }

    #[test]
    fn rules_reject_invalid_statuses_keywords_and_unbounded_input() {
        assert_eq!(
            ChannelAutoBanRules::new(vec![499], Vec::new()),
            Err(ChannelAutoBanRulesError::InvalidStatusCode)
        );
        assert_eq!(
            ChannelAutoBanRules::new(Vec::new(), vec!["line\nbreak".to_owned()]),
            Err(ChannelAutoBanRulesError::InvalidKeyword)
        );
        assert_eq!(
            ChannelAutoBanRules::new(
                Vec::new(),
                vec!["duplicate".to_owned(); MAX_CHANNEL_AUTO_BAN_RULES + 1],
            ),
            Err(ChannelAutoBanRulesError::TooManyRules)
        );
    }
}
