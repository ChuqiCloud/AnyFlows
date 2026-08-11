use std::fmt;

use af_domain::{ChannelType, Operation, Protocol};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue, Method};
use async_trait::async_trait;

use crate::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, Credential, RelayContext, ResponseMode,
    UpstreamRequest,
};

mod auth;
mod endpoint;
#[cfg(test)]
mod tests;

pub use auth::{
    CUSTOM_CREDENTIAL_PLACEHOLDER, CustomAuthentication, CustomHeaderAuthentication,
    MAX_CUSTOM_AUTH_TEMPLATE_BYTES,
};
pub use endpoint::{
    CUSTOM_MODEL_PLACEHOLDER, CustomEndpointTemplate, MAX_CUSTOM_ENDPOINT_PATH_SEGMENTS,
    MAX_CUSTOM_ENDPOINT_QUERY_PAIRS, MAX_CUSTOM_ENDPOINT_TEMPLATE_BYTES,
};

/// Custom 流式请求使用的端点策略。
#[derive(Clone, Eq, PartialEq)]
pub enum CustomStreamEndpoint {
    /// 流式和非流式请求使用相同端点。
    Same,
    /// 流式请求使用独立相对端点模板。
    Separate(CustomEndpointTemplate),
    /// 当前渠道不支持流式请求。
    Unsupported,
}

impl fmt::Debug for CustomStreamEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Same => formatter.write_str("CustomStreamEndpoint::Same"),
            Self::Separate(endpoint) => formatter
                .debug_tuple("CustomStreamEndpoint::Separate")
                .field(endpoint)
                .finish(),
            Self::Unsupported => formatter.write_str("CustomStreamEndpoint::Unsupported"),
        }
    }
}

/// 配置驱动的 Custom JSON API 适配器。
///
/// 协议转换仍由 `af-protocol` 完成；本类型只选择已验证的相对端点并重建认证。
/// Custom 没有供应商默认主机，调用方必须通过 [`RelayContext`] 提供基础地址。
#[derive(Clone, Eq, PartialEq)]
pub struct CustomAdaptor {
    protocol: Protocol,
    endpoint: CustomEndpointTemplate,
    stream_endpoint: CustomStreamEndpoint,
    authentication: CustomAuthentication,
    supported_models: Vec<String>,
}

impl CustomAdaptor {
    /// Custom 不提供隐式上游主机，空值用于要求显式渠道基础地址。
    pub const DEFAULT_BASE_URL: &'static str = "";

    /// 创建不携带内建模型清单的 Custom 适配器。
    #[must_use]
    pub const fn new(
        protocol: Protocol,
        endpoint: CustomEndpointTemplate,
        stream_endpoint: CustomStreamEndpoint,
        authentication: CustomAuthentication,
    ) -> Self {
        Self {
            protocol,
            endpoint,
            stream_endpoint,
            authentication,
            supported_models: Vec::new(),
        }
    }

    /// 创建带渠道模型清单的 Custom 适配器。
    #[must_use]
    pub fn with_supported_models<I, S>(
        protocol: Protocol,
        endpoint: CustomEndpointTemplate,
        stream_endpoint: CustomStreamEndpoint,
        authentication: CustomAuthentication,
        models: I,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            protocol,
            endpoint,
            stream_endpoint,
            authentication,
            supported_models: models.into_iter().map(Into::into).collect(),
        }
    }

    fn expected_operation(&self) -> Operation {
        match self.protocol {
            Protocol::OpenAiResponses => Operation::Responses,
            Protocol::OpenAiEmbeddings => Operation::Embedding,
            Protocol::OpenAiImages => Operation::Image,
            Protocol::OpenAiAudio | Protocol::OpenAiSpeech => Operation::Audio,
            Protocol::JinaRerank | Protocol::CohereRerank => Operation::Rerank,
            Protocol::XaiVideo => Operation::Video,
            Protocol::OpenAiChat | Protocol::Anthropic | Protocol::Gemini => Operation::Chat,
        }
    }

    fn endpoint(&self, response_mode: ResponseMode) -> AdaptorResult<&CustomEndpointTemplate> {
        match (response_mode, &self.stream_endpoint) {
            (ResponseMode::Full, _) | (ResponseMode::Stream, CustomStreamEndpoint::Same) => {
                Ok(&self.endpoint)
            }
            (ResponseMode::Stream, CustomStreamEndpoint::Separate(endpoint)) => Ok(endpoint),
            (ResponseMode::Stream, CustomStreamEndpoint::Unsupported) => {
                Err(AdaptorError::UnsupportedResponseMode)
            }
        }
    }

    fn apply_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()> {
        self.authentication.apply(headers, credential)?;
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
        if let Some(request_id) = context.request_id() {
            let value =
                HeaderValue::from_str(request_id).map_err(|_| AdaptorError::InvalidHeader)?;
            headers.insert(HeaderName::from_static("x-request-id"), value);
        }
        Ok(())
    }
}

#[async_trait]
impl Adaptor for CustomAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom
    }

    fn default_protocol(&self) -> Protocol {
        self.protocol
    }

    fn default_base_url(&self) -> &str {
        Self::DEFAULT_BASE_URL
    }

    fn supported_models(&self) -> Vec<String> {
        self.supported_models.clone()
    }

    fn build_url(
        &self,
        context: &RelayContext,
        target: AdaptorTarget<'_>,
    ) -> AdaptorResult<String> {
        let expected = self.expected_operation();
        if target.operation() != expected {
            return Err(AdaptorError::UnsupportedOperation {
                operation: target.operation(),
            });
        }
        self.endpoint(target.response_mode())?
            .render(context, target.model())
    }

    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()> {
        self.apply_headers(headers, credential, context)
    }

    async fn finalize_request(
        &self,
        request: UpstreamRequest,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        let (method, target, mut headers, body, response_mode, response_body_limit) =
            request.into_parts();
        if method != Method::POST {
            return Err(AdaptorError::UnsupportedRequestMethod);
        }
        // 渠道 Header 覆盖已结束；此处重新建立唯一认证和 JSON 基线。
        self.apply_headers(&mut headers, credential, context)?;
        UpstreamRequest::new(method, target, headers, body)
            .and_then(|request| request.with_response_body_limit(response_body_limit))
            .map(|request| request.with_response_mode(response_mode))
    }
}

impl fmt::Debug for CustomAdaptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomAdaptor")
            .field("protocol", &self.protocol)
            .field("endpoint", &self.endpoint)
            .field("stream_endpoint", &self.stream_endpoint)
            .field("authentication", &self.authentication)
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}
