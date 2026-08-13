use af_admin::{
    ModelCatalogBillingMode, ModelCatalogCapability, ModelCatalogItem, ModelCatalogLifecycle,
    ModelCatalogModality, ModelCatalogPage, ModelCatalogPricingScope, ModelCatalogProviderSummary,
    ModelCatalogQuery, ModelCatalogReadError, ModelCatalogRuntimeStatus,
};
use af_domain::Protocol;
use axum::{
    extract::{Extension, RawQuery, State},
    response::Response,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_error::ManagementError,
    management_models::{AdminModelModalityValue, modality_values},
    management_session::no_store_json,
};

const MAX_MODEL_CATALOG_QUERY_BYTES: usize = 1_024;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ModelCatalogTokenPrices)]
pub(crate) struct ModelCatalogTokenPricesResponse {
    #[schema(pattern = "^[0-9]+(?:\\.[0-9]+)?$", max_length = 64)]
    input: String,
    #[schema(pattern = "^[0-9]+(?:\\.[0-9]+)?$", max_length = 64)]
    output: String,
    #[schema(pattern = "^[0-9]+(?:\\.[0-9]+)?$", max_length = 64)]
    cache_read: String,
    #[schema(pattern = "^[0-9]+(?:\\.[0-9]+)?$", max_length = 64)]
    cache_creation_5m: String,
    #[schema(pattern = "^[0-9]+(?:\\.[0-9]+)?$", max_length = 64)]
    cache_creation_1h: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ModelCatalogRatios)]
pub(crate) struct ModelCatalogRatiosResponse {
    #[schema(pattern = "^[0-9]+$", max_length = 19)]
    group_micros: String,
    #[schema(pattern = "^[0-9]+$", max_length = 19)]
    group_model_micros: String,
    #[schema(pattern = "^[0-9]+$", max_length = 19)]
    peak_micros: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ModelCatalogItem)]
pub(crate) struct ModelCatalogItemResponse {
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    #[schema(min_length = 1, max_length = 64)]
    provider: String,
    #[schema(max_length = 4096, required = true)]
    description: Option<String>,
    #[schema(max_length = 2048, required = true)]
    icon_url: Option<String>,
    #[schema(max_items = 32)]
    tags: Vec<String>,
    #[schema(minimum = 1, maximum = 2147483647, required = true)]
    context_window: Option<i64>,
    #[schema(min_items = 1, max_items = 4)]
    input_modalities: Vec<AdminModelModalityValue>,
    #[schema(min_items = 1, max_items = 4)]
    output_modalities: Vec<AdminModelModalityValue>,
    supports_reasoning: bool,
    supports_tool_calls: bool,
    supports_responses_compact: bool,
    lifecycle: ModelCatalogLifecycleValue,
    runtime_status: ModelCatalogRuntimeStatusValue,
    #[schema(value_type = crate::openapi::schema::ModelCatalogBillingModeSchema)]
    billing_mode: ModelCatalogBillingMode,
    #[schema(required = true)]
    prices: Option<ModelCatalogTokenPricesResponse>,
    ratios: ModelCatalogRatiosResponse,
    #[schema(minimum = 1)]
    price_version: u64,
    #[schema(max_items = 10)]
    available_protocols: Vec<ModelCatalogProtocolValue>,
}

/// 模型目录对外展示的可调用协议集合。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ModelCatalogProtocol)]
pub(crate) enum ModelCatalogProtocolValue {
    OpenaiChat,
    OpenaiResponses,
    OpenaiEmbeddings,
    OpenaiImages,
    OpenaiAudio,
    OpenaiSpeech,
    JinaRerank,
    CohereRerank,
    XaiVideo,
    Anthropic,
    Gemini,
}

impl From<Protocol> for ModelCatalogProtocolValue {
    fn from(value: Protocol) -> Self {
        match value {
            Protocol::OpenAiChat => Self::OpenaiChat,
            Protocol::OpenAiResponses => Self::OpenaiResponses,
            Protocol::OpenAiEmbeddings => Self::OpenaiEmbeddings,
            Protocol::OpenAiImages => Self::OpenaiImages,
            Protocol::OpenAiAudio => Self::OpenaiAudio,
            Protocol::OpenAiSpeech => Self::OpenaiSpeech,
            Protocol::JinaRerank => Self::JinaRerank,
            Protocol::CohereRerank => Self::CohereRerank,
            Protocol::XaiVideo => Self::XaiVideo,
            Protocol::Anthropic => Self::Anthropic,
            Protocol::Gemini => Self::Gemini,
        }
    }
}

/// 模型广场公开的闭合商品生命周期。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ModelCatalogLifecycle)]
pub(crate) enum ModelCatalogLifecycleValue {
    Active,
    Deprecated,
}

impl From<ModelCatalogLifecycle> for ModelCatalogLifecycleValue {
    fn from(value: ModelCatalogLifecycle) -> Self {
        match value {
            ModelCatalogLifecycle::Active => Self::Active,
            ModelCatalogLifecycle::Deprecated => Self::Deprecated,
        }
    }
}

/// 模型广场公开的闭合运行时状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ModelCatalogRuntimeStatus)]
pub(crate) enum ModelCatalogRuntimeStatusValue {
    NotEvaluated,
    Available,
}

impl From<ModelCatalogRuntimeStatus> for ModelCatalogRuntimeStatusValue {
    fn from(value: ModelCatalogRuntimeStatus) -> Self {
        match value {
            ModelCatalogRuntimeStatus::NotEvaluated => Self::NotEvaluated,
            ModelCatalogRuntimeStatus::Available => Self::Available,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ModelCatalogListResponse)]
pub(crate) struct ModelCatalogListResponse {
    #[schema(value_type = crate::openapi::schema::ModelCatalogPricingScopeSchema)]
    pricing_scope: ModelCatalogPricingScope,
    #[schema(max_items = 100)]
    models: Vec<ModelCatalogItemResponse>,
    #[schema(min_length = 1, max_length = 256, required = true)]
    next_cursor: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ModelCatalogProvider)]
pub(crate) struct ModelCatalogProviderResponse {
    #[schema(min_length = 1, max_length = 64)]
    name: String,
    #[schema(minimum = 1)]
    model_count: u64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ModelCatalogProviderListResponse)]
pub(crate) struct ModelCatalogProviderListResponse {
    #[schema(value_type = crate::openapi::schema::ModelCatalogPricingScopeSchema)]
    pricing_scope: ModelCatalogPricingScope,
    #[schema(max_items = 10000)]
    providers: Vec<ModelCatalogProviderResponse>,
}

/// 返回游客公开基础目录或当前登录用户默认分组的可用目录。
pub(crate) async fn list_models(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    authentication: Option<Extension<af_admin::SessionAuthentication>>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query.as_deref())?;
    let reader = state
        .model_catalog_reader
        .as_deref()
        .ok_or(ManagementError::Internal)?;
    let page = reader
        .list(
            authentication.map(|Extension(authentication)| authentication),
            &query,
        )
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(ModelCatalogListResponse::from_page(page)))
}

/// 返回与模型目录相同访问范围内的完整供应商聚合。
pub(crate) async fn list_model_providers(
    State(state): State<HttpState>,
    authentication: Option<Extension<af_admin::SessionAuthentication>>,
) -> Result<Response, ManagementError> {
    let reader = state
        .model_catalog_reader
        .as_deref()
        .ok_or(ManagementError::Internal)?;
    let summary = reader
        .providers(authentication.map(|Extension(authentication)| authentication))
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(
        ModelCatalogProviderListResponse::from_summary(summary),
    ))
}

impl ModelCatalogProviderListResponse {
    fn from_summary(summary: ModelCatalogProviderSummary) -> Self {
        Self {
            pricing_scope: summary.pricing_scope(),
            providers: summary
                .providers()
                .iter()
                .map(|provider| ModelCatalogProviderResponse {
                    name: provider.name().to_owned(),
                    model_count: provider.model_count(),
                })
                .collect(),
        }
    }
}

impl ModelCatalogListResponse {
    fn from_page(page: ModelCatalogPage) -> Self {
        Self {
            pricing_scope: page.pricing_scope(),
            models: page
                .items()
                .iter()
                .map(ModelCatalogItemResponse::from_item)
                .collect(),
            next_cursor: page.next_cursor().map(str::to_owned),
        }
    }
}

impl ModelCatalogItemResponse {
    fn from_item(item: &ModelCatalogItem) -> Self {
        let metadata = item.metadata();
        let prices = item.prices().map(|prices| ModelCatalogTokenPricesResponse {
            input: prices.input().to_owned(),
            output: prices.output().to_owned(),
            cache_read: prices.cache_read().to_owned(),
            cache_creation_5m: prices.cache_creation_5m().to_owned(),
            cache_creation_1h: prices.cache_creation_1h().to_owned(),
        });
        let ratios = item.ratios();
        Self {
            model: item.model().to_owned(),
            display_name: metadata.display_name().to_owned(),
            provider: metadata.provider().to_owned(),
            description: metadata.description().map(str::to_owned),
            icon_url: metadata.icon_url().map(str::to_owned),
            tags: metadata.tags().to_vec(),
            context_window: metadata.context_window(),
            input_modalities: modality_values(metadata.input_modalities()),
            output_modalities: modality_values(metadata.output_modalities()),
            supports_reasoning: metadata.supports_reasoning(),
            supports_tool_calls: metadata.supports_tool_calls(),
            supports_responses_compact: item.supports_responses_compact(),
            lifecycle: metadata.lifecycle().into(),
            runtime_status: item.runtime_status().into(),
            billing_mode: item.billing_mode(),
            prices,
            ratios: ModelCatalogRatiosResponse {
                group_micros: ratios.group_micros().to_string(),
                group_model_micros: ratios.group_model_micros().to_string(),
                peak_micros: ratios.peak_micros().to_string(),
            },
            price_version: item.price_version(),
            available_protocols: item
                .available_protocols()
                .iter()
                .copied()
                .map(Into::into)
                .collect(),
        }
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<ModelCatalogQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(ModelCatalogQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(ModelCatalogQuery::default());
    }
    if raw_query.len() > MAX_MODEL_CATALOG_QUERY_BYTES {
        return Err(ManagementError::InvalidRequest);
    }
    validate_percent_encoding(raw_query)?;
    let mut search = None;
    let mut billing_mode = None;
    let mut providers = Vec::new();
    let mut input_modalities = Vec::new();
    let mut output_modalities = Vec::new();
    let mut capabilities = Vec::new();
    let mut protocols = Vec::new();
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "q" if search.is_none() => search = Some(value.into_owned()),
            "billing_mode" if billing_mode.is_none() => {
                billing_mode = Some(parse_billing_mode(&value)?)
            }
            "provider" => providers.push(value.into_owned()),
            "input_modality" => input_modalities.push(parse_modality(&value)?),
            "output_modality" => output_modalities.push(parse_modality(&value)?),
            "capability" => capabilities.push(parse_capability(&value)?),
            "protocol" => protocols.push(parse_protocol(&value)?),
            "after" if after.is_none() => after = Some(value.into_owned()),
            "limit" if limit.is_none() => limit = Some(parse_limit(&value)?),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    providers.sort_unstable();
    input_modalities.sort_unstable();
    output_modalities.sort_unstable();
    capabilities.sort_unstable();
    protocols.sort_unstable();
    // 重复筛选值没有额外语义，规范化后交给领域查询保持稳定缓存身份。
    providers.dedup();
    input_modalities.dedup();
    output_modalities.dedup();
    capabilities.dedup();
    protocols.dedup();
    ModelCatalogQuery::new(
        search,
        billing_mode,
        providers,
        input_modalities,
        output_modalities,
        capabilities,
        protocols,
        after,
        limit.unwrap_or(af_admin::DEFAULT_MODEL_CATALOG_PAGE_SIZE),
    )
    .map_err(map_read_error)
}

fn parse_modality(value: &str) -> Result<ModelCatalogModality, ManagementError> {
    match value {
        "text" => Ok(ModelCatalogModality::Text),
        "image" => Ok(ModelCatalogModality::Image),
        "audio" => Ok(ModelCatalogModality::Audio),
        "video" => Ok(ModelCatalogModality::Video),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn parse_capability(value: &str) -> Result<ModelCatalogCapability, ManagementError> {
    match value {
        "reasoning" => Ok(ModelCatalogCapability::Reasoning),
        "tool_calls" => Ok(ModelCatalogCapability::ToolCalls),
        "responses_compact" => Ok(ModelCatalogCapability::ResponsesCompact),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn parse_protocol(value: &str) -> Result<Protocol, ManagementError> {
    value
        .parse::<Protocol>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn parse_billing_mode(value: &str) -> Result<ModelCatalogBillingMode, ManagementError> {
    match value {
        "per_token" => Ok(ModelCatalogBillingMode::PerToken),
        "free" => Ok(ModelCatalogBillingMode::Free),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn parse_limit(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty()
        || value.starts_with('+')
        || value.starts_with('-')
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<usize>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn validate_percent_encoding(raw_query: &str) -> Result<(), ManagementError> {
    let bytes = raw_query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ManagementError::InvalidRequest);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn map_read_error(error: ModelCatalogReadError) -> ManagementError {
    match error {
        ModelCatalogReadError::InvalidQuery => ManagementError::InvalidRequest,
        ModelCatalogReadError::Internal => ManagementError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_admin::{ModelCatalogProvider, ModelCatalogProviderSummary};

    #[test]
    fn query_parser_rejects_duplicates_unknowns_and_unstable_values() {
        let query = parse_list_query(Some(
            "q=gpt&billing_mode=per_token&provider=OpenAI&input_modality=text&output_modality=image&capability=reasoning&protocol=openai_responses&after=gpt-4&limit=48",
        ))
        .unwrap();
        assert_eq!(query.search(), Some("gpt"));
        assert_eq!(
            query.billing_mode(),
            Some(ModelCatalogBillingMode::PerToken)
        );
        assert_eq!(query.after(), Some("gpt-4"));
        assert_eq!(query.limit(), 48);
        assert_eq!(query.providers(), &["OpenAI"]);
        assert_eq!(query.input_modalities(), &[ModelCatalogModality::Text]);
        assert_eq!(query.output_modalities(), &[ModelCatalogModality::Image]);
        assert_eq!(query.capabilities(), &[ModelCatalogCapability::Reasoning]);
        assert_eq!(query.protocols(), &[Protocol::OpenAiResponses]);

        for raw_query in [
            "q=",
            "q=gpt&q=claude",
            "billing_mode=paid",
            "provider=",
            "provider=%20padded%20",
            "input_modality=code",
            "output_modality=code",
            "capability=vision",
            "protocol=openai",
            "after=bad%0Amodel",
            "limit=0",
            "limit=101",
            "unknown=value",
            "q=%",
            "q=%ff",
        ] {
            assert_eq!(
                parse_list_query(Some(raw_query)),
                Err(ManagementError::InvalidRequest),
                "{raw_query}"
            );
        }
        let repeated = parse_list_query(Some(
            "provider=OpenAI&provider=OpenAI&input_modality=text&input_modality=text&capability=tool_calls&capability=tool_calls",
        ))
        .unwrap();
        assert_eq!(repeated.providers(), &["OpenAI"]);
        assert_eq!(repeated.input_modalities(), &[ModelCatalogModality::Text]);
        assert_eq!(
            repeated.capabilities(),
            &[ModelCatalogCapability::ToolCalls]
        );
        let oversized = format!("q={}", "a".repeat(MAX_MODEL_CATALOG_QUERY_BYTES));
        assert_eq!(
            parse_list_query(Some(&oversized)),
            Err(ManagementError::InvalidRequest)
        );
    }

    #[test]
    fn response_serializes_the_authoritative_pricing_scope() {
        let response = ModelCatalogListResponse::from_page(ModelCatalogPage::from_parts(
            ModelCatalogPricingScope::PublicBase,
            Vec::new(),
            None,
        ));
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["pricing_scope"], "public_base");
        assert_eq!(value["models"], serde_json::json!([]));
        assert!(value["next_cursor"].is_null());
    }

    #[test]
    fn provider_response_serializes_scope_and_authoritative_counts() {
        let summary = ModelCatalogProviderSummary::new(
            ModelCatalogPricingScope::PublicBase,
            vec![ModelCatalogProvider::new("OpenAI".to_owned(), 2).unwrap()],
        )
        .unwrap();
        let value =
            serde_json::to_value(ModelCatalogProviderListResponse::from_summary(summary)).unwrap();
        assert_eq!(value["pricing_scope"], "public_base");
        assert_eq!(
            value["providers"],
            serde_json::json!([{"name": "OpenAI", "model_count": 2}])
        );
    }
}
