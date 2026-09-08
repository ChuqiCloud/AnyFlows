use std::fmt;

use af_domain::{ChannelType, CredentialKind, Operation, Protocol};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};

use crate::credential::clear_authentication_headers;
use crate::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, Credential, RelayContext, ResponseMode,
};

/// Jina 原生 Rerank 渠道适配器。
///
/// 适配器只负责原生 `/v1/rerank` 目标与 Bearer 认证，不解释请求或响应正文。基础地址可为
/// 服务根、以 `v1` 结尾的地址或可信反向代理前缀；流式模式和非 Rerank 操作均失败关闭。
#[derive(Clone, Eq, PartialEq)]
pub struct JinaAdaptor {
    supported_models: Vec<String>,
}

impl Default for JinaAdaptor {
    fn default() -> Self {
        Self::new()
    }
}

impl JinaAdaptor {
    /// Jina 官方 API 的默认根地址。
    pub const DEFAULT_BASE_URL: &'static str = "https://api.jina.ai";

    /// 创建不携带内建模型限制的 Jina 适配器。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            supported_models: Vec::new(),
        }
    }

    /// 创建带渠道模型清单的 Jina 适配器；模型边界由上层配置校验。
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
        let has_v1_suffix = base_url
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .is_some_and(|segment| segment == "v1");
        Ok(if has_v1_suffix { "rerank" } else { "v1/rerank" })
    }
}

impl Adaptor for JinaAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Jina
    }

    fn default_protocol(&self) -> Protocol {
        Protocol::JinaRerank
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
        context: &RelayContext,
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
        if let Some(request_id) = context.request_id() {
            let value =
                HeaderValue::from_str(request_id).map_err(|_| AdaptorError::InvalidHeader)?;
            headers.insert(HeaderName::from_static("x-request-id"), value);
        }
        Ok(())
    }
}

impl fmt::Debug for JinaAdaptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JinaAdaptor")
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
        AdaptorTarget::new("private-jina-model", operation, response_mode)
    }

    #[test]
    fn metadata_models_and_debug_are_isolated() {
        let adaptor = JinaAdaptor::with_supported_models([
            "jina-reranker-private-a",
            "jina-reranker-private-b",
        ]);
        assert_eq!(adaptor.channel_type(), ChannelType::Jina);
        assert_eq!(adaptor.default_protocol(), Protocol::JinaRerank);
        assert_eq!(adaptor.default_base_url(), "https://api.jina.ai");
        assert_eq!(
            adaptor.supported_models(),
            ["jina-reranker-private-a", "jina-reranker-private-b"]
        );

        let debug = format!("{adaptor:?}");
        assert!(debug.contains("supported_model_count: 2"));
        assert!(!debug.contains("jina-reranker-private-a"));
        assert!(!debug.contains("jina-reranker-private-b"));
    }

    #[test]
    fn build_url_supports_root_v1_and_proxy_prefixes() {
        let adaptor = JinaAdaptor::new();
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Rerank, ResponseMode::Full))
                .unwrap(),
            "https://api.jina.ai/v1/rerank"
        );

        for base_url in [
            "https://gateway.example/proxy/jina",
            "https://gateway.example/proxy/jina/v1",
            "https://gateway.example/proxy/jina/v1/",
        ] {
            let built = adaptor
                .build_url(
                    &context().with_base_url(base_url).unwrap(),
                    target(Operation::Rerank, ResponseMode::Full),
                )
                .unwrap();
            assert!(built.ends_with("/proxy/jina/v1/rerank"));
        }

        let uppercase = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/jina/V1/")
                    .unwrap(),
                target(Operation::Rerank, ResponseMode::Full),
            )
            .unwrap();
        assert!(uppercase.ends_with("/proxy/jina/V1/v1/rerank"));
    }

    #[test]
    fn build_url_rejects_non_rerank_and_streaming_targets() {
        let adaptor = JinaAdaptor::new();
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Chat, ResponseMode::Full))
                .unwrap_err(),
            AdaptorError::UnsupportedOperation {
                operation: Operation::Chat
            }
        );
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Rerank, ResponseMode::Stream))
                .unwrap_err(),
            AdaptorError::UnsupportedResponseMode
        );
    }

    #[test]
    fn setup_headers_writes_bearer_json_request_id_and_rejects_oauth() {
        let adaptor = JinaAdaptor::new();
        let context = context().with_request_id("request-jina-1").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("stale-secret"),
        );
        adaptor
            .setup_headers(
                &mut headers,
                &Credential::api_key("private-jina-key").unwrap(),
                &context,
            )
            .unwrap();
        assert_eq!(headers["authorization"], "Bearer private-jina-key");
        assert_eq!(headers["content-type"], "application/json");
        assert_eq!(headers["accept"], "application/json");
        assert_eq!(headers["x-request-id"], "request-jina-1");
        assert!(headers.get("x-api-key").is_none());
        assert!(!format!("{headers:?}").contains("private-jina-key"));

        let error = adaptor
            .setup_headers(
                &mut HeaderMap::new(),
                &Credential::oauth("private-oauth-token").unwrap(),
                &context,
            )
            .unwrap_err();
        assert_eq!(
            error,
            AdaptorError::UnsupportedCredential {
                kind: CredentialKind::Oauth
            }
        );
        assert!(!format!("{error:?}\n{error}").contains("private-oauth-token"));
    }
}
