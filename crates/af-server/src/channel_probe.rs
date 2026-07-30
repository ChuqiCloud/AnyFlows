use std::{fmt, time::Duration};

use af_account::{CredentialDecryptor, DecryptedCredential};
use af_adapter::{
    AdaptorSendExt as _, AdaptorSettings, AdaptorTarget, Bytes, Credential, HeaderMap, HeaderName,
    HeaderValue, Method, OpenAiAdaptorSettings, Operation, PooledClient, RelayContext,
    ResponseMode, UpstreamRequest, get_adaptor,
};
use af_db::{ChannelProbeTargetRecord, ChannelProbeTargetRepository, DatabasePool};
use af_domain::{ChannelId, ChannelType, CredentialKind, Protocol, Role};
use af_http::{AdminChannelProbe, AdminChannelProbeFuture, AdminChannelProbeOutcome};
use af_protocol::{
    CanonicalRequest, ContentBlock, Message, Sampling, TokenCount, ToolChoice, openai_chat,
    openai_responses,
};
use af_scheduler::{ChannelProbe, ChannelProbeStatus};
use tokio::time::timeout;

/// 从数据库装配目标并执行最小 OpenAI 原生请求的真实渠道探活器。
#[derive(Clone)]
pub(crate) struct DatabaseChannelProbe {
    targets: ChannelProbeTargetRepository,
    decryptor: CredentialDecryptor,
    client: PooledClient,
}

impl DatabaseChannelProbe {
    /// 使用共享数据库和主转发 HTTP Client 创建探活器。
    #[must_use]
    pub(crate) fn new(
        database: DatabasePool,
        decryptor: CredentialDecryptor,
        client: PooledClient,
    ) -> Self {
        Self {
            targets: ChannelProbeTargetRepository::new(database),
            decryptor,
            client,
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
            // 专属代理仓储尚未落地，绑定代理的凭据绝不能回退直连。
            return Err(ProbeFailure::RequiredProxyUnavailable);
        }
        if target.channel_type() != ChannelType::OpenAi
            || !matches!(
                target.protocol(),
                Protocol::OpenAiChat | Protocol::OpenAiResponses
            )
        {
            return Err(ProbeFailure::UnsupportedTarget);
        }

        let decrypted = self
            .decryptor
            .decrypt_target(&target)
            .map_err(|_| ProbeFailure::CredentialDecrypt)?;
        let credential = adaptor_credential(&decrypted)?;
        let adaptor = get_adaptor(
            target.channel_type(),
            AdaptorSettings::OpenAi(
                OpenAiAdaptorSettings::for_protocol(
                    target.protocol(),
                    vec![target.model().to_owned()],
                )
                .map_err(|_| ProbeFailure::AdapterBuild)?,
            ),
        )
        .map_err(|_| ProbeFailure::AdapterBuild)?;
        if adaptor.default_protocol() != target.protocol() {
            return Err(ProbeFailure::UnsupportedTarget);
        }

        let mut context = RelayContext::new(self.client.clone());
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
        let response = adaptor
            .send(request, &context)
            .await
            .map_err(|_| ProbeFailure::Transport)?;
        if !response.status().is_success() {
            return Err(ProbeFailure::UpstreamStatus);
        }
        let body = response
            .into_body()
            .into_bytes()
            .await
            .map_err(|_| ProbeFailure::ResponseRead)?;
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
                ChannelProbeStatus::Unhealthy
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
            .field("client", &"<受控>")
            .finish()
    }
}

fn adaptor_credential(credential: &DecryptedCredential) -> Result<Credential, ProbeFailure> {
    match credential.kind() {
        CredentialKind::ApiKey => Credential::api_key(credential.expose_secret()),
        CredentialKind::Oauth => Credential::oauth(credential.expose_secret()),
        CredentialKind::SetupToken
        | CredentialKind::Bedrock
        | CredentialKind::ServiceAccount
        | CredentialKind::Upstream => return Err(ProbeFailure::UnsupportedTarget),
    }
    .map_err(|_| ProbeFailure::CredentialBuild)
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
        Protocol::OpenAiChat => Ok(Operation::Chat),
        Protocol::OpenAiResponses => Ok(Operation::Responses),
        Protocol::Anthropic | Protocol::Gemini => Err(ProbeFailure::UnsupportedTarget),
    }
}

fn probe_body(model: &str, protocol: Protocol) -> Result<Bytes, ProbeFailure> {
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
        Protocol::Anthropic | Protocol::Gemini => {
            return Err(ProbeFailure::UnsupportedTarget);
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
        Protocol::Anthropic | Protocol::Gemini => Err(ProbeFailure::UnsupportedTarget),
    }
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
            Self::Transport => "probe_transport",
            Self::UpstreamStatus => "probe_upstream_status",
            Self::ResponseRead => "probe_response_read",
            Self::InvalidResponse => "probe_invalid_response",
        }
    }
}
