//! Upstream usage settings do not change downstream billing or wallet balances.
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamUsageProvider {
    #[default]
    Auto,
    Codex,
    NewApi,
    OneApi,
    Sub2api,
    None,
}

impl UpstreamUsageProvider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Codex => "codex",
            Self::NewApi => "new_api",
            Self::OneApi => "one_api",
            Self::Sub2api => "sub2api",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamUsageScope {
    #[default]
    Key,
    Account,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamUsageConfig {
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub provider: UpstreamUsageProvider,
    #[serde(default)]
    pub scope: UpstreamUsageScope,
    pub profile_id: Option<i64>,
    pub refresh_interval_secs: Option<i64>,
    pub quota_per_unit: Option<String>,
    pub currency: Option<String>,
    pub low_balance_threshold: Option<String>,
    #[serde(default)]
    pub pause_when_exhausted: bool,
    #[serde(default)]
    pub inherited: bool,
}

impl UpstreamUsageConfig {
    pub fn validate(&self) -> Result<(), UpstreamUsageError> {
        if self.version < 0
            || self.profile_id.is_some_and(|id| id <= 0)
            || (self.scope == UpstreamUsageScope::Account && self.profile_id.is_none())
            || (self.scope == UpstreamUsageScope::Key && self.profile_id.is_some())
            || self
                .refresh_interval_secs
                .is_some_and(|value| !(60..=604_800).contains(&value))
        {
            return Err(UpstreamUsageError::InvalidInput);
        }
        validate_upstream_usage_amount(self.quota_per_unit.as_deref(), true)?;
        validate_upstream_usage_amount(self.low_balance_threshold.as_deref(), false)?;
        validate_upstream_usage_currency(self.currency.as_deref())
    }
}

/// Query secrets are write-only and never included in Debug output.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamUsageProfileWrite {
    pub name: String,
    pub provider: UpstreamUsageProvider,
    pub base_url: String,
    pub platform_user_id: Option<String>,
    pub quota_per_unit: Option<String>,
    pub currency: Option<String>,
    #[serde(default)]
    pub version: i64,
    pub secret: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpstreamUsageProfile {
    pub id: i64,
    pub name: String,
    pub provider: UpstreamUsageProvider,
    pub base_url: String,
    pub platform_user_id: Option<String>,
    pub quota_per_unit: Option<String>,
    pub currency: Option<String>,
    pub version: i64,
    pub has_secret: bool,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum UpstreamUsageError {
    #[error("上游额度配置无效")]
    InvalidInput,
    #[error("需要管理员权限")]
    Forbidden,
    #[error("渠道、凭据或查询配置不存在")]
    NotFound,
    #[error("配置已更新，请刷新后重试")]
    Conflict,
    #[error("查询配置仍被凭据引用")]
    InUse,
    #[error("上游额度服务暂不可用")]
    Unavailable,
}

pub fn validate_upstream_usage_amount(
    value: Option<&str>,
    positive: bool,
) -> Result<(), UpstreamUsageError> {
    if let Some(value) = value {
        let parsed = value
            .parse::<Decimal>()
            .map_err(|_| UpstreamUsageError::InvalidInput)?;
        if value.len() > 64
            || value.trim() != value
            || parsed.is_sign_negative()
            || (positive && parsed.is_zero())
            || parsed.scale() > 12
        {
            return Err(UpstreamUsageError::InvalidInput);
        }
    }
    Ok(())
}

pub fn validate_upstream_usage_currency(value: Option<&str>) -> Result<(), UpstreamUsageError> {
    if value.is_some_and(|value| {
        value.is_empty()
            || value.len() > 16
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    }) {
        return Err(UpstreamUsageError::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_usage_keeps_unknown_unlimited_and_currency_conversion_explicit() {
        assert!(UpstreamUsageConfig::default().validate().is_ok());
        let config = UpstreamUsageConfig {
            quota_per_unit: Some("0".to_owned()),
            ..Default::default()
        };
        assert_eq!(config.validate(), Err(UpstreamUsageError::InvalidInput));
        let account = UpstreamUsageConfig {
            scope: UpstreamUsageScope::Account,
            ..Default::default()
        };
        assert_eq!(account.validate(), Err(UpstreamUsageError::InvalidInput));
    }
}
