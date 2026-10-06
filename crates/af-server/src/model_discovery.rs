use std::{collections::BTreeMap, fmt};

use af_account::CredentialDecryptor;
use af_adapter::{
    Adaptor, AdaptorError, AdaptorSendExt as _, AdaptorSettings, AdaptorTransportError,
    AnthropicAdaptor, AnthropicAdaptorSettings, GeminiAdaptor, GeminiAdaptorSettings, HeaderMap,
    HeaderName, HeaderValue, Method, OpenAiAdaptor, OpenAiAdaptorSettings, RelayContext,
    UpstreamRequest, get_adaptor,
};
use af_admin::{
    UpstreamModelDiscoverer, UpstreamModelDiscovery, UpstreamModelDiscoveryError,
    UpstreamModelDiscoveryFuture,
};
use af_db::{
    DiscoveredModelRecord, MAX_MODEL_SYNC_CANDIDATES, ModelDiscoveryTargetLookup,
    ModelDiscoveryTargetRecord, ModelSyncRepository,
};
use af_domain::{ChannelId, ChannelType, Protocol};
use af_httpclient::HttpClientProvider;
use serde::Deserialize;
use url::Url;

use crate::adaptor_credential::build_adaptor_credential;

const MAX_DISCOVERY_PAGES: usize = 100;
const DISCOVERY_PAGE_SIZE: &str = "1000";

/// 使用数据库渠道快照和生产受控传输枚举上游模型。
#[derive(Clone)]
pub(crate) struct DatabaseUpstreamModelDiscoverer {
    targets: ModelSyncRepository,
    decryptor: CredentialDecryptor,
    clients: HttpClientProvider,
}

impl DatabaseUpstreamModelDiscoverer {
    #[must_use]
    pub(crate) fn new(
        targets: ModelSyncRepository,
        decryptor: CredentialDecryptor,
        clients: HttpClientProvider,
    ) -> Self {
        Self {
            targets,
            decryptor,
            clients,
        }
    }

    async fn discover_inner(
        &self,
        channel_id: ChannelId,
    ) -> Result<UpstreamModelDiscovery, UpstreamModelDiscoveryError> {
        let target = match self
            .targets
            .load_discovery_target(channel_id)
            .await
            .map_err(|_| UpstreamModelDiscoveryError::Internal)?
        {
            ModelDiscoveryTargetLookup::NotFound => {
                return Err(UpstreamModelDiscoveryError::ChannelNotFound);
            }
            ModelDiscoveryTargetLookup::Unavailable => {
                return Err(UpstreamModelDiscoveryError::ChannelUnavailable);
            }
            ModelDiscoveryTargetLookup::Found(target) => target,
        };
        if target.proxy_required() {
            // 模型同步链尚未装配专属代理快照，禁止为同步请求回退直连。
            return Err(UpstreamModelDiscoveryError::ChannelUnavailable);
        }
        if !supported_target(&target) {
            return Err(UpstreamModelDiscoveryError::UnsupportedChannel);
        }
        let decrypted = self
            .decryptor
            .decrypt_envelope(
                target.channel_id(),
                target.credential_id(),
                target.credential_kind(),
                target.envelope(),
            )
            .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
        let credential = build_adaptor_credential(&decrypted)
            .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
        let adaptor = get_adaptor(
            target.channel_type(),
            adaptor_settings(target.channel_type(), target.protocol())?,
        )
        .map_err(|_| UpstreamModelDiscoveryError::UnsupportedChannel)?;
        if adaptor.default_protocol() != target.protocol() {
            return Err(UpstreamModelDiscoveryError::UnsupportedChannel);
        }
        let client = self
            .clients
            .get(target.timeout().map(af_domain::ChannelTimeout::duration))
            .map_err(|_| UpstreamModelDiscoveryError::Internal)?;
        let mut context = RelayContext::new(client);
        if let Some(base_url) = target.base_url() {
            context = context
                .with_base_url(base_url)
                .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
        }
        context = context.with_oauth_identity(target.oauth_provider(), target.oauth_account_key());
        let mut headers = HeaderMap::new();
        adaptor
            .setup_headers(&mut headers, &credential, &context)
            .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
        apply_header_overrides(&target, &mut headers)?;
        let reverse_mappings = reverse_mappings(&target)?;
        let models = match target.channel_type() {
            ChannelType::OpenAi => {
                discover_openai(&*adaptor, &context, &headers, &reverse_mappings).await?
            }
            ChannelType::Anthropic => {
                discover_anthropic(&*adaptor, &context, &headers, &reverse_mappings).await?
            }
            ChannelType::Gemini => {
                discover_gemini(&*adaptor, &context, &headers, &reverse_mappings).await?
            }
            ChannelType::Bedrock
            | ChannelType::Vertex
            | ChannelType::Jina
            | ChannelType::Cohere
            | ChannelType::Xai
            | ChannelType::Custom => {
                return Err(UpstreamModelDiscoveryError::UnsupportedChannel);
            }
        };
        Ok(UpstreamModelDiscovery::new(
            channel_id,
            target.channel_type(),
            target.protocol(),
            models,
        ))
    }
}

impl UpstreamModelDiscoverer for DatabaseUpstreamModelDiscoverer {
    fn discover<'a>(&'a self, channel_id: ChannelId) -> UpstreamModelDiscoveryFuture<'a> {
        Box::pin(async move {
            let result = self.discover_inner(channel_id).await;
            if let Err(error) = &result {
                tracing::debug!(
                    channel_id = channel_id.get(),
                    error_kind = discovery_error_kind(*error),
                    "上游模型发现失败"
                );
            }
            result
        })
    }
}

impl fmt::Debug for DatabaseUpstreamModelDiscoverer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseUpstreamModelDiscoverer(<受控>)")
    }
}

async fn discover_openai(
    adaptor: &dyn Adaptor,
    context: &RelayContext,
    headers: &HeaderMap,
    reverse: &BTreeMap<String, String>,
) -> Result<Vec<DiscoveredModelRecord>, UpstreamModelDiscoveryError> {
    let target = openai_models_url(context)?;
    let body = send_page(adaptor, context, headers, target).await?;
    if context.is_codex_oauth() {
        let wire = serde_json::from_slice::<CodexModelsWire>(&body)
            .map_err(|_| UpstreamModelDiscoveryError::InvalidResponse)?;
        return collect_models(
            wire.models
                .into_iter()
                .filter(|model| model.visibility.as_deref().unwrap_or("list") != "hide")
                .map(|model| DiscoveredEvidence {
                    upstream_model: model.slug,
                    display_name: normalize_optional_text(model.display_name),
                    description: normalize_optional_text(model.description),
                    context_window: model.context_window,
                    input_token_limit: None,
                    output_token_limit: None,
                    supported_methods: Vec::new(),
                }),
            reverse,
        );
    }
    let wire = serde_json::from_slice::<OpenAiModelsWire>(&body)
        .map_err(|_| UpstreamModelDiscoveryError::InvalidResponse)?;
    collect_models(
        wire.data.into_iter().map(|model| DiscoveredEvidence {
            upstream_model: model.id,
            display_name: None,
            description: None,
            context_window: None,
            input_token_limit: None,
            output_token_limit: None,
            supported_methods: Vec::new(),
        }),
        reverse,
    )
}

fn openai_models_url(context: &RelayContext) -> Result<Url, UpstreamModelDiscoveryError> {
    if context.is_codex_oauth() {
        let target = context
            .append_path_from(
                url::Url::parse(OpenAiAdaptor::CODEX_OAUTH_BASE_URL)
                    .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?,
                "backend-api/codex/models",
            )
            .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
        let mut target =
            Url::parse(&target).map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
        target
            .query_pairs_mut()
            .append_pair("client_version", OpenAiAdaptor::CODEX_CLIENT_VERSION);
        return Ok(target);
    }
    versioned_models_url(context, OpenAiAdaptor::DEFAULT_BASE_URL, "v1")
}

async fn discover_anthropic(
    adaptor: &dyn Adaptor,
    context: &RelayContext,
    headers: &HeaderMap,
    reverse: &BTreeMap<String, String>,
) -> Result<Vec<DiscoveredModelRecord>, UpstreamModelDiscoveryError> {
    // DeepSeek 的 Anthropic 兼容端点复用 OpenAI 风格的模型列表接口，
    // 因此模型发现需要移除 `/anthropic` 前缀后访问 `/v1/models`。
    if let Some(target) = deepseek_anthropic_models_url(context)? {
        let body = send_page(adaptor, context, headers, target).await?;
        let wire = serde_json::from_slice::<OpenAiModelsWire>(&body)
            .map_err(|_| UpstreamModelDiscoveryError::InvalidResponse)?;
        return collect_models(
            wire.data.into_iter().map(|model| DiscoveredEvidence {
                upstream_model: model.id,
                display_name: None,
                description: None,
                context_window: None,
                input_token_limit: None,
                output_token_limit: None,
                supported_methods: Vec::new(),
            }),
            reverse,
        );
    }

    let base = versioned_models_url(context, AnthropicAdaptor::DEFAULT_BASE_URL, "v1")?;
    let mut after_id = None;
    let mut evidence = Vec::new();
    for _ in 0..MAX_DISCOVERY_PAGES {
        let mut target = base.clone();
        target
            .query_pairs_mut()
            .append_pair("limit", DISCOVERY_PAGE_SIZE);
        if let Some(after_id) = after_id.as_deref() {
            target.query_pairs_mut().append_pair("after_id", after_id);
        }
        let body = send_page(adaptor, context, headers, target).await?;
        let wire = serde_json::from_slice::<AnthropicModelsWire>(&body)
            .map_err(|_| UpstreamModelDiscoveryError::InvalidResponse)?;
        evidence.extend(wire.data.into_iter().map(|model| DiscoveredEvidence {
            upstream_model: model.id,
            display_name: normalize_optional_text(model.display_name),
            description: None,
            context_window: None,
            input_token_limit: None,
            output_token_limit: None,
            supported_methods: Vec::new(),
        }));
        enforce_candidate_bound(evidence.len())?;
        if !wire.has_more {
            return collect_models(evidence, reverse);
        }
        let next = wire
            .last_id
            .filter(|value| !value.is_empty())
            .ok_or(UpstreamModelDiscoveryError::InvalidResponse)?;
        if after_id.as_deref() == Some(next.as_str()) {
            return Err(UpstreamModelDiscoveryError::InvalidResponse);
        }
        after_id = Some(next);
    }
    Err(UpstreamModelDiscoveryError::InvalidResponse)
}

fn deepseek_anthropic_models_url(
    context: &RelayContext,
) -> Result<Option<Url>, UpstreamModelDiscoveryError> {
    let mut base = context
        .resolve_base_url(AnthropicAdaptor::DEFAULT_BASE_URL)
        .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
    let is_deepseek = base.host_str() == Some("api.deepseek.com")
        && base
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            == Some("anthropic");
    if !is_deepseek {
        return Ok(None);
    }

    {
        let mut segments = base
            .path_segments_mut()
            .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
        segments.pop_if_empty();
        segments.pop();
        segments.push("v1");
        segments.push("models");
    }
    Ok(Some(base))
}

async fn discover_gemini(
    adaptor: &dyn Adaptor,
    context: &RelayContext,
    headers: &HeaderMap,
    reverse: &BTreeMap<String, String>,
) -> Result<Vec<DiscoveredModelRecord>, UpstreamModelDiscoveryError> {
    let base = versioned_models_url(
        context,
        GeminiAdaptor::DEFAULT_BASE_URL,
        GeminiAdaptor::DEFAULT_API_VERSION,
    )?;
    let mut page_token = None;
    let mut evidence = Vec::new();
    for _ in 0..MAX_DISCOVERY_PAGES {
        let mut target = base.clone();
        target
            .query_pairs_mut()
            .append_pair("pageSize", DISCOVERY_PAGE_SIZE);
        if let Some(page_token) = page_token.as_deref() {
            target
                .query_pairs_mut()
                .append_pair("pageToken", page_token);
        }
        let body = send_page(adaptor, context, headers, target).await?;
        let wire = serde_json::from_slice::<GeminiModelsWire>(&body)
            .map_err(|_| UpstreamModelDiscoveryError::InvalidResponse)?;
        for model in wire.models {
            let upstream_model = model
                .name
                .strip_prefix("models/")
                .filter(|value| !value.is_empty())
                .ok_or(UpstreamModelDiscoveryError::InvalidResponse)?
                .to_owned();
            let mut methods = model.supported_generation_methods;
            methods.sort();
            methods.dedup();
            evidence.push(DiscoveredEvidence {
                upstream_model,
                display_name: normalize_optional_text(model.display_name),
                description: normalize_optional_text(model.description),
                context_window: model.input_token_limit,
                input_token_limit: model.input_token_limit,
                output_token_limit: model.output_token_limit,
                supported_methods: methods,
            });
        }
        enforce_candidate_bound(evidence.len())?;
        let Some(next) = wire.next_page_token.filter(|value| !value.is_empty()) else {
            return collect_models(evidence, reverse);
        };
        if page_token.as_deref() == Some(next.as_str()) {
            return Err(UpstreamModelDiscoveryError::InvalidResponse);
        }
        page_token = Some(next);
    }
    Err(UpstreamModelDiscoveryError::InvalidResponse)
}

async fn send_page(
    adaptor: &dyn Adaptor,
    context: &RelayContext,
    headers: &HeaderMap,
    target: Url,
) -> Result<af_adapter::Bytes, UpstreamModelDiscoveryError> {
    let request = UpstreamRequest::new(Method::GET, String::from(target), headers.clone(), None)
        .map_err(|_| UpstreamModelDiscoveryError::Internal)?;
    let response = adaptor
        .send(request, context)
        .await
        .map_err(map_adaptor_error)?;
    if !response.status().is_success() {
        return Err(UpstreamModelDiscoveryError::UpstreamRejected);
    }
    response
        .into_body()
        .into_bytes()
        .await
        .map_err(map_adaptor_error)
}

fn collect_models<I>(
    evidence: I,
    reverse: &BTreeMap<String, String>,
) -> Result<Vec<DiscoveredModelRecord>, UpstreamModelDiscoveryError>
where
    I: IntoIterator<Item = DiscoveredEvidence>,
{
    let mut indexed = BTreeMap::new();
    for evidence in evidence {
        let canonical_model = reverse
            .get(&evidence.upstream_model)
            .cloned()
            .unwrap_or_else(|| evidence.upstream_model.clone());
        let record = DiscoveredModelRecord::new(
            canonical_model.clone(),
            evidence.upstream_model,
            evidence.display_name,
            evidence.description,
            evidence.context_window,
            evidence.input_token_limit,
            evidence.output_token_limit,
            evidence.supported_methods,
        )
        .map_err(|_| UpstreamModelDiscoveryError::InvalidResponse)?;
        if indexed.insert(canonical_model, record).is_some() {
            return Err(UpstreamModelDiscoveryError::InvalidResponse);
        }
        enforce_candidate_bound(indexed.len())?;
    }
    Ok(indexed.into_values().collect())
}

fn reverse_mappings(
    target: &ModelDiscoveryTargetRecord,
) -> Result<BTreeMap<String, String>, UpstreamModelDiscoveryError> {
    let mut reverse = BTreeMap::new();
    for mapping in target.mappings() {
        if reverse
            .insert(
                mapping.upstream_model().to_owned(),
                mapping.canonical_model().to_owned(),
            )
            .is_some()
        {
            return Err(UpstreamModelDiscoveryError::ChannelUnavailable);
        }
    }
    Ok(reverse)
}

fn versioned_models_url(
    context: &RelayContext,
    default_base_url: &str,
    version: &str,
) -> Result<Url, UpstreamModelDiscoveryError> {
    let base = context
        .resolve_base_url(default_base_url)
        .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
    let has_version_suffix = base
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .is_some_and(|segment| segment == version);
    let path = if has_version_suffix {
        "models".to_owned()
    } else {
        format!("{version}/models")
    };
    context
        .append_path(default_base_url, &path)
        .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)
        .and_then(|value| {
            Url::parse(&value).map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)
        })
}

fn apply_header_overrides(
    target: &ModelDiscoveryTargetRecord,
    headers: &mut HeaderMap,
) -> Result<(), UpstreamModelDiscoveryError> {
    for override_header in target.headers() {
        let name = HeaderName::from_bytes(override_header.name().as_bytes())
            .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
        let mut value = HeaderValue::from_str(override_header.value())
            .map_err(|_| UpstreamModelDiscoveryError::ChannelUnavailable)?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(())
}

fn adaptor_settings(
    channel_type: ChannelType,
    protocol: Protocol,
) -> Result<AdaptorSettings, UpstreamModelDiscoveryError> {
    match (channel_type, protocol) {
        (
            ChannelType::OpenAi,
            Protocol::OpenAiChat
            | Protocol::OpenAiResponses
            | Protocol::OpenAiEmbeddings
            | Protocol::OpenAiImages
            | Protocol::OpenAiAudio
            | Protocol::OpenAiSpeech,
        ) => Ok(AdaptorSettings::OpenAi(
            OpenAiAdaptorSettings::for_protocol(protocol, Vec::new())
                .map_err(|_| UpstreamModelDiscoveryError::UnsupportedChannel)?,
        )),
        (ChannelType::Anthropic, Protocol::Anthropic) => Ok(AdaptorSettings::Anthropic(
            AnthropicAdaptorSettings::new(Vec::new()),
        )),
        (ChannelType::Gemini, Protocol::Gemini) => Ok(AdaptorSettings::Gemini(
            GeminiAdaptorSettings::new(Vec::new()),
        )),
        _ => Err(UpstreamModelDiscoveryError::UnsupportedChannel),
    }
}

fn supported_target(target: &ModelDiscoveryTargetRecord) -> bool {
    matches!(
        (target.channel_type(), target.protocol()),
        (
            ChannelType::OpenAi,
            Protocol::OpenAiChat
                | Protocol::OpenAiResponses
                | Protocol::OpenAiEmbeddings
                | Protocol::OpenAiImages
                | Protocol::OpenAiAudio
                | Protocol::OpenAiSpeech
        ) | (ChannelType::Anthropic, Protocol::Anthropic)
            | (ChannelType::Gemini, Protocol::Gemini)
    )
}

fn enforce_candidate_bound(count: usize) -> Result<(), UpstreamModelDiscoveryError> {
    if count <= MAX_MODEL_SYNC_CANDIDATES {
        Ok(())
    } else {
        Err(UpstreamModelDiscoveryError::CandidateLimitExceeded)
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn map_adaptor_error(error: AdaptorError) -> UpstreamModelDiscoveryError {
    match error {
        AdaptorError::Transport(
            AdaptorTransportError::ConnectTimeout
            | AdaptorTransportError::ReadTimeout
            | AdaptorTransportError::RequestTimeout,
        ) => UpstreamModelDiscoveryError::Timeout,
        AdaptorError::ResponseBodyTooLarge => UpstreamModelDiscoveryError::InvalidResponse,
        AdaptorError::Transport(_) => UpstreamModelDiscoveryError::UpstreamRejected,
        _ => UpstreamModelDiscoveryError::Internal,
    }
}

const fn discovery_error_kind(error: UpstreamModelDiscoveryError) -> &'static str {
    match error {
        UpstreamModelDiscoveryError::ChannelNotFound => "model_discovery_channel_not_found",
        UpstreamModelDiscoveryError::ChannelUnavailable => "model_discovery_channel_unavailable",
        UpstreamModelDiscoveryError::UnsupportedChannel => "model_discovery_unsupported_channel",
        UpstreamModelDiscoveryError::Timeout => "model_discovery_timeout",
        UpstreamModelDiscoveryError::UpstreamRejected => "model_discovery_upstream_rejected",
        UpstreamModelDiscoveryError::InvalidResponse => "model_discovery_invalid_response",
        UpstreamModelDiscoveryError::CandidateLimitExceeded => {
            "model_discovery_candidate_limit_exceeded"
        }
        UpstreamModelDiscoveryError::Internal => "model_discovery_internal",
    }
}

struct DiscoveredEvidence {
    upstream_model: String,
    display_name: Option<String>,
    description: Option<String>,
    context_window: Option<i64>,
    input_token_limit: Option<i64>,
    output_token_limit: Option<i64>,
    supported_methods: Vec<String>,
}

#[derive(Deserialize)]
struct OpenAiModelsWire {
    #[serde(default)]
    data: Vec<OpenAiModelWire>,
}

#[derive(Deserialize)]
struct OpenAiModelWire {
    id: String,
}

#[derive(Deserialize)]
struct CodexModelsWire {
    #[serde(default)]
    models: Vec<CodexModelWire>,
}

#[derive(Deserialize)]
struct CodexModelWire {
    slug: String,
    display_name: Option<String>,
    description: Option<String>,
    context_window: Option<i64>,
    visibility: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicModelsWire {
    #[serde(default)]
    data: Vec<AnthropicModelWire>,
    #[serde(default)]
    has_more: bool,
    last_id: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicModelWire {
    id: String,
    display_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiModelsWire {
    #[serde(default)]
    models: Vec<GeminiModelWire>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiModelWire {
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    input_token_limit: Option<i64>,
    output_token_limit: Option<i64>,
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

#[cfg(test)]
mod tests {
    use af_httpclient::{HttpClientConfig, HttpClientPool};

    use super::*;

    fn context(base_url: &str) -> RelayContext {
        RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        )
        .with_base_url(base_url)
        .unwrap()
    }

    #[test]
    fn model_urls_preserve_prefix_and_avoid_duplicate_versions() {
        for base in [
            "https://gateway.example/openai",
            "https://gateway.example/openai/v1",
            "https://gateway.example/openai/v1/",
        ] {
            assert_eq!(
                versioned_models_url(&context(base), OpenAiAdaptor::DEFAULT_BASE_URL, "v1")
                    .unwrap()
                    .as_str(),
                "https://gateway.example/openai/v1/models"
            );
        }
        assert_eq!(
            versioned_models_url(
                &context("https://gateway.example/gemini"),
                GeminiAdaptor::DEFAULT_BASE_URL,
                "v1beta",
            )
            .unwrap()
            .as_str(),
            "https://gateway.example/gemini/v1beta/models"
        );
    }

    #[test]
    fn codex_oauth_model_url_uses_chatgpt_manifest_endpoint() {
        let context = context("https://gateway.example/incorrect/v1")
            .with_oauth_identity(Some("codex"), Some("org-codex-test"));
        assert_eq!(
            openai_models_url(&context).unwrap().as_str(),
            "https://chatgpt.com/backend-api/codex/models?client_version=0.146.0"
        );
    }

    #[test]
    fn codex_models_wire_keeps_listed_models_and_explicit_metadata() {
        let wire: CodexModelsWire = serde_json::from_value(serde_json::json!({
            "models": [
                {
                    "slug": "gpt-5.6-luna",
                    "display_name": "GPT-5.6 Luna",
                    "description": "Fast model",
                    "context_window": 200000,
                    "visibility": "list"
                },
                {"slug": "gpt-reserve", "visibility": "hide"}
            ]
        }))
        .unwrap();
        let models = collect_models(
            wire.models
                .into_iter()
                .filter(|model| model.visibility.as_deref().unwrap_or("list") != "hide")
                .map(|model| DiscoveredEvidence {
                    upstream_model: model.slug,
                    display_name: normalize_optional_text(model.display_name),
                    description: normalize_optional_text(model.description),
                    context_window: model.context_window,
                    input_token_limit: None,
                    output_token_limit: None,
                    supported_methods: Vec::new(),
                }),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(models.len(), 1);
        assert!(format!("{:?}", models[0]).contains("has_context_window_hint: true"));
    }

    #[test]
    fn deepseek_anthropic_model_discovery_uses_openai_model_endpoint() {
        assert_eq!(
            deepseek_anthropic_models_url(&context("https://api.deepseek.com/anthropic"))
                .unwrap()
                .unwrap()
                .as_str(),
            "https://api.deepseek.com/v1/models"
        );
        assert!(
            deepseek_anthropic_models_url(&context("https://api.anthropic.com"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn sparse_openai_and_rich_gemini_wires_keep_only_explicit_evidence() {
        let openai: OpenAiModelsWire = serde_json::from_value(serde_json::json!({
            "object": "list",
            "data": [{"id": "gpt-test", "created": 1, "owned_by": "owner"}]
        }))
        .unwrap();
        let models = collect_models(
            openai.data.into_iter().map(|model| DiscoveredEvidence {
                upstream_model: model.id,
                display_name: None,
                description: None,
                context_window: None,
                input_token_limit: None,
                output_token_limit: None,
                supported_methods: Vec::new(),
            }),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(models.len(), 1);
        assert!(format!("{:?}", models[0]).contains("has_context_window_hint: false"));

        let gemini: GeminiModelsWire = serde_json::from_value(serde_json::json!({
            "models": [{
                "name": "models/gemini-test",
                "displayName": "Gemini Test",
                "description": "Evidence",
                "inputTokenLimit": 1000,
                "outputTokenLimit": 200,
                "supportedGenerationMethods": ["generateContent"]
            }]
        }))
        .unwrap();
        assert_eq!(gemini.models[0].input_token_limit, Some(1000));
    }

    #[test]
    fn candidate_capacity_has_a_distinct_failure_classification() {
        assert_eq!(enforce_candidate_bound(MAX_MODEL_SYNC_CANDIDATES), Ok(()));
        assert_eq!(
            enforce_candidate_bound(MAX_MODEL_SYNC_CANDIDATES + 1),
            Err(UpstreamModelDiscoveryError::CandidateLimitExceeded)
        );
    }
}
