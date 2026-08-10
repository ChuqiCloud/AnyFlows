use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_account::CredentialDecryptor;
use af_adapter::{
    Adaptor, AdaptorError, AdaptorSendExt as _, AdaptorSettings, AdaptorTarget,
    AdaptorTransportError, AnthropicAdaptorSettings, Bytes, GeminiAdaptorSettings, HeaderMap,
    HeaderName, HeaderValue, Method, OpenAiAdaptorSettings, Operation, RelayContext, ResponseMode,
    UpstreamRequest, get_adaptor,
};
use af_db::{
    ChannelProbeTargetRecord, ChannelProbeTargetRepository, CompactProbeStateRepository,
    DatabasePool,
};
use af_domain::{
    ChannelId, ChannelType, Protocol, ResponsesCompactMode, ResponsesCompactProbeResult, Role,
};
use af_http::{AdminChannelProbe, AdminChannelProbeFuture, AdminChannelProbeOutcome};
use af_httpclient::HttpClientProvider;
use af_protocol::{
    AudioFile, AudioFileFormat, AudioTranscriptionOptions, CanonicalAudioTranscriptionRequest,
    CanonicalEmbeddingRequest, CanonicalImageGenerationRequest, CanonicalRequest,
    CanonicalResponsesCompactionRequest, ContentBlock, EmbeddingInput, ImageDimensions,
    ImageGenerationOptions, ImageOutputFormat, ImageQuality, ImageSize, Message,
    ResponsesCompactionInput, Sampling, TokenCount, ToolChoice, anthropic, gemini, openai_audio,
    openai_chat, openai_embeddings, openai_images, openai_responses, openai_responses_compact,
};
use af_relay::encode_openai_audio_request;
use af_scheduler::{ChannelProbe, ChannelProbeStatus};
use tokio::time::timeout;

use crate::adaptor_credential::build_adaptor_credential;

/// 从数据库装配目标并执行最小原生协议请求的真实渠道探活器。
#[derive(Clone)]
pub(crate) struct DatabaseChannelProbe {
    targets: ChannelProbeTargetRepository,
    compact_states: CompactProbeStateRepository,
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
            targets: ChannelProbeTargetRepository::new(database.clone()),
            compact_states: CompactProbeStateRepository::new(database),
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
            headers.clone(),
            Some(probe_body(target.model(), target.protocol())?),
        )
        .map_err(|_| ProbeFailure::RequestBuild)?;
        let response_body_limit = match target.protocol() {
            Protocol::OpenAiImages => Some(openai_images::MAX_RESPONSE_BODY_BYTES),
            Protocol::OpenAiAudio => Some(openai_audio::MAX_TRANSCRIPTION_RESPONSE_BODY_BYTES),
            _ => None,
        };
        let request = match response_body_limit {
            Some(limit) => request
                .with_response_body_limit(limit)
                .map_err(|_| ProbeFailure::RequestBuild)?,
            None => request,
        };
        let response = adaptor
            .send(request, &context)
            .await
            .map_err(map_transport_failure)?;
        if !response.status().is_success() {
            return Err(ProbeFailure::UpstreamStatus);
        }
        let response_body = response.into_body();
        let body = match response_body_limit {
            Some(limit) => response_body.into_bytes_with_limit(limit).await,
            None => response_body.into_bytes().await,
        }
        .map_err(map_response_read_failure)?;
        validate_probe_response(target.protocol(), &body)?;
        self.probe_responses_compact(&target, adaptor.as_ref(), &context, headers)
            .await;
        Ok(())
    }

    /// 在普通 Responses 健康检查成功后独立探测 Compact，不让能力失败误伤普通接口。
    async fn probe_responses_compact(
        &self,
        target: &ChannelProbeTargetRecord,
        adaptor: &dyn Adaptor,
        context: &RelayContext,
        headers: HeaderMap,
    ) {
        if target.protocol() != Protocol::OpenAiResponses
            || target.channel_type() != ChannelType::OpenAi
            || target.responses_compact_mode() == ResponsesCompactMode::ForceOff
        {
            return;
        }
        let Some(model) = target.responses_compact_model() else {
            return;
        };
        let started_at = match probe_timestamp_millis() {
            Ok(started_at) => started_at,
            Err(failure) => {
                log_compact_probe_failure(target.channel_id(), failure);
                return;
            }
        };
        let request = match compact_probe_request(adaptor, context, headers, model) {
            Ok(request) => request,
            Err(failure) => {
                log_compact_probe_failure(target.channel_id(), failure);
                return;
            }
        };
        let response = match adaptor.send(request, context).await {
            Ok(response) => response,
            Err(error) => {
                log_compact_probe_failure(target.channel_id(), map_transport_failure(error));
                return;
            }
        };
        let status = response.status();
        let status_code = status.as_u16();
        if status.as_u16() == 404 {
            self.record_compact_fact(
                target,
                ResponsesCompactProbeResult::Unsupported,
                started_at,
                Some(status_code),
            )
            .await;
            return;
        }
        if !status.is_success() {
            // 认证、模型、限流和 5xx 都可能是暂态，不能覆盖既有支持事实。
            log_compact_probe_failure(target.channel_id(), ProbeFailure::UpstreamStatus);
            return;
        }
        let body = match response
            .into_body()
            .into_bytes_with_limit(openai_responses_compact::MAX_BODY_BYTES)
            .await
        {
            Ok(body) => body,
            Err(error) => {
                log_compact_probe_failure(target.channel_id(), map_response_read_failure(error));
                return;
            }
        };
        let result = if openai_responses_compact::parse_response(&body).is_ok() {
            ResponsesCompactProbeResult::Supported
        } else {
            // 2xx 却不是 response.compaction 是确定性协议不支持，不能由普通响应冒充。
            ResponsesCompactProbeResult::Unsupported
        };
        self.record_compact_fact(target, result, started_at, Some(status_code))
            .await;
    }

    async fn record_compact_fact(
        &self,
        target: &ChannelProbeTargetRecord,
        result: ResponsesCompactProbeResult,
        checked_at: i64,
        http_status: Option<u16>,
    ) {
        if self
            .compact_states
            .record(
                target.channel_id(),
                &target.revision(),
                result,
                checked_at,
                http_status,
            )
            .await
            .is_err()
        {
            // 能力事实写入失败不能覆盖已经确认的普通渠道健康结论。
            log_compact_probe_failure(target.channel_id(), ProbeFailure::StateWrite);
        }
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
            .field("compact_states", &"<受控>")
            .field("decryptor", &"<受控>")
            .field("clients", &"<受控>")
            .field("default_timeout", &self.default_timeout)
            .finish()
    }
}

fn compact_probe_request(
    adaptor: &dyn Adaptor,
    context: &RelayContext,
    headers: HeaderMap,
    model: &str,
) -> Result<UpstreamRequest, ProbeFailure> {
    let canonical = CanonicalResponsesCompactionRequest::new(
        model.to_owned(),
        Some(ResponsesCompactionInput::Text("ping".to_owned())),
        None,
        None,
    )
    .map_err(|_| ProbeFailure::RequestBuild)?;
    let value = openai_responses_compact::build_request(&canonical)
        .map_err(|_| ProbeFailure::RequestBuild)?;
    let body = serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| ProbeFailure::RequestBuild)?;
    UpstreamRequest::new(
        Method::POST,
        adaptor
            .build_url(
                context,
                AdaptorTarget::new(model, Operation::ResponsesCompact, ResponseMode::Full),
            )
            .map_err(|_| ProbeFailure::RequestBuild)?,
        headers,
        Some(body),
    )
    .and_then(|request| request.with_response_body_limit(openai_responses_compact::MAX_BODY_BYTES))
    .map_err(|_| ProbeFailure::RequestBuild)
}

fn probe_timestamp_millis() -> Result<i64, ProbeFailure> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .filter(|timestamp| *timestamp > 0)
        .ok_or(ProbeFailure::Clock)
}

fn log_compact_probe_failure(channel_id: ChannelId, failure: ProbeFailure) {
    tracing::debug!(
        channel_id = channel_id.get(),
        probe_result = "inconclusive",
        error_kind = failure.as_str(),
        "Responses Compact 专属探活未产生新的确定性事实"
    );
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
        Protocol::OpenAiAudio => Ok(Operation::Audio),
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
    if protocol == Protocol::OpenAiAudio {
        return encode_openai_audio_request(&probe_audio_request(model)?)
            .map_err(|_| ProbeFailure::RequestBuild);
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
        Protocol::OpenAiEmbeddings | Protocol::OpenAiImages | Protocol::OpenAiAudio => {
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
        Protocol::OpenAiAudio => openai_audio::parse_transcription_response(body)
            .map(|_| ())
            .map_err(|_| ProbeFailure::InvalidResponse),
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
                | Protocol::OpenAiAudio
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
    let mut models = vec![target.model().to_owned()];
    if let Some(compact_model) = target.responses_compact_model()
        && compact_model != target.model()
    {
        models.push(compact_model.to_owned());
    }
    match (target.channel_type(), target.protocol()) {
        (
            ChannelType::OpenAi,
            Protocol::OpenAiChat
            | Protocol::OpenAiResponses
            | Protocol::OpenAiEmbeddings
            | Protocol::OpenAiImages
            | Protocol::OpenAiAudio,
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

fn probe_audio_request(model: &str) -> Result<CanonicalAudioTranscriptionRequest, ProbeFailure> {
    let file = AudioFile::new(AudioFileFormat::Wav, one_second_probe_wav())
        .map_err(|_| ProbeFailure::RequestBuild)?;
    let options = AudioTranscriptionOptions::new(None, None, Vec::new(), None)
        .map_err(|_| ProbeFailure::RequestBuild)?;
    CanonicalAudioTranscriptionRequest::new(model.to_owned(), file, options)
        .map_err(|_| ProbeFailure::RequestBuild)
}

fn one_second_probe_wav() -> Bytes {
    const SAMPLE_RATE: u32 = 8_000;
    const DATA_BYTES: u32 = SAMPLE_RATE * 2;
    let mut bytes = Vec::with_capacity((44 + DATA_BYTES) as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + DATA_BYTES).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&DATA_BYTES.to_le_bytes());
    bytes.resize((44 + DATA_BYTES) as usize, 0);
    Bytes::from(bytes)
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
    Clock,
    StateWrite,
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
            Self::Clock => "probe_clock",
            Self::StateWrite => "probe_compact_state_write",
        }
    }
}
