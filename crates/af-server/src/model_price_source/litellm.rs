use std::collections::BTreeMap;

use af_admin::{
    ModelPriceSourceCandidate, ModelPriceSourceDiscoveryError, ModelPriceSourcePreview,
    ModelPriceSourceTarget,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{self, value::RawValue};

/// 解析 LiteLLM 官方顶层模型价格映射，只返回本地权威身份的精确匹配。
pub(super) fn parse(
    body: &[u8],
    targets: Vec<ModelPriceSourceTarget>,
    fetched_at: i64,
    revision: Option<String>,
) -> Result<ModelPriceSourcePreview, ModelPriceSourceDiscoveryError> {
    // 官方文件当前带 UTF-8 BOM；只剥离这个明确的编码标记，不放宽其余正文边界。
    let body = body.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(body);
    let entries = serde_json::from_slice::<BTreeMap<String, LiteLlmEntry>>(body)
        .map_err(|_| ModelPriceSourceDiscoveryError::InvalidResponse)?;
    if entries.is_empty() {
        return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
    }
    if entries.len() > super::MAX_LITELLM_MODELS {
        return Err(ModelPriceSourceDiscoveryError::CandidateLimitExceeded);
    }

    let mut candidates = Vec::new();
    for target in targets {
        let Some(entry) = entries.get(target.model()) else {
            continue;
        };
        let Some(provider) = entry.litellm_provider.as_deref() else {
            continue;
        };
        // LiteLLM 同一模型名可能有多个 provider；不做别名或最低价回退。
        if provider != target.provider() {
            continue;
        }

        let costs = [
            per_million(entry.input_cost_per_token.as_deref())?,
            per_million(entry.output_cost_per_token.as_deref())?,
            per_million(entry.cache_read_input_token_cost.as_deref())?,
            None,
        ];
        let cache_creation = [
            per_million(entry.cache_creation_input_token_cost.as_deref())?,
            per_million(entry.cache_creation_input_token_cost_above_1hr.as_deref())?,
        ];
        if costs.iter().all(Option::is_none) && cache_creation.iter().all(Option::is_none) {
            continue;
        }

        let candidate = ModelPriceSourceCandidate::new(
            target.model().to_owned(),
            target.provider().to_owned(),
            provider.to_owned(),
            target.model().to_owned(),
            "LiteLLM".to_owned(),
            None,
            None,
            costs,
            has_tiered_pricing(&entry.extra),
            false,
        )?
        .with_cache_creation_prices(cache_creation)?
        .with_source_deprecation_date(entry.deprecation_date.clone())?;
        candidates.push(candidate);
    }
    candidates.sort_by(|left, right| left.model().cmp(right.model()));
    ModelPriceSourcePreview::new("litellm", fetched_at, revision, candidates)
}

fn per_million(
    number: Option<&RawValue>,
) -> Result<Option<Decimal>, ModelPriceSourceDiscoveryError> {
    super::parse_decimal(number)?.map_or(Ok(None), |value| {
        value
            .checked_mul(Decimal::from(1_000_000_u32))
            .ok_or(ModelPriceSourceDiscoveryError::InvalidResponse)
            .map(Some)
    })
}

fn has_tiered_pricing(extra: &BTreeMap<String, serde_json::Value>) -> bool {
    extra.keys().any(|key| {
        key.contains("cost")
            && (key.contains("_above_")
                || key.ends_with("_priority")
                || key.ends_with("_flex")
                || key.ends_with("_batch")
                || key.ends_with("_batches")
                || key.contains("long_context"))
    })
}

#[derive(Deserialize)]
struct LiteLlmEntry {
    input_cost_per_token: Option<Box<RawValue>>,
    output_cost_per_token: Option<Box<RawValue>>,
    cache_read_input_token_cost: Option<Box<RawValue>>,
    cache_creation_input_token_cost: Option<Box<RawValue>>,
    cache_creation_input_token_cost_above_1hr: Option<Box<RawValue>>,
    litellm_provider: Option<String>,
    deprecation_date: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(provider: &str) -> Vec<ModelPriceSourceTarget> {
        vec![ModelPriceSourceTarget::new("claude-test".to_owned(), provider.to_owned()).unwrap()]
    }

    #[test]
    fn parses_per_token_prices_into_exact_million_token_cache_dimensions() {
        let preview = parse(
            br#"{
              "claude-test": {
                "input_cost_per_token": 0.000003,
                "output_cost_per_token": 1.5e-5,
                "cache_read_input_token_cost": 0.0000003,
                "cache_creation_input_token_cost": 3.75e-6,
                "cache_creation_input_token_cost_above_1hr": 6e-6,
                "litellm_provider": "anthropic"
              }
            }"#,
            target("anthropic"),
            1_700_000_000,
            Some("etag-1".to_owned()),
        )
        .unwrap();

        let candidate = &preview.candidates()[0];
        assert_eq!(candidate.costs()[0], Some(Decimal::from(3_u32)));
        assert_eq!(candidate.costs()[1], Some(Decimal::from(15_u32)));
        assert_eq!(candidate.costs()[2], Some(Decimal::new(3, 1)));
        assert_eq!(
            candidate.cache_creation_prices(),
            [Some(Decimal::new(375, 2)), Some(Decimal::new(6, 0))]
        );
    }

    #[test]
    fn requires_exact_provider_and_marks_tiered_or_deprecated_entries() {
        let preview = parse(
            br#"{
              "claude-test": {
                "input_cost_per_token": 0.000003,
                "litellm_provider": "anthropic",
                "input_cost_per_token_above_200k_tokens": 0.000006,
                "deprecation_date": "2026-12-31"
              }
            }"#,
            target("anthropic"),
            1,
            None,
        )
        .unwrap();
        let candidate = &preview.candidates()[0];
        assert!(candidate.has_tiered_pricing());
        assert_eq!(candidate.source_deprecation_date(), Some("2026-12-31"));

        let empty = parse(
            br#"{"claude-test":{"input_cost_per_token":0.000003,"litellm_provider":"vertex_ai"}}"#,
            target("anthropic"),
            1,
            None,
        )
        .unwrap();
        assert!(empty.candidates().is_empty());
    }

    #[test]
    fn rejects_negative_price_after_exact_conversion() {
        let result = parse(
            br#"{"claude-test":{"input_cost_per_token":-0.000003,"litellm_provider":"anthropic"}}"#,
            target("anthropic"),
            1,
            None,
        );
        assert!(matches!(
            result,
            Err(ModelPriceSourceDiscoveryError::InvalidResponse)
        ));
    }

    #[test]
    fn accepts_the_official_utf8_bom_without_relaxing_json_validation() {
        let mut body = b"\xEF\xBB\xBF{".to_vec();
        body.extend_from_slice(
            br#""claude-test":{"input_cost_per_token":0.000003,"litellm_provider":"anthropic"}}"#,
        );
        let preview = parse(&body, target("anthropic"), 1, None).unwrap();
        assert_eq!(preview.candidates().len(), 1);
    }
}
