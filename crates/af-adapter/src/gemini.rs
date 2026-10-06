use std::fmt;

use af_domain::{ChannelType, MAX_MODEL_NAME_BYTES, Operation, Protocol};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};

use crate::credential::{api_key_or_bearer_header, clear_authentication_headers};
use crate::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, Credential,
    MAX_UPSTREAM_REQUEST_TARGET_BYTES, RelayContext, ResponseMode,
};

/// Gemini Developer API 官方及原生协议兼容渠道适配器。
///
/// 模型只进入官方 `models/{model}:generateContent` 路径，正文不重复携带。API Key
/// 使用 `X-Goog-Api-Key`，OAuth access token 使用 Bearer，二者不会同时发送。
#[derive(Clone, Default, Eq, PartialEq)]
pub struct GeminiAdaptor {
    supported_models: Vec<String>,
}

impl GeminiAdaptor {
    /// Gemini Developer API 的默认服务根地址。
    pub const DEFAULT_BASE_URL: &'static str = "https://generativelanguage.googleapis.com";
    /// 当前原生协议转换边界对应的官方 API 版本。
    pub const DEFAULT_API_VERSION: &'static str = "v1beta";

    /// 创建不携带内建模型清单的适配器。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            supported_models: Vec::new(),
        }
    }

    /// 创建带渠道模型清单的适配器；模型名由上层配置边界负责校验。
    #[must_use]
    pub fn with_supported_models<I, S>(models: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            supported_models: models.into_iter().map(Into::into).collect(),
        }
    }

    fn build_generate_content_url(
        &self,
        context: &RelayContext,
        model: &str,
        response_mode: ResponseMode,
    ) -> AdaptorResult<String> {
        if !is_valid_model(model) {
            return Err(AdaptorError::InvalidRequestTarget);
        }

        let mut target = context.resolve_base_url(Self::DEFAULT_BASE_URL)?;
        let has_version_suffix = target
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .is_some_and(|segment| segment == Self::DEFAULT_API_VERSION);
        let action = match response_mode {
            ResponseMode::Full => "generateContent",
            ResponseMode::Stream => "streamGenerateContent",
        };
        let model_action = format!("{model}:{action}");
        {
            // 模型作为独立段交给 URL 库编码，不能通过字符串拼接注入路径或查询串。
            let mut segments = target
                .path_segments_mut()
                .map_err(|_| AdaptorError::InvalidBaseUrl)?;
            segments.pop_if_empty();
            if !has_version_suffix {
                segments.push(Self::DEFAULT_API_VERSION);
            }
            segments.push("models");
            segments.push(&model_action);
        }
        if response_mode == ResponseMode::Stream {
            target.query_pairs_mut().append_pair("alt", "sse");
        }

        let target = String::from(target);
        if target.len() > MAX_UPSTREAM_REQUEST_TARGET_BYTES {
            return Err(AdaptorError::InvalidRequestTarget);
        }
        Ok(target)
    }
}

impl Adaptor for GeminiAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Gemini
    }

    fn default_protocol(&self) -> Protocol {
        Protocol::Gemini
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
        if target.operation() != Operation::Chat {
            return Err(AdaptorError::UnsupportedOperation {
                operation: target.operation(),
            });
        }
        self.build_generate_content_url(context, target.model(), target.response_mode())
    }

    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()> {
        let (authentication_name, authentication_value) =
            api_key_or_bearer_header(credential, HeaderName::from_static("x-goog-api-key"))?;
        clear_authentication_headers(headers);
        headers.insert(authentication_name, authentication_value);
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

impl fmt::Debug for GeminiAdaptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeminiAdaptor")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}

fn is_valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.contains('/')
        && !model.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use af_httpclient::{HttpClientConfig, HttpClientPool};

    use super::*;

    fn context() -> RelayContext {
        RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        )
    }

    fn target(model: &str, operation: Operation, response_mode: ResponseMode) -> AdaptorTarget<'_> {
        AdaptorTarget::new(model, operation, response_mode)
    }

    #[test]
    fn gemini_metadata_models_and_debug_are_stable() {
        let adaptor =
            GeminiAdaptor::with_supported_models(["gemini-flash-test", "gemini-pro-test"]);
        assert_eq!(adaptor.channel_type(), ChannelType::Gemini);
        assert_eq!(adaptor.default_protocol(), Protocol::Gemini);
        assert_eq!(
            adaptor.default_base_url(),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            adaptor.supported_models(),
            ["gemini-flash-test", "gemini-pro-test"]
        );
        let debug = format!("{adaptor:?}");
        assert!(debug.contains("supported_model_count: 2"));
        assert!(!debug.contains("gemini-flash-test"));
        assert!(!debug.contains("gemini-pro-test"));
    }

    #[test]
    fn build_url_supports_full_stream_and_versioned_proxy_paths() {
        let adaptor = GeminiAdaptor::new();
        assert_eq!(
            adaptor
                .build_url(
                    &context(),
                    target("gemini-2.5-flash", Operation::Chat, ResponseMode::Full),
                )
                .unwrap(),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );
        assert_eq!(
            adaptor
                .build_url(
                    &context(),
                    target("gemini-2.5-flash", Operation::Chat, ResponseMode::Stream,),
                )
                .unwrap(),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
        for base_url in [
            "https://gateway.example/proxy/gemini",
            "https://gateway.example/proxy/gemini/v1beta",
            "https://gateway.example/proxy/gemini/v1beta/",
        ] {
            let target = adaptor
                .build_url(
                    &context().with_base_url(base_url).unwrap(),
                    target("gemini-test", Operation::Chat, ResponseMode::Full),
                )
                .unwrap();
            assert!(target.ends_with("/proxy/gemini/v1beta/models/gemini-test:generateContent"));
        }
        let uppercase_version = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/gemini/V1BETA/")
                    .unwrap(),
                target("gemini-test", Operation::Chat, ResponseMode::Full),
            )
            .unwrap();
        assert!(
            uppercase_version
                .ends_with("/proxy/gemini/V1BETA/v1beta/models/gemini-test:generateContent")
        );

        let encoded_model = adaptor
            .build_url(
                &context(),
                target(
                    "gemini?mode=test#fragment",
                    Operation::Chat,
                    ResponseMode::Full,
                ),
            )
            .unwrap();
        assert!(encoded_model.contains("/gemini%3Fmode=test%23fragment:generateContent"));
        assert!(!encoded_model.contains("?mode="));
        assert!(!encoded_model.contains("#fragment"));
    }

    #[test]
    fn build_url_rejects_invalid_model_and_unsupported_operation_without_leaks() {
        let adaptor = GeminiAdaptor::new();
        let private_model = "private/model-canary";
        for model in [
            "",
            " gemini-test",
            "gemini-test ",
            private_model,
            "gemini\nsecret",
        ] {
            let error = adaptor
                .build_url(
                    &context(),
                    target(model, Operation::Chat, ResponseMode::Full),
                )
                .unwrap_err();
            assert_eq!(error, AdaptorError::InvalidRequestTarget);
            if !model.is_empty() {
                assert!(!format!("{error:?}\n{error}").contains(model));
            }
        }
        assert_eq!(
            adaptor
                .build_url(
                    &context(),
                    target("gemini-test", Operation::Responses, ResponseMode::Full),
                )
                .unwrap_err(),
            AdaptorError::UnsupportedOperation {
                operation: Operation::Responses
            }
        );
        let oversized = "m".repeat(MAX_MODEL_NAME_BYTES + 1);
        assert_eq!(
            adaptor
                .build_url(
                    &context(),
                    target(&oversized, Operation::Chat, ResponseMode::Full),
                )
                .unwrap_err(),
            AdaptorError::InvalidRequestTarget
        );
    }

    #[test]
    fn setup_headers_isolates_api_key_and_oauth_authentication() {
        let adaptor = GeminiAdaptor::new();
        let context = context().with_request_id("request-gemini-1").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer stale-oauth"),
        );
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("stale-provider-key"),
        );
        adaptor
            .setup_headers(
                &mut headers,
                &Credential::api_key("gemini-api-key").unwrap(),
                &context,
            )
            .unwrap();
        assert_eq!(headers["x-goog-api-key"], "gemini-api-key");
        assert!(headers.get("authorization").is_none());
        assert!(headers.get("x-api-key").is_none());
        assert_eq!(headers["content-type"], "application/json");
        assert_eq!(headers["accept"], "application/json");
        assert_eq!(headers["x-request-id"], "request-gemini-1");
        assert!(!format!("{headers:?}").contains("gemini-api-key"));

        adaptor
            .setup_headers(
                &mut headers,
                &Credential::oauth("gemini-oauth-token").unwrap(),
                &context,
            )
            .unwrap();
        assert_eq!(headers["authorization"], "Bearer gemini-oauth-token");
        assert!(headers.get("x-goog-api-key").is_none());
        assert!(!format!("{headers:?}").contains("gemini-oauth-token"));
    }
}
