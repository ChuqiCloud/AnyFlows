use std::{
    collections::BTreeMap,
    fmt,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_admin::{
    MAX_MODEL_PRICE_SOURCE_TARGETS, ModelPriceRuntimeRefreshError, ModelPriceRuntimeRefreshFuture,
    ModelPriceRuntimeRefresher, ModelPriceSourceCandidate, ModelPriceSourceDiscoverer,
    ModelPriceSourceDiscoveryError, ModelPriceSourceDiscoveryFuture, ModelPriceSourceKind,
    ModelPriceSourcePreview, ModelPriceSourceTarget,
};
use af_billing::ModelPriceCache;
use af_httpclient::{
    HeaderMap, HeaderName, HeaderValue, HttpClientProvider, HttpTransportError, Method,
};
use rust_decimal::Decimal;
use serde::Deserialize;

mod litellm;

const MODELS_DEV_ENDPOINT: &str = "https://models.dev/api.json";
const LITELLM_ENDPOINT: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
const PUBLIC_SOURCE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PUBLIC_SOURCE_RESPONSE_BYTES: usize = 8 * 1_024 * 1_024;
const MAX_MODELS_DEV_PROVIDERS: usize = 512;
const MAX_MODELS_DEV_MODELS: usize = 20_000;
const MAX_LITELLM_MODELS: usize = 50_000;

type Clock = Arc<dyn Fn() -> Option<i64> + Send + Sync>;

struct SourcePayload {
    body: Vec<u8>,
    revision: Option<String>,
}

/// 使用生产受控 HTTP Client 读取固定公开价格目录。
#[derive(Clone)]
pub(crate) struct ModelPriceDiscoverer {
    clients: HttpClientProvider,
    clock: Clock,
}

impl ModelPriceDiscoverer {
    #[must_use]
    pub(crate) fn new(clients: HttpClientProvider) -> Self {
        Self {
            clients,
            clock: Arc::new(current_unix_timestamp),
        }
    }

    async fn discover_inner(
        &self,
        source: ModelPriceSourceKind,
        targets: Vec<ModelPriceSourceTarget>,
    ) -> Result<ModelPriceSourcePreview, ModelPriceSourceDiscoveryError> {
        if targets.len() > MAX_MODEL_PRICE_SOURCE_TARGETS {
            return Err(ModelPriceSourceDiscoveryError::CandidateLimitExceeded);
        }
        let (endpoint, allow_text_plain) = match source {
            ModelPriceSourceKind::ModelsDev => (MODELS_DEV_ENDPOINT, false),
            ModelPriceSourceKind::LiteLlm => (LITELLM_ENDPOINT, true),
        };
        let payload = self.fetch_source(endpoint, allow_text_plain).await?;
        let fetched_at = (self.clock)().ok_or(ModelPriceSourceDiscoveryError::InvalidResponse)?;
        match source {
            ModelPriceSourceKind::ModelsDev => {
                parse_models_dev(&payload.body, targets, fetched_at, payload.revision)
            }
            ModelPriceSourceKind::LiteLlm => {
                litellm::parse(&payload.body, targets, fetched_at, payload.revision)
            }
        }
    }

    async fn fetch_source(
        &self,
        endpoint: &str,
        allow_text_plain: bool,
    ) -> Result<SourcePayload, ModelPriceSourceDiscoveryError> {
        let client = self
            .clients
            .get(Some(PUBLIC_SOURCE_TIMEOUT))
            .map_err(|_| ModelPriceSourceDiscoveryError::Unavailable)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
        let response = client
            .execute(Method::GET, endpoint, headers, None)
            .await
            .map_err(map_transport_error)?;
        let content_type = response
            .headers()
            .get(HeaderName::from_static("content-type"))
            .and_then(|value| value.to_str().ok())
            .map(str::to_ascii_lowercase);
        let content_type_valid = content_type.as_deref().is_some_and(|value| {
            value.contains("application/json") || (allow_text_plain && value.contains("text/plain"))
        });
        if !response.status().is_success() || !content_type_valid {
            return Err(ModelPriceSourceDiscoveryError::Unavailable);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PUBLIC_SOURCE_RESPONSE_BYTES as u64)
        {
            return Err(ModelPriceSourceDiscoveryError::ResponseTooLarge);
        }
        let revision = response
            .headers()
            .get(HeaderName::from_static("etag"))
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut stream = response.into_bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            let chunk = chunk.map_err(map_transport_error)?;
            let next_len = body
                .len()
                .checked_add(chunk.len())
                .ok_or(ModelPriceSourceDiscoveryError::ResponseTooLarge)?;
            if next_len > MAX_PUBLIC_SOURCE_RESPONSE_BYTES {
                return Err(ModelPriceSourceDiscoveryError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(SourcePayload { body, revision })
    }
}

impl ModelPriceSourceDiscoverer for ModelPriceDiscoverer {
    fn discover<'a>(
        &'a self,
        source: ModelPriceSourceKind,
        targets: Vec<ModelPriceSourceTarget>,
    ) -> ModelPriceSourceDiscoveryFuture<'a> {
        Box::pin(async move {
            let result = self.discover_inner(source, targets).await;
            if let Err(error) = result.as_ref() {
                tracing::debug!(
                    source = source.as_str(),
                    error_kind = source_error_kind(*error),
                    "公开模型价格源读取失败"
                );
            }
            result
        })
    }
}

impl fmt::Debug for ModelPriceDiscoverer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModelPriceDiscoverer(<固定公开来源>)")
    }
}

/// 将正式写入后的数据库价格发布到当前进程不可变快照。
#[derive(Clone)]
pub(crate) struct RuntimeModelPriceRefresher {
    cache: ModelPriceCache,
}

impl RuntimeModelPriceRefresher {
    #[must_use]
    pub(crate) const fn new(cache: ModelPriceCache) -> Self {
        Self { cache }
    }
}

impl ModelPriceRuntimeRefresher for RuntimeModelPriceRefresher {
    fn refresh<'a>(&'a self) -> ModelPriceRuntimeRefreshFuture<'a> {
        Box::pin(async move {
            self.cache
                .invalidate()
                .map_err(|_| ModelPriceRuntimeRefreshError)?;
            self.cache
                .refresh_if_stale()
                .await
                .map(|_| ())
                .map_err(|_| ModelPriceRuntimeRefreshError)
        })
    }
}

impl fmt::Debug for RuntimeModelPriceRefresher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeModelPriceRefresher(<受控>)")
    }
}

fn parse_models_dev(
    body: &[u8],
    targets: Vec<ModelPriceSourceTarget>,
    fetched_at: i64,
    revision: Option<String>,
) -> Result<ModelPriceSourcePreview, ModelPriceSourceDiscoveryError> {
    let providers = serde_json::from_slice::<BTreeMap<String, ModelsDevProvider>>(body)
        .map_err(|_| ModelPriceSourceDiscoveryError::InvalidResponse)?;
    if providers.is_empty() || providers.len() > MAX_MODELS_DEV_PROVIDERS {
        return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
    }
    let model_count = providers.values().try_fold(0_usize, |count, provider| {
        count.checked_add(provider.models.len())
    });
    if model_count.is_none_or(|count| count > MAX_MODELS_DEV_MODELS) {
        return Err(ModelPriceSourceDiscoveryError::CandidateLimitExceeded);
    }
    if providers.iter().any(|(provider_id, provider)| {
        provider
            .id
            .as_deref()
            .is_some_and(|id| id != provider_id.as_str())
    }) {
        return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
    }

    let targets = targets
        .into_iter()
        .map(|target| (target.model().to_owned(), target))
        .collect::<BTreeMap<_, _>>();
    let mut candidates = BTreeMap::<String, ModelPriceSourceCandidate>::new();
    for (provider_id, provider) in &providers {
        for (source_model_id, model) in &provider.models {
            let Some(target) = targets.get(source_model_id) else {
                continue;
            };
            if model
                .id
                .as_deref()
                .is_some_and(|id| id != source_model_id.as_str())
            {
                return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
            }
            let context_window = model.limit.as_ref().and_then(|limit| limit.context);
            if model.cost.is_none() && context_window.is_none() {
                continue;
            }
            let cost = model.cost.as_ref();
            let candidate = ModelPriceSourceCandidate::new(
                target.model().to_owned(),
                provider_id.clone(),
                provider.name.clone(),
                source_model_id.clone(),
                model.name.clone(),
                model.last_updated.clone(),
                context_window,
                [
                    parse_decimal(cost.and_then(|value| value.input.as_deref()))?,
                    parse_decimal(cost.and_then(|value| value.output.as_deref()))?,
                    parse_decimal(cost.and_then(|value| value.cache_read.as_deref()))?,
                    parse_decimal(cost.and_then(|value| value.cache_write.as_deref()))?,
                ],
                cost.is_some_and(|value| {
                    value.tiers.as_ref().is_some_and(nonempty_json)
                        || value.context_over_200k.is_some()
                }),
                model.status.as_deref() == Some("deprecated"),
            )?;
            match candidates.get(target.model()) {
                Some(current) if !models_dev_candidate_is_better(&candidate, current) => {}
                _ => {
                    candidates.insert(target.model().to_owned(), candidate);
                }
            }
        }
    }
    ModelPriceSourcePreview::new(
        "models_dev",
        fetched_at,
        revision,
        candidates.into_values().collect(),
    )
}

fn models_dev_candidate_is_better(
    candidate: &ModelPriceSourceCandidate,
    current: &ModelPriceSourceCandidate,
) -> bool {
    let candidate_input = candidate.costs()[0];
    let current_input = current.costs()[0];
    let candidate_has_nonzero_input = candidate_input.is_some_and(|price| !price.is_zero());
    let current_has_nonzero_input = current_input.is_some_and(|price| !price.is_zero());
    match (candidate_has_nonzero_input, current_has_nonzero_input) {
        (true, false) => true,
        (false, true) => false,
        _ => match (candidate_input, current_input) {
            (Some(candidate_price), Some(current_price)) => {
                candidate_price < current_price
                    || (candidate_price == current_price
                        && candidate.provider() < current.provider())
            }
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => candidate.provider() < current.provider(),
        },
    }
}

fn parse_decimal(
    number: Option<&serde_json::value::RawValue>,
) -> Result<Option<Decimal>, ModelPriceSourceDiscoveryError> {
    number
        .map(|number| {
            // 公开价必须从原始 JSON 数字文本构造，避免先经过浮点数而损失精度。
            let value = number.get();
            Decimal::from_str_exact(value)
                .or_else(|_| Decimal::from_scientific(value))
                .map_err(|_| ModelPriceSourceDiscoveryError::InvalidResponse)
                .and_then(|decimal| {
                    if decimal < Decimal::ZERO {
                        Err(ModelPriceSourceDiscoveryError::InvalidResponse)
                    } else {
                        Ok(decimal)
                    }
                })
        })
        .transpose()
}

fn nonempty_json(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(items) => !items.is_empty(),
        serde_json::Value::Object(items) => !items.is_empty(),
        serde_json::Value::Null => false,
        _ => true,
    }
}

fn map_transport_error(error: HttpTransportError) -> ModelPriceSourceDiscoveryError {
    if error.is_timeout() {
        ModelPriceSourceDiscoveryError::Timeout
    } else {
        ModelPriceSourceDiscoveryError::Unavailable
    }
}

const fn source_error_kind(error: ModelPriceSourceDiscoveryError) -> &'static str {
    match error {
        ModelPriceSourceDiscoveryError::Timeout => "model_price_source_timeout",
        ModelPriceSourceDiscoveryError::Unavailable => "model_price_source_unavailable",
        ModelPriceSourceDiscoveryError::ResponseTooLarge => "model_price_source_response_too_large",
        ModelPriceSourceDiscoveryError::InvalidResponse => "model_price_source_invalid_response",
        ModelPriceSourceDiscoveryError::CandidateLimitExceeded => {
            "model_price_source_candidate_limit_exceeded"
        }
    }
}

fn current_unix_timestamp() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
}

#[derive(Deserialize)]
struct ModelsDevProvider {
    id: Option<String>,
    name: String,
    #[serde(default)]
    models: BTreeMap<String, ModelsDevModel>,
}

#[derive(Deserialize)]
struct ModelsDevModel {
    id: Option<String>,
    name: String,
    last_updated: Option<String>,
    status: Option<String>,
    limit: Option<ModelsDevLimit>,
    cost: Option<ModelsDevCost>,
}

#[derive(Deserialize)]
struct ModelsDevLimit {
    context: Option<i64>,
}

#[derive(Deserialize)]
struct ModelsDevCost {
    input: Option<Box<serde_json::value::RawValue>>,
    output: Option<Box<serde_json::value::RawValue>>,
    cache_read: Option<Box<serde_json::value::RawValue>>,
    cache_write: Option<Box<serde_json::value::RawValue>>,
    tiers: Option<serde_json::Value>,
    context_over_200k: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_matches_local_model_ids_across_providers_and_preserves_decimal_costs() {
        let preview = parse_models_dev(
            br#"{
              "openai": {
                "id": "openai",
                "name": "OpenAI",
                "models": {
                  "gpt-test": {
                    "id": "gpt-test",
                    "name": "GPT Test",
                    "last_updated": "2026-07-31",
                    "limit": {"context": 400000},
                    "cost": {
                      "input": 1.234567890123456789,
                      "output": 9.5,
                      "cache_read": 0.123,
                      "tiers": [{"input": 2}]
                    }
                  }
                }
              }
            }"#,
            vec![
                ModelPriceSourceTarget::new("gpt-test".to_owned(), "local-alias".to_owned())
                    .unwrap(),
            ],
            1_700_000_000,
            Some("revision-1".to_owned()),
        )
        .unwrap();

        assert_eq!(preview.candidates().len(), 1);
        let candidate = &preview.candidates()[0];
        assert_eq!(candidate.provider(), "openai");
        assert_eq!(candidate.context_window(), Some(400_000));
        assert_eq!(
            candidate.costs()[0],
            Some(Decimal::from_str_exact("1.234567890123456789").unwrap())
        );
        assert!(candidate.has_tiered_pricing());
        assert!(candidate.costs()[3].is_none());
    }

    #[test]
    fn parser_prefers_the_lowest_nonzero_input_price_for_duplicate_model_ids() {
        let preview = parse_models_dev(
            br#"{
              "openai": {"name":"OpenAI","models":{"shared-model":{"name":"Shared","cost":{"input":2}}}},
              "vendor": {"name":"Vendor","models":{"shared-model":{"name":"Shared","cost":{"input":1.5}}}}
            }"#,
            vec![ModelPriceSourceTarget::new("shared-model".to_owned(), "local".to_owned()).unwrap()],
            1,
            None,
        )
        .unwrap();

        assert_eq!(preview.candidates().len(), 1);
        assert_eq!(preview.candidates()[0].provider(), "vendor");
        assert_eq!(
            preview.candidates()[0].costs()[0],
            Some(Decimal::new(15, 1))
        );
    }

    #[test]
    fn parser_rejects_negative_cost_and_mismatched_ids() {
        let target = || {
            vec![ModelPriceSourceTarget::new("gpt-test".to_owned(), "openai".to_owned()).unwrap()]
        };
        assert!(matches!(
            parse_models_dev(
                br#"{"openai":{"id":"other","name":"OpenAI","models":{}}}"#,
                target(),
                1,
                None,
            ),
            Err(ModelPriceSourceDiscoveryError::InvalidResponse)
        ));
        assert!(matches!(
            parse_models_dev(
                br#"{"openai":{"name":"OpenAI","models":{"gpt-test":{"name":"GPT","cost":{"input":-1,"output":1}}}}}"#,
                target(),
                1,
                None,
            ),
            Err(ModelPriceSourceDiscoveryError::InvalidResponse)
        ));
        assert!(matches!(
            parse_models_dev(
                br#"{"openai":{"name":"OpenAI","models":{"gpt-test":{"name":"GPT","cost":{"input":"1.25","output":1}}}}}"#,
                target(),
                1,
                None,
            ),
            Err(ModelPriceSourceDiscoveryError::InvalidResponse)
        ));
    }

    #[test]
    fn parser_accepts_scientific_cost_without_float_rounding() {
        let preview = parse_models_dev(
            br#"{"openai":{"name":"OpenAI","models":{"gpt-test":{"name":"GPT","cost":{"input":1.234567890123456789e-3}}}}}"#,
            vec![ModelPriceSourceTarget::new("gpt-test".to_owned(), "openai".to_owned()).unwrap()],
            1,
            None,
        )
        .unwrap();

        assert_eq!(
            preview.candidates()[0].costs()[0],
            Some(Decimal::from_str_exact("0.001234567890123456789").unwrap())
        );
    }
}
