use std::fmt;

use af_domain::{ChannelType, ClientSimulationProfile, CredentialKind, Operation, Protocol};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};

use crate::{AdaptorError, AdaptorResult, UpstreamRequest};

/// 单个客户端仿真 Header 值的独立上限。
pub const MAX_CLIENT_SIMULATION_HEADER_VALUE_BYTES: usize = 512;

const ANTHROPIC_CLI_USER_AGENT_V1: &str = "claude-cli/2.1.114 (external, sdk-cli)";
const ANTHROPIC_CLI_X_APP_V1: &str = "cli";

/// 客户端仿真中间件可读取的非敏感请求投影。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientSimulationContext {
    channel_type: ChannelType,
    protocol: Protocol,
    operation: Operation,
    credential_kind: CredentialKind,
}

impl ClientSimulationContext {
    /// 创建不含 URL、模型、正文或凭据内容的仿真上下文。
    #[must_use]
    pub const fn new(
        channel_type: ChannelType,
        protocol: Protocol,
        operation: Operation,
        credential_kind: CredentialKind,
    ) -> Self {
        Self {
            channel_type,
            protocol,
            operation,
            credential_kind,
        }
    }

    /// 返回目标适配器类型。
    #[must_use]
    pub const fn channel_type(self) -> ChannelType {
        self.channel_type
    }

    /// 返回目标原生协议。
    #[must_use]
    pub const fn protocol(self) -> Protocol {
        self.protocol
    }

    /// 返回本次规范化操作。
    #[must_use]
    pub const fn operation(self) -> Operation {
        self.operation
    }

    /// 返回凭据种类，不暴露凭据内容。
    #[must_use]
    pub const fn credential_kind(self) -> CredentialKind {
        self.credential_kind
    }
}

/// 客户端仿真插件唯一可以生成的 HTTP 身份 Header。
///
/// 字段保持私有，插件无法借此覆盖认证、Cookie、协议版本、请求 ID 或连接级 Header。
#[derive(Clone, Default, Eq, PartialEq)]
pub struct ClientSimulationHeaders {
    user_agent: Option<HeaderValue>,
    x_app: Option<HeaderValue>,
}

impl ClientSimulationHeaders {
    /// 创建空的白名单 Header 补丁。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            user_agent: None,
            x_app: None,
        }
    }

    /// 设置受控 `User-Agent`；值必须是有界可见 ASCII 文本。
    pub fn with_user_agent(mut self, value: &str) -> AdaptorResult<Self> {
        self.user_agent = Some(parse_identity_header(value)?);
        Ok(self)
    }

    /// 设置受控 `X-App`；值必须是有界可见 ASCII 文本。
    pub fn with_x_app(mut self, value: &str) -> AdaptorResult<Self> {
        self.x_app = Some(parse_identity_header(value)?);
        Ok(self)
    }

    /// 判断中间件是否生成了至少一个身份字段。
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.user_agent.is_none() && self.x_app.is_none()
    }

    fn into_header_map(self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(value) = self.user_agent {
            headers.insert(HeaderName::from_static("user-agent"), value);
        }
        if let Some(value) = self.x_app {
            headers.insert(HeaderName::from_static("x-app"), value);
        }
        headers
    }
}

impl fmt::Debug for ClientSimulationHeaders {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientSimulationHeaders")
            .field("has_user_agent", &self.user_agent.is_some())
            .field("has_x_app", &self.x_app.is_some())
            .finish()
    }
}

/// 只产生白名单身份 Header 的可插拔客户端仿真契约。
///
/// 中间件不接触完整请求，因而不能改变 URL、正文、认证、模型或计费事实。
pub trait ClientSimulationMiddleware: Send + Sync {
    /// 返回审计与配置使用的闭合版本化档案。
    fn profile(&self) -> ClientSimulationProfile;

    /// 根据非敏感上下文生成白名单身份 Header。
    fn prepare_headers(
        &self,
        context: ClientSimulationContext,
    ) -> AdaptorResult<ClientSimulationHeaders>;
}

/// AnyFlows 内置的版本化客户端仿真档案实现。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuiltInClientSimulation {
    profile: ClientSimulationProfile,
}

impl BuiltInClientSimulation {
    /// 选择一个闭合内置档案；调用方仍须显式把它装配到候选。
    #[must_use]
    pub const fn new(profile: ClientSimulationProfile) -> Self {
        Self { profile }
    }
}

impl ClientSimulationMiddleware for BuiltInClientSimulation {
    fn profile(&self) -> ClientSimulationProfile {
        self.profile
    }

    fn prepare_headers(
        &self,
        context: ClientSimulationContext,
    ) -> AdaptorResult<ClientSimulationHeaders> {
        match self.profile {
            ClientSimulationProfile::AnthropicCliHeadersV1 => {
                if context.channel_type() != ChannelType::Anthropic
                    || context.protocol() != Protocol::Anthropic
                    || context.operation() != Operation::Chat
                    || context.credential_kind() != CredentialKind::Oauth
                {
                    return Err(AdaptorError::InvalidClientSimulation);
                }
                ClientSimulationHeaders::new()
                    .with_user_agent(ANTHROPIC_CLI_USER_AGENT_V1)?
                    .with_x_app(ANTHROPIC_CLI_X_APP_V1)
            }
        }
    }
}

/// 应用一次受控仿真补丁并重新执行统一请求 Header 预算校验。
///
/// 插件返回空补丁视为配置错误，禁止静默退回未仿真请求。
pub fn apply_client_simulation(
    middleware: &dyn ClientSimulationMiddleware,
    context: ClientSimulationContext,
    request: UpstreamRequest,
) -> AdaptorResult<UpstreamRequest> {
    let headers = middleware.prepare_headers(context)?;
    if headers.is_empty() {
        return Err(AdaptorError::InvalidClientSimulation);
    }
    request.with_header_overrides(headers.into_header_map())
}

fn parse_identity_header(value: &str) -> AdaptorResult<HeaderValue> {
    if value.is_empty()
        || value.len() > MAX_CLIENT_SIMULATION_HEADER_VALUE_BYTES
        || value.trim() != value
        || !value.bytes().all(|byte| matches!(byte, b' '..=b'~'))
    {
        return Err(AdaptorError::InvalidClientSimulation);
    }
    HeaderValue::try_from(value).map_err(|_| AdaptorError::InvalidClientSimulation)
}

#[cfg(test)]
mod tests {
    use af_httpclient::{Bytes, Method};

    use super::*;

    fn anthropic_oauth_context() -> ClientSimulationContext {
        ClientSimulationContext::new(
            ChannelType::Anthropic,
            Protocol::Anthropic,
            Operation::Chat,
            CredentialKind::Oauth,
        )
    }

    #[test]
    fn built_in_anthropic_profile_only_adds_closed_identity_headers() {
        let mut original = HeaderMap::new();
        let mut authorization = HeaderValue::from_static("Bearer private-token-canary");
        authorization.set_sensitive(true);
        original.insert(HeaderName::from_static("authorization"), authorization);
        original.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
        let request = UpstreamRequest::new(
            Method::POST,
            "https://api.anthropic.com/v1/messages",
            original,
            Some(Bytes::from_static(b"private-body-canary")),
        )
        .unwrap();
        let middleware =
            BuiltInClientSimulation::new(ClientSimulationProfile::AnthropicCliHeadersV1);

        let request =
            apply_client_simulation(&middleware, anthropic_oauth_context(), request).unwrap();

        assert_eq!(request.headers()["user-agent"], ANTHROPIC_CLI_USER_AGENT_V1);
        assert_eq!(request.headers()["x-app"], ANTHROPIC_CLI_X_APP_V1);
        assert_eq!(
            request.headers()["authorization"],
            "Bearer private-token-canary"
        );
        assert_eq!(request.headers()["anthropic-version"], "2023-06-01");
        assert_eq!(request.headers().len(), 4);

        let debug = format!("{middleware:?}\n{request:?}");
        assert!(debug.contains("AnthropicCliHeadersV1"));
        assert!(!debug.contains("private-token-canary"));
        assert!(!debug.contains("private-body-canary"));
        assert!(!debug.contains(ANTHROPIC_CLI_USER_AGENT_V1));
    }

    #[test]
    fn built_in_profile_fails_closed_outside_anthropic_oauth_chat() {
        let middleware =
            BuiltInClientSimulation::new(ClientSimulationProfile::AnthropicCliHeadersV1);
        for context in [
            ClientSimulationContext::new(
                ChannelType::OpenAi,
                Protocol::Anthropic,
                Operation::Chat,
                CredentialKind::Oauth,
            ),
            ClientSimulationContext::new(
                ChannelType::Anthropic,
                Protocol::OpenAiChat,
                Operation::Chat,
                CredentialKind::Oauth,
            ),
            ClientSimulationContext::new(
                ChannelType::Anthropic,
                Protocol::Anthropic,
                Operation::Responses,
                CredentialKind::Oauth,
            ),
            ClientSimulationContext::new(
                ChannelType::Anthropic,
                Protocol::Anthropic,
                Operation::Chat,
                CredentialKind::ApiKey,
            ),
        ] {
            assert_eq!(
                middleware.prepare_headers(context),
                Err(AdaptorError::InvalidClientSimulation)
            );
        }
    }

    #[test]
    fn middleware_cannot_generate_empty_or_non_identity_header_values() {
        for invalid in ["", " leading", "trailing ", "line\nbreak", "非 ASCII"] {
            assert_eq!(
                ClientSimulationHeaders::new().with_user_agent(invalid),
                Err(AdaptorError::InvalidClientSimulation)
            );
        }
        assert_eq!(
            ClientSimulationHeaders::new()
                .with_x_app(&"x".repeat(MAX_CLIENT_SIMULATION_HEADER_VALUE_BYTES + 1)),
            Err(AdaptorError::InvalidClientSimulation)
        );
        let debug = format!(
            "{:?}",
            ClientSimulationHeaders::new()
                .with_user_agent("private-profile-value")
                .unwrap()
        );
        assert!(debug.contains("has_user_agent"));
        assert!(!debug.contains("private-profile-value"));
    }

    #[test]
    fn empty_plugin_output_is_rejected_before_request_changes() {
        struct EmptyMiddleware;

        impl ClientSimulationMiddleware for EmptyMiddleware {
            fn profile(&self) -> ClientSimulationProfile {
                ClientSimulationProfile::AnthropicCliHeadersV1
            }

            fn prepare_headers(
                &self,
                _context: ClientSimulationContext,
            ) -> AdaptorResult<ClientSimulationHeaders> {
                Ok(ClientSimulationHeaders::new())
            }
        }

        let request = UpstreamRequest::new(
            Method::POST,
            "https://api.anthropic.com/v1/messages",
            HeaderMap::new(),
            None,
        )
        .unwrap();
        assert_eq!(
            apply_client_simulation(&EmptyMiddleware, anthropic_oauth_context(), request)
                .unwrap_err(),
            AdaptorError::InvalidClientSimulation
        );
    }
}
