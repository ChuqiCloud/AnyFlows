use std::fmt;

use af_domain::{ChannelType, Operation, Protocol};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};

use crate::credential::{api_key_or_bearer_header, clear_authentication_headers};
use crate::{Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, Credential, RelayContext};

/// Anthropic Messages 官方及标准兼容渠道适配器。
///
/// 渠道基础地址可使用服务根或反向代理前缀；末尾非空段精确为 `v1` 时只追加
/// `messages`，其他路径追加 `v1/messages`。API Key 使用 `X-Api-Key`，OAuth
/// access token 使用 Bearer，二者不会同时发送。
#[derive(Clone, Default, Eq, PartialEq)]
pub struct AnthropicAdaptor {
    supported_models: Vec<String>,
}

impl AnthropicAdaptor {
    /// Anthropic 官方 API 的默认根地址。
    pub const DEFAULT_BASE_URL: &'static str = "https://api.anthropic.com";
    /// Anthropic 官方 SDK 当前使用的稳定 API 版本。
    pub const DEFAULT_API_VERSION: &'static str = "2023-06-01";

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

    fn endpoint_path(&self, context: &RelayContext) -> AdaptorResult<&'static str> {
        let base_url = context.resolve_base_url(Self::DEFAULT_BASE_URL)?;
        let has_v1_suffix = base_url
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .is_some_and(|segment| segment == "v1");
        Ok(if has_v1_suffix {
            "messages"
        } else {
            "v1/messages"
        })
    }
}

impl Adaptor for AnthropicAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Anthropic
    }

    fn default_protocol(&self) -> Protocol {
        Protocol::Anthropic
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
        context.append_path(Self::DEFAULT_BASE_URL, self.endpoint_path(context)?)
    }

    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()> {
        let (authentication_name, authentication_value) =
            api_key_or_bearer_header(credential, HeaderName::from_static("x-api-key"))?;
        clear_authentication_headers(headers);
        headers.insert(authentication_name, authentication_value);
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static(Self::DEFAULT_API_VERSION),
        );
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

impl fmt::Debug for AnthropicAdaptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicAdaptor")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
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

    fn target(operation: Operation) -> AdaptorTarget<'static> {
        AdaptorTarget::new("claude-test", operation, crate::ResponseMode::Full)
    }

    #[test]
    fn anthropic_metadata_models_and_debug_are_stable() {
        let adaptor =
            AnthropicAdaptor::with_supported_models(["claude-sonnet-test", "claude-opus-test"]);
        assert_eq!(adaptor.channel_type(), ChannelType::Anthropic);
        assert_eq!(adaptor.default_protocol(), Protocol::Anthropic);
        assert_eq!(adaptor.default_base_url(), "https://api.anthropic.com");
        assert_eq!(
            adaptor.supported_models(),
            ["claude-sonnet-test", "claude-opus-test"]
        );
        let debug = format!("{adaptor:?}");
        assert!(debug.contains("supported_model_count: 2"));
        assert!(!debug.contains("claude-sonnet-test"));
        assert!(!debug.contains("claude-opus-test"));
    }

    #[test]
    fn build_url_supports_root_and_v1_base_paths() {
        let adaptor = AnthropicAdaptor::new();
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Chat))
                .unwrap(),
            "https://api.anthropic.com/v1/messages"
        );
        for base_url in [
            "https://gateway.example/proxy/anthropic",
            "https://gateway.example/proxy/anthropic/v1",
            "https://gateway.example/proxy/anthropic/v1/",
        ] {
            let target = adaptor
                .build_url(
                    &context().with_base_url(base_url).unwrap(),
                    target(Operation::Chat),
                )
                .unwrap();
            assert!(target.ends_with("/proxy/anthropic/v1/messages"));
        }
        let uppercase_v1 = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/anthropic/V1/")
                    .unwrap(),
                target(Operation::Chat),
            )
            .unwrap();
        assert!(uppercase_v1.ends_with("/proxy/anthropic/V1/v1/messages"));
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Responses))
                .unwrap_err(),
            AdaptorError::UnsupportedOperation {
                operation: Operation::Responses
            }
        );
    }

    #[test]
    fn setup_headers_isolates_api_key_and_oauth_authentication() {
        let adaptor = AnthropicAdaptor::new();
        let context = context().with_request_id("request-anthropic-1").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer stale-oauth"),
        );
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("stale-version"),
        );
        adaptor
            .setup_headers(
                &mut headers,
                &Credential::api_key("anthropic-api-key").unwrap(),
                &context,
            )
            .unwrap();
        assert_eq!(headers["x-api-key"], "anthropic-api-key");
        assert!(headers.get("authorization").is_none());
        assert_eq!(headers["anthropic-version"], "2023-06-01");
        assert_eq!(headers["content-type"], "application/json");
        assert_eq!(headers["accept"], "application/json");
        assert_eq!(headers["x-request-id"], "request-anthropic-1");
        assert!(!format!("{headers:?}").contains("anthropic-api-key"));

        adaptor
            .setup_headers(
                &mut headers,
                &Credential::oauth("anthropic-oauth-token").unwrap(),
                &context,
            )
            .unwrap();
        assert_eq!(headers["authorization"], "Bearer anthropic-oauth-token");
        assert!(headers.get("x-api-key").is_none());
        assert_eq!(headers["anthropic-version"], "2023-06-01");
        assert!(!format!("{headers:?}").contains("anthropic-oauth-token"));
    }
}
