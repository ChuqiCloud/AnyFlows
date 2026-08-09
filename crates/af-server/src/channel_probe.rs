use std::{fmt, time::Duration};

use af_account::CredentialDecryptor;
use af_adapter::{
    AdaptorError, AdaptorSendExt as _, AdaptorSettings, AdaptorTarget, AdaptorTransportError,
    AnthropicAdaptorSettings, Bytes, GeminiAdaptorSettings, HeaderMap, HeaderName, HeaderValue,
    Method, OpenAiAdaptorSettings, Operation, RelayContext, ResponseMode, UpstreamRequest,
    get_adaptor,
};
use af_db::{ChannelProbeTargetRecord, ChannelProbeTargetRepository, DatabasePool};
use af_domain::{ChannelId, ChannelType, Protocol, Role};
use af_http::{AdminChannelProbe, AdminChannelProbeFuture, AdminChannelProbeOutcome};
use af_httpclient::HttpClientProvider;
use af_protocol::{
    CanonicalEmbeddingRequest, CanonicalImageGenerationRequest, CanonicalRequest, ContentBlock,
    EmbeddingInput, ImageDimensions, ImageGenerationOptions, ImageOutputFormat, ImageQuality,
    ImageSize, Message, Sampling, TokenCount, ToolChoice, anthropic, gemini, openai_chat,
    openai_embeddings, openai_images, openai_responses,
};
use af_scheduler::{ChannelProbe, ChannelProbeStatus};
use tokio::time::timeout;

use crate::adaptor_credential::build_adaptor_credential;

/// 从数据库装配目标并执行最小原生协议请求的真实渠道探活器。
#[derive(Clone)]
pub(crate) struct DatabaseChannelProbe {
    targets: ChannelProbeTargetRepository,
    decryptor: CredentialDecryptor,
    clients: HttpClientProvider,
    default_timeout: Duration,
}

impl DatabaseChannelProbe {
    /// 使用共享数据库和主转发 HTTP Client 创建探活器。
    #[must_use]
    pub(crate) fn new(
        database: DatabasePool,
        decryptor: CredentialDecryptor,
        clients: HttpClientProvider,
        default_timeout: Duration,
    ) -> Self {
        debug_assert!(!default_timeout.is_zero());
        Self {
            targets: ChannelProbeTargetRepository::new(database),
            decryptor,
            clients,
            default_timeout,
        }
    }

    async fn check_inner(&self, channel_id: ChannelId) -> Result<(), ProbeFailure> {
        let target = self
            .targets
            .load(channel_id)
            .await
            .map_err(|_| ProbeFailure::TargetRead)?
            .ok_or(ProbeFailure::TargetUnavailable)?;
        if target.proxy_required() {
            // 探活链尚未装配专属代理快照，绑定代理的凭据必须保持失败关闭。
            return Err(ProbeFailure::RequiredProxyUnavailable);
        }
        if !supported_target(&target) {
            return Err(ProbeFailure::UnsupportedTarget);
        }

        let decrypted = self
            .decryptor
            .decrypt_target(&target)
            .map_err(|_| ProbeFailure::CredentialDecrypt)?;
        let credential =
            build_adaptor_credential(&decrypted).map_err(|_| ProbeFailure::CredentialBuild)?;
        let adaptor = get_adaptor(target.channel_type(), adaptor_settings(&target)?)
            .map_err(|_| ProbeFailure::AdapterBuild)?;
        if adaptor.default_protocol() != target.protocol() {
            return Err(ProbeFailure::UnsupportedTarget);
        }

        let timeout = target
            .timeout()
            .map_or(self.default_timeout, af_domain::ChannelTimeout::duration);
        let client = self
            .clients
            .get(Some(timeout))
            .map_err(|_| ProbeFailure::ContextBuild)?;
        let mut context = RelayContext::new(client);
        if let Some(base_url) = target.base_url() {
            context = context
                .with_base_url(base_url)
                .map_err(|_| ProbeFailure::ContextBuild)?;
        }
        let mut headers = HeaderMap::new();
        adaptor
            .setup_headers(&mut headers, &credential, &context)
            .map_err(|_| ProbeFailure::CredentialBuild)?;
        apply_header_overrides(&target, &mut headers)?;

        let request = UpstreamRequest::new(
            Method::POST,
            adaptor
                .build_url(
                    &context,
                    AdaptorTarget::new(
                        target.model(),
                        probe_operation(target.protocol())?,
                        ResponseMode::Full,
                    ),
                )
                .map_err(|_| ProbeFailure::RequestBuild)?,
            headers,
            Some(probe_body(target.model(), target.protocol())?),
        )
        .map_err(|_| ProbeFailure::RequestBuild)?;
        let request = if target.protocol() == Protocol::OpenAiImages {
            request
                .with_response_body_limit(openai_images::MAX_RESPONSE_BODY_BYTES)
                .map_err(|_| ProbeFailure::RequestBuild)?
        } else {
            request
        };
        let response = adaptor
            .send(request, &context)
            .await
            .map_err(map_transport_failure)?;
        if !response.status().is_success() {
            return Err(ProbeFailure::UpstreamStatus);
        }
        let response_body = response.into_body();
        let body = if target.protocol() == Protocol::OpenAiImages {
            response_body
                .into_bytes_with_limit(openai_images::MAX_RESPONSE_BODY_BYTES)
                .await
        } else {
            response_body.into_bytes().await
        }
        .map_err(map_response_read_failure)?;
        validate_probe_response(target.protocol(), &body)?;
        Ok(())
    }
}

impl ChannelProbe for DatabaseChannelProbe {
    async fn check(&self, channel_id: ChannelId) -> ChannelProbeStatus {
        match self.check_inner(channel_id).await {
            Ok(()) => {
                tracing::debug!(
                    channel_id = channel_id.get(),
                    probe_result = "healthy",
                    "渠道真实探活成功"
                );
                ChannelProbeStatus::Healthy
            }
            Err(failure) => {
                tracing::debug!(
                    channel_id = channel_id.get(),
                    probe_result = "unhealthy",
                    error_kind = failure.as_str(),
                    "渠道真实探活失败"
                );
                if failure == ProbeFailure::TimedOut {
                    ChannelProbeStatus::TimedOut
                } else {
                    ChannelProbeStatus::Unhealthy
                }
            }
        }
    }
}

/// 为管理端单次测活施加配置一致的硬超时。
#[derive(Clone)]
pub(crate) struct BoundedAdminChannelProbe {
    probe: DatabaseChannelProbe,
    timeout: Duration,
}

impl BoundedAdminChannelProbe {
    #[must_use]
    pub(crate) const fn new(probe: DatabaseChannelProbe, timeout: Duration) -> Self {
        Self { probe, timeout }
    }
}

impl AdminChannelProbe for BoundedAdminChannelProbe {
    fn probe<'a>(&'a self, channel_id: ChannelId) -> AdminChannelProbeFuture<'a> {
        Box::pin(async move {
            match timeout(self.timeout, self.probe.check(channel_id)).await {
                Ok(ChannelProbeStatus::Healthy) => AdminChannelProbeOutcome::Healthy,
                Ok(ChannelProbeStatus::Unhealthy) => AdminChannelProbeOutcome::Unhealthy,
                Ok(ChannelProbeStatus::TimedOut) => AdminChannelProbeOutcome::TimedOut,
                // 调度层未来新增的状态默认按不健康处理，避免未知结论被误报为成功。
                Ok(_) => AdminChannelProbeOutcome::Unhealthy,
                Err(_) => AdminChannelProbeOutcome::TimedOut,
            }
        })
    }
}

impl fmt::Debug for BoundedAdminChannelProbe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundedAdminChannelProbe")
            .field("probe", &"<受控>")
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl fmt::Debug for DatabaseChannelProbe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseChannelProbe")
            .field("targets", &"<受控>")
            .field("decryptor", &"<受控>")
            .field("clients", &"<受控>")
            .field("default_timeout", &self.default_timeout)
            .finish()
    }
}

fn apply_header_overrides(
    target: &ChannelProbeTargetRecord,
    headers: &mut HeaderMap,
) -> Result<(), ProbeFailure> {
    for override_header in target.headers() {
        let name = HeaderName::from_bytes(override_header.name().as_bytes())
            .map_err(|_| ProbeFailure::HeaderOverride)?;
        let mut value = HeaderValue::from_str(override_header.value())
            .map_err(|_| ProbeFailure::HeaderOverride)?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(())
}

fn probe_operation(protocol: Protocol) -> Result<Operation, ProbeFailure> {
    match protocol {
        Protocol::OpenAiChat | Protocol::Anthropic | Protocol::Gemini => Ok(Operation::Chat),
        Protocol::OpenAiResponses => Ok(Operation::Responses),
        Protocol::OpenAiEmbeddings => Ok(Operation::Embedding),
        Protocol::OpenAiImages => Ok(Operation::Image),
    }
}

fn probe_body(model: &str, protocol: Protocol) -> Result<Bytes, ProbeFailure> {
    if protocol == Protocol::OpenAiEmbeddings {
        let value = openai_embeddings::build_request(&probe_embedding_request(model)?)
            .map_err(|_| ProbeFailure::RequestBuild)?;
        return Ok(Bytes::from(value.to_string()));
    }
    if protocol == Protocol::OpenAiImages {
        // 图片探活使用官方最低质量和单张 1024 方图，降低真实探测产生的上游费用。
        let value = openai_images::build_request(&probe_image_request(model)?)
            .map_err(|_| ProbeFailure::RequestBuild)?;
        return Ok(Bytes::from(value.to_string()));
    }
    let mut request = CanonicalRequest::new(
        probe_operation(protocol)?,
        model.to_owned(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text("ping".to_owned())],
        )],
        false,
    );
    request.tool_choice = ToolChoice::None;
    request.sampling = Sampling::new(
        None,
        None,
        Some(TokenCount::new(1).expect("固定探活输出上限必须有效")),
        Vec::new(),
    )
    .expect("固定探活采样参数必须有效");
    let value = match protocol {
        Protocol::OpenAiChat => {
            openai_chat::build_request(&request).map_err(|_| ProbeFailure::RequestBuild)?
        }
        Protocol::OpenAiResponses => {
            let mut value = openai_responses::build_request(&request)
                .map_err(|_| ProbeFailure::RequestBuild)?;
            // 探活不产生可续接的供应商状态，避免周期任务累积无主响应。
            value["store"] = false.into();
            value
        }
        Protocol::OpenAiEmbeddings | Protocol::OpenAiImages => {
            unreachable!("非对话协议已在独立请求分支返回")
        }
        Protocol::Anthropic => {
            anthropic::build_request(&request).map_err(|_| ProbeFailure::RequestBuild)?
        }
        Protocol::Gemini => {
            gemini::build_request(&request).map_err(|_| ProbeFailure::RequestBuild)?
        }
    };
    Ok(Bytes::from(value.to_string()))
}

fn validate_probe_response(protocol: Protocol, body: &[u8]) -> Result<(), ProbeFailure> {
    match protocol {
        Protocol::OpenAiChat => openai_chat::parse_response(body)
            .map(|_| ())
            .map_err(|_| ProbeFailure::InvalidResponse),
        Protocol::OpenAiResponses => openai_responses::parse_response(body)
            .map(|_| ())
            .map_err(|_| ProbeFailure::InvalidResponse),
        Protocol::OpenAiEmbeddings => {
            let request = probe_embedding_request("probe-model")?;
            openai_embeddings::parse_response(body)
                .and_then(|response| {
                    response
                        .validate_for_request(&request)
                        .map_err(|_| openai_embeddings::ParseEmbeddingResponseError::InvalidValue)
                        .map(|_| response)
                })
                .map(|_| ())
                .map_err(|_| ProbeFailure::InvalidResponse)
        }
        Protocol::OpenAiImages => {
            let request = probe_image_request("probe-model")?;
            openai_images::parse_response(body)
                .and_then(|response| {
                    response
                        .validate_for_request(&request)
                        .map_err(|_| openai_images::ParseImageGenerationResponseError::InvalidValue)
                        .map(|_| response)
                })
                .map(|_| ())
                .map_err(|_| ProbeFailure::InvalidResponse)
        }
        Protocol::Anthropic => anthropic::parse_response(body)
            .map(|_| ())
            .map_err(|_| ProbeFailure::InvalidResponse),
        Protocol::Gemini => gemini::parse_response(body)
            .map(|_| ())
            .map_err(|_| ProbeFailure::InvalidResponse),
    }
}

fn supported_target(target: &ChannelProbeTargetRecord) -> bool {
    matches!(
        (target.channel_type(), target.protocol()),
        (
            ChannelType::OpenAi,
            Protocol::OpenAiChat
                | Protocol::OpenAiResponses
                | Protocol::OpenAiEmbeddings
                | Protocol::OpenAiImages
        ) | (ChannelType::Anthropic, Protocol::Anthropic)
            | (ChannelType::Gemini, Protocol::Gemini)
    )
}

fn map_transport_failure(error: AdaptorError) -> ProbeFailure {
    match error {
        AdaptorError::Transport(
            AdaptorTransportError::ConnectTimeout
            | AdaptorTransportError::ReadTimeout
            | AdaptorTransportError::RequestTimeout,
        ) => ProbeFailure::TimedOut,
        _ => ProbeFailure::Transport,
    }
}

fn map_response_read_failure(error: AdaptorError) -> ProbeFailure {
    match map_transport_failure(error) {
        ProbeFailure::TimedOut => ProbeFailure::TimedOut,
        _ => ProbeFailure::ResponseRead,
    }
}

fn adaptor_settings(target: &ChannelProbeTargetRecord) -> Result<AdaptorSettings, ProbeFailure> {
    let models = vec![target.model().to_owned()];
    match (target.channel_type(), target.protocol()) {
        (
            ChannelType::OpenAi,
            Protocol::OpenAiChat
            | Protocol::OpenAiResponses
            | Protocol::OpenAiEmbeddings
            | Protocol::OpenAiImages,
        ) => Ok(AdaptorSettings::OpenAi(
            OpenAiAdaptorSettings::for_protocol(target.protocol(), models)
                .map_err(|_| ProbeFailure::AdapterBuild)?,
        )),
        (ChannelType::Anthropic, Protocol::Anthropic) => Ok(AdaptorSettings::Anthropic(
            AnthropicAdaptorSettings::new(models),
        )),
        (ChannelType::Gemini, Protocol::Gemini) => {
            Ok(AdaptorSettings::Gemini(GeminiAdaptorSettings::new(models)))
        }
        _ => Err(ProbeFailure::UnsupportedTarget),
    }
}

fn probe_embedding_request(model: &str) -> Result<CanonicalEmbeddingRequest, ProbeFailure> {
    CanonicalEmbeddingRequest::new(
        model.to_owned(),
        EmbeddingInput::Text("ping".to_owned()),
        None,
    )
    .map_err(|_| ProbeFailure::RequestBuild)
}

fn probe_image_request(model: &str) -> Result<CanonicalImageGenerationRequest, ProbeFailure> {
    let dimensions = ImageDimensions::new(1_024, 1_024)
        .map(ImageSize::Exact)
        .map_err(|_| ProbeFailure::RequestBuild)?;
    let options = ImageGenerationOptions::new(
        None,
        Some(dimensions),
        Some(ImageQuality::Low),
        None,
        None,
        Some(ImageOutputFormat::Png),
        None,
    )
    .map_err(|_| ProbeFailure::RequestBuild)?;
    CanonicalImageGenerationRequest::new(model.to_owned(), "ping".to_owned(), options)
        .map_err(|_| ProbeFailure::RequestBuild)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProbeFailure {
    TargetRead,
    TargetUnavailable,
    RequiredProxyUnavailable,
    UnsupportedTarget,
    CredentialDecrypt,
    CredentialBuild,
    AdapterBuild,
    ContextBuild,
    HeaderOverride,
    RequestBuild,
    TimedOut,
    Transport,
    UpstreamStatus,
    ResponseRead,
    InvalidResponse,
}

impl ProbeFailure {
    const fn as_str(self) -> &'static str {
        match self {
            Self::TargetRead => "probe_target_read",
            Self::TargetUnavailable => "probe_target_unavailable",
            Self::RequiredProxyUnavailable => "probe_required_proxy_unavailable",
            Self::UnsupportedTarget => "probe_unsupported_target",
            Self::CredentialDecrypt => "probe_credential_decrypt",
            Self::CredentialBuild => "probe_credential_build",
            Self::AdapterBuild => "probe_adapter_build",
            Self::ContextBuild => "probe_context_build",
            Self::HeaderOverride => "probe_header_override",
            Self::RequestBuild => "probe_request_build",
            Self::TimedOut => "probe_timeout",
            Self::Transport => "probe_transport",
            Self::UpstreamStatus => "probe_upstream_status",
            Self::ResponseRead => "probe_response_read",
            Self::InvalidResponse => "probe_invalid_response",
        }
    }
}
