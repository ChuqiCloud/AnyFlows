use std::{
    collections::BTreeMap,
    fmt,
    str::FromStr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_admin::{
    MAX_MODEL_PRICE_SOURCE_TARGETS, ModelPriceRuntimeRefreshError, ModelPriceRuntimeRefreshFuture,
    ModelPriceRuntimeRefresher, ModelPriceSourceCandidate, ModelPriceSourceDiscoverer,
    ModelPriceSourceDiscoveryError, ModelPriceSourceDiscoveryFuture, ModelPriceSourcePreview,
    ModelPriceSourceTarget,
};
use af_billing::ModelPriceCache;
use af_httpclient::{
    HeaderMap, HeaderName, HeaderValue, HttpClientProvider, HttpTransportError, Method,
};
use rust_decimal::Decimal;
use serde::Deserialize;

const MODELS_DEV_ENDPOINT: &str = "https://models.dev/api.json";
const MODELS_DEV_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_MODELS_DEV_RESPONSE_BYTES: usize = 8 * 1_024 * 1_024;
const MAX_MODELS_DEV_PROVIDERS: usize = 512;
const MAX_MODELS_DEV_MODELS: usize = 20_000;

type Clock = Arc<dyn Fn() -> Option<i64> + Send + Sync>;

/// 使用生产受控 HTTP Client 读取固定 models.dev 公开价格目录。
#[derive(Clone)]
pub(crate) struct ModelsDevPriceDiscoverer {
    clients: HttpClientProvider,
    clock: Clock,
}

impl ModelsDevPriceDiscoverer {
    #[must_use]
    pub(crate) fn new(clients: HttpClientProvider) -> Self {
        Self {
            clients,
            clock: Arc::new(current_unix_timestamp),
        }
    }

    async fn discover_inner(
        &self,
        targets: Vec<ModelPriceSourceTarget>,
    ) -> Result<ModelPriceSourcePreview, ModelPriceSourceDiscoveryError> {
        if targets.len() > MAX_MODEL_PRICE_SOURCE_TARGETS {
            return Err(ModelPriceSourceDiscoveryError::CandidateLimitExceeded);
        }
        let client = self
            .clients
            .get(Some(MODELS_DEV_TIMEOUT))
            .map_err(|_| ModelPriceSourceDiscoveryError::Unavailable)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
        let response = client
            .execute(Method::GET, MODELS_DEV_ENDPOINT, headers, None)
            .await
            .map_err(map_transport_error)?;
        if !response.status().is_success()
            || response
                .headers()
                .get(HeaderName::from_static("content-type"))
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| !value.to_ascii_lowercase().contains("application/json"))
        {
            return Err(ModelPriceSourceDiscoveryError::Unavailable);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_MODELS_DEV_RESPONSE_BYTES as u64)
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
            if next_len > MAX_MODELS_DEV_RESPONSE_BYTES {
                return Err(ModelPriceSourceDiscoveryError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let fetched_at = (self.clock)().ok_or(ModelPriceSourceDiscoveryError::InvalidResponse)?;
        parse_models_dev(&body, targets, fetched_at, revision)
    }
}

impl ModelPriceSourceDiscoverer for ModelsDevPriceDiscoverer {
    fn discover<'a>(
        &'a self,
        targets: Vec<ModelPriceSourceTarget>,
    ) -> ModelPriceSourceDiscoveryFuture<'a> {
        Box::pin(async move {
            let result = self.discover_inner(targets).await;
            if let Err(error) = result.as_ref() {
                tracing::debug!(
                    error_kind = source_error_kind(*error),
                    "公开模型价格源读取失败"
                );
            }
            result
        })
    }
}

impl fmt::Debug for ModelsDevPriceDiscoverer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModelsDevPriceDiscoverer(<固定公开来源>)")
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

    let mut candidates = Vec::new();
    for target in targets {
        let Some(provider) = providers.get(target.provider()) else {
            continue;
        };
        if provider
            .id
            .as_deref()
            .is_some_and(|id| id != target.provider())
        {
            return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
        }
        let Some(model) = provider.models.get(target.model()) else {
            continue;
        };
        if model.id.as_deref().is_some_and(|id| id != target.model()) {
            return Err(ModelPriceSourceDiscoveryError::InvalidResponse);
        }
        let Some(cost) = model.cost.as_ref() else {
            continue;
        };
        candidates.push(ModelPriceSourceCandidate::new(
            target.model().to_owned(),
            target.provider().to_owned(),
            provider.name.clone(),
            model
                .id
                .clone()
                .unwrap_or_else(|| target.model().to_owned()),
            model.name.clone(),
            model.last_updated.clone(),
            [
                parse_decimal(cost.input.as_ref())?,
                parse_decimal(cost.output.as_ref())?,
                parse_decimal(cost.cache_read.as_ref())?,
                parse_decimal(cost.cache_write.as_ref())?,
            ],
            cost.tiers.as_ref().is_some_and(nonempty_json) || cost.context_over_200k.is_some(),
            model.status.as_deref() == Some("deprecated"),
        )?);
    }
    candidates.sort_by(|left, right| left.model().cmp(right.model()));
    ModelPriceSourcePreview::new("models_dev", fetched_at, revision, candidates)
}

fn parse_decimal(
    number: Option<&serde_json::Number>,
) -> Result<Option<Decimal>, ModelPriceSourceDiscoveryError> {
    number
        .map(|number| {
            let value = number.to_string();
            Decimal::from_str_exact(&value)
                .or_else(|_| Decimal::from_scientific(&value))
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
    cost: Option<ModelsDevCost>,
}

#[derive(Deserialize)]
struct ModelsDevCost {
    input: Option<serde_json::Number>,
    output: Option<serde_json::Number>,
    cache_read: Option<serde_json::Number>,
    cache_write: Option<serde_json::Number>,
    tiers: Option<serde_json::Value>,
    context_over_200k: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_matches_exact_provider_and_preserves_decimal_costs() {
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
                ModelPriceSourceTarget::new("gpt-test".to_owned(), "openai".to_owned()).unwrap(),
                ModelPriceSourceTarget::new("gpt-test".to_owned(), "other".to_owned()).unwrap(),
            ],
            1_700_000_000,
            Some("revision-1".to_owned()),
        )
        .unwrap();

        assert_eq!(preview.candidates().len(), 1);
        let candidate = &preview.candidates()[0];
        assert_eq!(candidate.provider(), "openai");
        assert_eq!(
            candidate.costs()[0],
            Some(Decimal::from_str("1.234567890123456789").unwrap())
        );
        assert!(candidate.has_tiered_pricing());
        assert!(candidate.costs()[3].is_none());
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
    }
}
