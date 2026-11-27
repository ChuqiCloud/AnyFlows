//! Shared merchant supply contracts. Upstream secrets never occur in response types.
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_MERCHANT_SERVICE_MODELS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum MerchantPoolError {
    #[error("merchant pool request is invalid")]
    InvalidInput,
    #[error("merchant model pricing is unavailable")]
    ModelPriceUnavailable,
    #[error("merchant pool permission denied")]
    Forbidden,
    #[error("merchant pool resource not found")]
    NotFound,
    #[error("merchant pool state conflict")]
    Conflict,
    #[error("merchant pool resource has unfinished business")]
    InUse,
    #[error("merchant pool capacity reached")]
    LimitReached,
    #[error("merchant pool service unavailable")]
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantProfile {
    pub id: i64,
    pub owner_user_id: i64,
    pub name: String,
    pub description: String,
    pub status: String,
    pub review_reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantApplication {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantPool {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub commission_bps: u16,
    pub commission_inherit_global: bool,
    pub effective_commission_bps: u16,
    pub min_ratio_micros: i64,
    pub max_ratio_micros: i64,
    pub settlement_delay_secs: u32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantPoolWrite {
    pub group_id: i64,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub commission_bps: u16,
    #[serde(default)]
    pub commission_inherit_global: bool,
    pub min_ratio_micros: i64,
    pub max_ratio_micros: i64,
    pub settlement_delay_secs: u32,
}

/// 商户模型自己的价格定义。缺省时沿用平台模型目录价格并应用商户倍率。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantModelPricing {
    pub mode: MerchantModelPricingMode,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub cache_read: Option<String>,
    #[serde(default)]
    pub cache_write_5m: Option<String>,
    #[serde(default)]
    pub cache_write_1h: Option<String>,
    #[serde(default)]
    pub request_price: Option<String>,
    #[serde(default)]
    pub expression: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MerchantModelPricingMode {
    /// 明确沿用平台价格目录。
    Platform,
    /// 五类 Token 单价，单位为美元/百万 Token。
    PerToken,
    /// 固定美元/请求价格。
    PerCall,
    /// 显式免费。
    Free,
    /// 使用受限计费表达式。
    Expression,
}

impl MerchantModelPricing {
    pub fn validate(&self) -> Result<(), MerchantPoolError> {
        let values = [
            self.input.as_deref(),
            self.output.as_deref(),
            self.cache_read.as_deref(),
            self.cache_write_5m.as_deref(),
            self.cache_write_1h.as_deref(),
        ];
        if values
            .into_iter()
            .flatten()
            .any(|value| !valid_pricing_decimal(value))
            || self
                .request_price
                .as_deref()
                .is_some_and(|value| !valid_pricing_decimal(value))
            || self
                .expression
                .as_deref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 8192)
        {
            return Err(MerchantPoolError::InvalidInput);
        }
        match self.mode {
            MerchantModelPricingMode::Platform => {
                if values.into_iter().flatten().next().is_some()
                    || self.request_price.is_some()
                    || self.expression.is_some()
                {
                    return Err(MerchantPoolError::InvalidInput);
                }
            }
            MerchantModelPricingMode::PerToken => {
                if self.request_price.is_some() || self.expression.is_some() {
                    return Err(MerchantPoolError::InvalidInput);
                }
            }
            MerchantModelPricingMode::PerCall => {
                if self
                    .request_price
                    .as_deref()
                    .and_then(|value| value.parse::<rust_decimal::Decimal>().ok())
                    .is_none_or(|value| value <= rust_decimal::Decimal::ZERO)
                    || values.into_iter().flatten().next().is_some()
                    || self.expression.is_some()
                {
                    return Err(MerchantPoolError::InvalidInput);
                }
            }
            MerchantModelPricingMode::Free => {
                if values.into_iter().flatten().any(|value| {
                    value
                        .parse::<rust_decimal::Decimal>()
                        .is_ok_and(|price| !price.is_zero())
                }) || self.request_price.is_some()
                    || self.expression.is_some()
                {
                    return Err(MerchantPoolError::InvalidInput);
                }
            }
            MerchantModelPricingMode::Expression => {
                if values.into_iter().flatten().any(|value| {
                    value
                        .parse::<rust_decimal::Decimal>()
                        .is_ok_and(|price| !price.is_zero())
                }) || self.request_price.is_some()
                    || self.expression.as_deref().is_none_or(str::is_empty)
                {
                    return Err(MerchantPoolError::InvalidInput);
                }
            }
        }
        Ok(())
    }
}

fn valid_pricing_decimal(value: &str) -> bool {
    let Ok(price) = value.parse::<rust_decimal::Decimal>() else {
        return false;
    };
    if price < rust_decimal::Decimal::ZERO || price.scale() > 28 {
        return false;
    }
    let significant = price
        .normalize()
        .mantissa()
        .unsigned_abs()
        .to_string()
        .len();
    significant.saturating_sub(price.normalize().scale() as usize) <= 10
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantModelWrite {
    pub model: String,
    pub upstream_model: String,
    pub ratio_micros: Option<i64>,
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    /// 空值保持历史行为，使用平台价格目录。
    #[serde(default)]
    pub pricing: Option<MerchantModelPricing>,
}

/// Only this write-only command accepts a key. Do not derive Debug/Serialize.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantServiceWrite {
    pub pool_id: i64,
    pub name: String,
    pub description: String,
    /// Create defaults to listed. Omitted updates preserve the current setting.
    #[serde(default)]
    pub listed: Option<bool>,
    pub base_url: String,
    pub protocol: String,
    pub api_key: Option<String>,
    pub default_ratio_micros: i64,
    pub models: Vec<MerchantModelWrite>,
    pub concurrency: Option<u32>,
    pub timeout_secs: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantRevision {
    pub id: i64,
    pub revision: i64,
    pub status: String,
    pub name: String,
    pub description: String,
    pub base_url: String,
    pub protocol: String,
    pub default_ratio_micros: i64,
    pub models: Vec<MerchantModelWrite>,
    pub concurrency: Option<u32>,
    pub timeout_secs: u32,
    pub has_api_key: bool,
    pub review_reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantOwnedService {
    pub id: i64,
    pub merchant_id: i64,
    pub pool_id: i64,
    pub name: String,
    pub description: String,
    pub status: String,
    pub listed: bool,
    pub published: Option<MerchantRevision>,
    pub draft: Option<MerchantRevision>,
    pub active_price: Option<crate::MerchantServicePrice>,
    pub maintenance: Vec<crate::MerchantMaintenance>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantServiceListingWrite {
    pub listed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantModelPrices {
    pub currency: String,
    pub unit: String,
    pub billing_mode: String,
    pub video_per_second: Option<MerchantVideoPrices>,
    pub input: String,
    pub output: String,
    pub cache_read: String,
    pub cache_write_5m: String,
    pub cache_write_1h: String,
    pub request_price: Option<String>,
    #[serde(default)]
    pub billing_expression: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantVideoPrices {
    pub p480: String,
    pub p720: String,
    pub p1080: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantModelOffer {
    pub model: String,
    pub provider: Option<String>,
    pub sales_ratio_micros: i64,
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub prices: Option<MerchantModelPrices>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantCatalogService {
    pub id: i64,
    pub pool_id: i64,
    pub merchant_id: i64,
    pub merchant_name: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub protocol: String,
    pub revision: i64,
    pub price_version: i64,
    pub maintenance: Vec<crate::MerchantMaintenance>,
    pub models: Vec<MerchantModelOffer>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantCatalogProvider {
    pub name: Option<String>,
    pub model_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantCatalogProviderSummary {
    pub providers: Vec<MerchantCatalogProvider>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantPage<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantListQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub model: Option<String>,
    /// JSON array of exact provider names; null selects models without provider metadata.
    pub provider: Option<String>,
    pub protocol: Option<String>,
    pub capability: Option<String>,
    pub min_ratio_micros: Option<i64>,
    pub max_ratio_micros: Option<i64>,
    pub window_seconds: Option<u32>,
    pub streaming: Option<bool>,
    pub search: Option<String>,
    pub status: Option<String>,
    pub sort: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantReview {
    pub decision: String,
    pub reason: String,
    pub expected_revision: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantPoolSettings {
    pub commission_bps: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantPoolTerms {
    pub commission_bps: Option<u16>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantQuoteWrite {
    pub pool_id: i64,
    pub default_ratio_micros: i64,
    pub models: Vec<MerchantModelWrite>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MerchantQuoteModel {
    pub model: String,
    pub sales_ratio_micros: i64,
    pub base_prices: MerchantModelPrices,
    pub prices: MerchantModelPrices,
    pub net_prices: MerchantModelPrices,
}

#[derive(Clone, Debug, Serialize)]
pub struct MerchantQuote {
    pub models: Vec<MerchantQuoteModel>,
    pub effective_commission_bps: u16,
}

#[derive(Clone, Debug)]
pub struct MerchantRuntimeCandidate {
    pub pool_id: i64,
    pub group_id: i64,
    pub merchant_id: i64,
    pub service_id: i64,
    pub revision_id: i64,
    pub price_version: i64,
    pub channel_id: i64,
    pub sales_ratio_micros: i64,
    pub commission_bps: u16,
    pub settlement_delay_secs: u32,
    pub protocol: String,
    pub model: MerchantModelWrite,
}

pub fn validate_merchant_text(value: &str, maximum: usize, empty: bool) -> bool {
    (empty || !value.is_empty())
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

/// Merchant introductions may contain line breaks, while names and identifiers remain single-line.
pub fn validate_merchant_description(value: &str, maximum: usize, empty: bool) -> bool {
    if !(empty || !value.is_empty()) || value.len() > maximum || value.trim() != value {
        return false;
    }
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\n' => {}
            '\r' if chars.peek() == Some(&'\n') => {
                chars.next();
            }
            character if character.is_control() => return false,
            _ => {}
        }
    }
    true
}

impl MerchantPoolWrite {
    pub fn validate(&self) -> Result<(), MerchantPoolError> {
        if self.group_id <= 0
            || !validate_merchant_text(&self.name, 128, false)
            || !validate_merchant_description(&self.description, 2048, true)
            || self.commission_bps > 10_000
            || self.min_ratio_micros <= 0
            || self.max_ratio_micros < self.min_ratio_micros
            || self.max_ratio_micros > 100_000_000
            || self.settlement_delay_secs > 90 * 86_400
        {
            return Err(MerchantPoolError::InvalidInput);
        }
        Ok(())
    }
}

impl MerchantServiceWrite {
    pub fn validate(&self) -> Result<(), MerchantPoolError> {
        if self.pool_id <= 0
            || !validate_merchant_text(&self.name, 128, false)
            || !validate_merchant_description(&self.description, 2048, true)
            || !matches!(
                self.protocol.as_str(),
                "openai_chat"
                    | "openai_responses"
                    | "openai_images"
                    | "openai_audio"
                    | "openai_speech"
                    | "xai_video"
            )
            || self.default_ratio_micros <= 0
            || self.default_ratio_micros > 100_000_000
            || self.models.is_empty()
            || self.models.len() > MAX_MERCHANT_SERVICE_MODELS
            || !(1..=300).contains(&self.timeout_secs)
            || self.concurrency.is_some_and(|n| n == 0 || n > 10_000)
            || self.api_key.as_ref().is_some_and(|key| {
                key.is_empty() || key.len() > 8192 || key.chars().any(char::is_control)
            })
        {
            return Err(MerchantPoolError::InvalidInput);
        }
        let mut names = std::collections::BTreeSet::new();
        for model in &self.models {
            let video = model.model == "grok-imagine-video"
                || model.model == "grok-imagine-video-1.5"
                || model.model.starts_with("grok-imagine-video-1.5-");
            if (self.protocol == "xai_video") != video
                || (video && model.upstream_model != model.model)
                || (video
                    && model
                        .pricing
                        .as_ref()
                        .is_some_and(|pricing| pricing.mode != MerchantModelPricingMode::Platform))
            {
                return Err(MerchantPoolError::InvalidInput);
            }
            if !validate_merchant_text(&model.model, 255, false)
                || !validate_merchant_text(&model.upstream_model, 255, false)
                || !names.insert(&model.model)
                || model
                    .ratio_micros
                    .is_some_and(|n| n <= 0 || n > 100_000_000)
                || model.context_window == Some(0)
                || model.max_output_tokens == Some(0)
                || (model.supports_vision
                    && !matches!(self.protocol.as_str(), "openai_chat" | "openai_responses"))
                || (!matches!(self.protocol.as_str(), "openai_chat" | "openai_responses")
                    && (model.supports_streaming || model.supports_tools))
                || matches!((model.context_window,model.max_output_tokens),(Some(c),Some(o)) if o > c)
                || model
                    .pricing
                    .as_ref()
                    .is_some_and(|pricing| pricing.validate().is_err())
            {
                return Err(MerchantPoolError::InvalidInput);
            }
        }
        Ok(())
    }
}
