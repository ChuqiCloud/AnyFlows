use std::{collections::BTreeSet, fmt, sync::Arc};

use af_domain::{MAX_CHANNEL_AUTO_BAN_KEYWORD_BYTES, MAX_CHANNEL_AUTO_BAN_KEYWORD_TOTAL_BYTES};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use thiserror::Error;

/// 单条自动禁用关键词允许的最大 UTF-8 字节数。
pub const MAX_AUTO_BAN_KEYWORD_BYTES: usize = MAX_CHANNEL_AUTO_BAN_KEYWORD_BYTES;
/// 单份策略全部关键词允许占用的最大 UTF-8 字节数。
pub const MAX_AUTO_BAN_KEYWORD_TOTAL_BYTES: usize = MAX_CHANNEL_AUTO_BAN_KEYWORD_TOTAL_BYTES;
/// 单次自动禁用证据允许的最大 UTF-8 字节数。
pub const MAX_AUTO_BAN_EVIDENCE_BYTES: usize = 2 * 1_024;

/// 已脱敏自动禁用证据构造失败。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AutoBanEvidenceError {
    /// 去除首尾空白后没有可匹配内容。
    #[error("自动禁用证据不能为空")]
    Empty,
    /// 证据超过固定字节上限。
    #[error("自动禁用证据超过长度上限")]
    TooLong,
    /// 证据包含换行、制表符或其他控制字符。
    #[error("自动禁用证据包含控制字符")]
    ContainsControl,
}

/// 调用方已完成脱敏的有界上游错误证据。
///
/// 本类型不提供内容访问器，`Debug` 也只显示长度；调用方必须先从受信的结构化
/// `error.type`、`error.code` 或已脱敏摘要构造，禁止直接传入完整响应正文。
#[derive(Clone, Eq, PartialEq)]
pub struct AutoBanEvidence {
    normalized: Box<str>,
}

impl AutoBanEvidence {
    /// 校验边界并将已脱敏文本归一为小写匹配证据。
    pub fn from_redacted_text(value: &str) -> Result<Self, AutoBanEvidenceError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(AutoBanEvidenceError::Empty);
        }
        if value.len() > MAX_AUTO_BAN_EVIDENCE_BYTES {
            return Err(AutoBanEvidenceError::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(AutoBanEvidenceError::ContainsControl);
        }
        let normalized = value.to_lowercase();
        if normalized.len() > MAX_AUTO_BAN_EVIDENCE_BYTES {
            return Err(AutoBanEvidenceError::TooLong);
        }
        Ok(Self {
            normalized: normalized.into_boxed_str(),
        })
    }

    pub(crate) fn normalized(&self) -> &str {
        &self.normalized
    }
}

impl fmt::Debug for AutoBanEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutoBanEvidence")
            .field("byte_len", &self.normalized.len())
            .finish()
    }
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct AutoBanKeyword {
    normalized: Box<str>,
}

impl AutoBanKeyword {
    fn new(value: &str) -> Result<Self, AutoBanKeywordBuildError> {
        let value = value.trim();
        if value.is_empty()
            || value.chars().any(char::is_control)
            || value.len() > MAX_AUTO_BAN_KEYWORD_BYTES
        {
            return Err(AutoBanKeywordBuildError::InvalidKeyword);
        }
        let normalized = value.to_lowercase();
        if normalized.len() > MAX_AUTO_BAN_KEYWORD_BYTES {
            return Err(AutoBanKeywordBuildError::InvalidKeyword);
        }
        Ok(Self {
            normalized: normalized.into_boxed_str(),
        })
    }
}

impl fmt::Debug for AutoBanKeyword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutoBanKeyword")
            .field("byte_len", &self.normalized.len())
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct AutoBanKeywordMatcher {
    keywords: Vec<AutoBanKeyword>,
    automaton: Arc<AhoCorasick>,
}

impl AutoBanKeywordMatcher {
    pub(crate) fn build(values: Vec<String>) -> Result<Option<Self>, AutoBanKeywordBuildError> {
        if values.is_empty() {
            return Ok(None);
        }

        let mut total_bytes = 0_usize;
        let mut unique = BTreeSet::new();
        for value in values {
            let keyword = AutoBanKeyword::new(&value)?;
            total_bytes = total_bytes
                .checked_add(keyword.normalized.len())
                .ok_or(AutoBanKeywordBuildError::TooManyBytes)?;
            if total_bytes > MAX_AUTO_BAN_KEYWORD_TOTAL_BYTES {
                return Err(AutoBanKeywordBuildError::TooManyBytes);
            }
            unique.insert(keyword);
        }
        let keywords = unique.into_iter().collect::<Vec<_>>();
        let automaton = AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostFirst)
            .build(keywords.iter().map(|keyword| keyword.normalized.as_ref()))
            .map_err(|_| AutoBanKeywordBuildError::MatcherBuild)?;
        Ok(Some(Self {
            keywords,
            automaton: Arc::new(automaton),
        }))
    }

    pub(crate) fn len(&self) -> usize {
        self.keywords.len()
    }

    pub(crate) fn is_match(&self, evidence: &AutoBanEvidence) -> bool {
        self.automaton.is_match(evidence.normalized())
    }
}

impl PartialEq for AutoBanKeywordMatcher {
    fn eq(&self, other: &Self) -> bool {
        self.keywords == other.keywords
    }
}

impl Eq for AutoBanKeywordMatcher {}

impl fmt::Debug for AutoBanKeywordMatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutoBanKeywordMatcher")
            .field("keyword_count", &self.keywords.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutoBanKeywordBuildError {
    InvalidKeyword,
    TooManyBytes,
    MatcherBuild,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_is_bounded_normalized_and_debug_redacted() {
        const CANARY: &str = "Sensitive-Evidence-Canary";
        let evidence = AutoBanEvidence::from_redacted_text(&format!("  {CANARY}  ")).unwrap();

        assert_eq!(evidence.normalized(), "sensitive-evidence-canary");
        let rendered = format!("{evidence:?}");
        assert!(!rendered.contains(CANARY));
        assert!(!rendered.contains("sensitive-evidence-canary"));
        assert_eq!(
            AutoBanEvidence::from_redacted_text(" \n "),
            Err(AutoBanEvidenceError::Empty)
        );
        assert_eq!(
            AutoBanEvidence::from_redacted_text("line\nsecret"),
            Err(AutoBanEvidenceError::ContainsControl)
        );
        assert_eq!(
            AutoBanEvidence::from_redacted_text(&"x".repeat(MAX_AUTO_BAN_EVIDENCE_BYTES + 1)),
            Err(AutoBanEvidenceError::TooLong)
        );
    }

    #[test]
    fn matcher_normalizes_deduplicates_and_matches_without_exposing_keywords() {
        const CANARY: &str = "Workspace Disabled Canary";
        let matcher = AutoBanKeywordMatcher::build(vec![
            format!("  {CANARY}  "),
            CANARY.to_lowercase(),
            "quota exhausted".to_owned(),
        ])
        .unwrap()
        .unwrap();
        let evidence = AutoBanEvidence::from_redacted_text(
            "stable code: workspace disabled canary; retry is not useful",
        )
        .unwrap();

        assert_eq!(matcher.len(), 2);
        assert!(matcher.is_match(&evidence));
        let rendered = format!("{matcher:?}");
        assert!(!rendered.contains(CANARY));
        assert!(!rendered.contains("quota exhausted"));
    }

    #[test]
    fn matcher_rejects_invalid_and_unbounded_keywords() {
        assert_eq!(
            AutoBanKeywordMatcher::build(vec!["   ".to_owned()]),
            Err(AutoBanKeywordBuildError::InvalidKeyword)
        );
        assert_eq!(
            AutoBanKeywordMatcher::build(vec!["line\nbreak".to_owned()]),
            Err(AutoBanKeywordBuildError::InvalidKeyword)
        );
        assert_eq!(
            AutoBanKeywordMatcher::build(vec!["x".repeat(MAX_AUTO_BAN_KEYWORD_BYTES + 1)]),
            Err(AutoBanKeywordBuildError::InvalidKeyword)
        );

        let oversized = (0..=MAX_AUTO_BAN_KEYWORD_TOTAL_BYTES / MAX_AUTO_BAN_KEYWORD_BYTES)
            .map(|index| {
                format!(
                    "{index:032x}{}",
                    "x".repeat(MAX_AUTO_BAN_KEYWORD_BYTES - 32)
                )
            })
            .collect();
        assert_eq!(
            AutoBanKeywordMatcher::build(oversized),
            Err(AutoBanKeywordBuildError::TooManyBytes)
        );
    }
}
