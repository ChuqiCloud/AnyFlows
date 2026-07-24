use std::fmt;

use af_account::{CredentialDecryptor, DecryptedCredential};
use af_adapter::{
    AdaptorSendExt as _, AdaptorSettings, Bytes, Credential, HeaderMap, HeaderName, HeaderValue,
    Method, OpenAiAdaptorSettings, Operation, PooledClient, RelayContext, UpstreamRequest,
    get_adaptor,
};
use af_db::{ChannelProbeTargetRecord, ChannelProbeTargetRepository, DatabasePool};
use af_domain::{ChannelId, ChannelType, CredentialKind, Protocol, Role};
use af_protocol::{
    CanonicalRequest, ContentBlock, Message, Sampling, TokenCount, ToolChoice, openai_chat,
};
use af_scheduler::{ChannelProbe, ChannelProbeStatus};

/// 从数据库装配目标并执行最小 OpenAI Chat 请求的真实渠道探活器。
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
        if target.channel_type() != ChannelType::OpenAi || target.protocol() != Protocol::OpenAiChat
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
            AdaptorSettings::OpenAi(OpenAiAdaptorSettings::new(vec![target.model().to_owned()])),
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
                .build_url(&context, Operation::Chat)
                .map_err(|_| ProbeFailure::RequestBuild)?,
            headers,
            Some(probe_body(target.model())?),
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
        openai_chat::parse_response(&body).map_err(|_| ProbeFailure::InvalidResponse)?;
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

fn probe_body(model: &str) -> Result<Bytes, ProbeFailure> {
    let mut request = CanonicalRequest::new(
        Operation::Chat,
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
    let value = openai_chat::build_request(&request).map_err(|_| ProbeFailure::RequestBuild)?;
    Ok(Bytes::from(value.to_string()))
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
