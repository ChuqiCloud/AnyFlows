use std::fmt;

use af_domain::{ChannelType, CredentialKind, Operation, Protocol};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};

use crate::credential::clear_authentication_headers;
use crate::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, Credential, RelayContext, ResponseMode,
};

/// Cohere v2 原生 Rerank 渠道适配器。
///
/// 适配器只负责官方 `/v2/rerank` 目标与 Bearer API Key，不解释请求或响应正文。基础
/// 地址可为服务根、以 `v2` 结尾的地址或可信代理前缀；其他操作和流式模式失败关闭。
#[derive(Clone, Eq, PartialEq)]
pub struct CohereAdaptor {
    supported_models: Vec<String>,
}

impl Default for CohereAdaptor {
    fn default() -> Self {
        Self::new()
    }
}

impl CohereAdaptor {
    /// Cohere 官方 API 默认根地址。
    pub const DEFAULT_BASE_URL: &'static str = "https://api.cohere.com";

    /// 创建不携带内建模型限制的 Cohere 适配器。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            supported_models: Vec::new(),
        }
    }

    /// 创建带渠道模型清单的 Cohere 适配器。
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

    fn endpoint_path(
        &self,
        context: &RelayContext,
        target: AdaptorTarget<'_>,
    ) -> AdaptorResult<&'static str> {
        if target.operation() != Operation::Rerank {
            return Err(AdaptorError::UnsupportedOperation {
                operation: target.operation(),
            });
        }
        if target.response_mode() != ResponseMode::Full {
            return Err(AdaptorError::UnsupportedResponseMode);
        }
        let base_url = context.resolve_base_url(Self::DEFAULT_BASE_URL)?;
        let has_v2_suffix = base_url
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .is_some_and(|segment| segment == "v2");
        Ok(if has_v2_suffix { "rerank" } else { "v2/rerank" })
    }
}

impl Adaptor for CohereAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Cohere
    }

    fn default_protocol(&self) -> Protocol {
        Protocol::CohereRerank
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
        context.append_path(Self::DEFAULT_BASE_URL, self.endpoint_path(context, target)?)
    }

    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        _context: &RelayContext,
    ) -> AdaptorResult<()> {
        if credential.kind() != CredentialKind::ApiKey {
            return Err(AdaptorError::UnsupportedCredential {
                kind: credential.kind(),
            });
        }
        clear_authentication_headers(headers);
        let mut authorization =
            HeaderValue::from_str(&format!("Bearer {}", credential.expose_secret()))
                .map_err(|_| AdaptorError::InvalidHeader)?;
        authorization.set_sensitive(true);
        headers.insert(HeaderName::from_static("authorization"), authorization);
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("x-client-name"),
            HeaderValue::from_static("AnyFlows"),
        );
        Ok(())
    }
}

impl fmt::Debug for CohereAdaptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CohereAdaptor")
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

    fn target(operation: Operation, response_mode: ResponseMode) -> AdaptorTarget<'static> {
        AdaptorTarget::new("private-cohere-model", operation, response_mode)
    }

    #[test]
    fn metadata_url_and_debug_are_stable_and_redacted() {
        let adaptor = CohereAdaptor::with_supported_models(["rerank-private-a"]);
        assert_eq!(adaptor.channel_type(), ChannelType::Cohere);
        assert_eq!(adaptor.default_protocol(), Protocol::CohereRerank);
        assert_eq!(adaptor.default_base_url(), "https://api.cohere.com");
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Rerank, ResponseMode::Full))
                .unwrap(),
            "https://api.cohere.com/v2/rerank"
        );
        let proxied = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/cohere/v2/")
                    .unwrap(),
                target(Operation::Rerank, ResponseMode::Full),
            )
            .unwrap();
        assert!(proxied.ends_with("/cohere/v2/rerank"));
        let debug = format!("{adaptor:?}");
        assert!(debug.contains("supported_model_count: 1"));
        assert!(!debug.contains("rerank-private-a"));
    }

    #[test]
    fn headers_use_bearer_api_key_and_reject_oauth() {
        let adaptor = CohereAdaptor::new();
        let mut headers = HeaderMap::new();
        adaptor
            .setup_headers(
                &mut headers,
                &Credential::api_key("private-cohere-key").unwrap(),
                &context(),
            )
            .unwrap();
        assert_eq!(headers["authorization"], "Bearer private-cohere-key");
        assert_eq!(headers["x-client-name"], "AnyFlows");
        assert!(!format!("{headers:?}").contains("private-cohere-key"));
        assert_eq!(
            adaptor
                .setup_headers(
                    &mut HeaderMap::new(),
                    &Credential::oauth("private-oauth").unwrap(),
                    &context(),
                )
                .unwrap_err(),
            AdaptorError::UnsupportedCredential {
                kind: CredentialKind::Oauth
            }
        );
    }
}
