use std::fmt;

use af_domain::{ChannelType, CredentialKind, Operation, Protocol};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};

use crate::{Adaptor, AdaptorError, AdaptorResult, Credential, RelayContext};

/// OpenAI Chat Completions 及兼容渠道适配器。
///
/// 渠道基础地址由 `RelayContext` 覆盖，可传服务根或反向代理前缀；末尾非空段精确为
/// `v1`（可带尾斜杠）时不会重复追加版本路径，其他路径会追加 `/v1/chat/completions`。
/// 空模型清单表示无内建模型限制，具体模型由渠道配置提供。当前只实现 Bearer-compatible
/// OpenAI Chat 的 URL 与认证 Header 构造，Azure deployment 等专用参数留待后续适配器切片。
#[derive(Clone, Default, Eq, PartialEq)]
pub struct OpenAiAdaptor {
    supported_models: Vec<String>,
}

impl OpenAiAdaptor {
    /// OpenAI 官方 API 的默认根地址。
    pub const DEFAULT_BASE_URL: &'static str = "https://api.openai.com";

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
            "chat/completions"
        } else {
            "v1/chat/completions"
        })
    }
}

impl Adaptor for OpenAiAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::OpenAi
    }

    fn default_protocol(&self) -> Protocol {
        Protocol::OpenAiChat
    }

    fn default_base_url(&self) -> &str {
        Self::DEFAULT_BASE_URL
    }

    fn supported_models(&self) -> Vec<String> {
        self.supported_models.clone()
    }

    fn build_url(&self, context: &RelayContext, operation: Operation) -> AdaptorResult<String> {
        if operation != Operation::Chat {
            return Err(AdaptorError::UnsupportedOperation { operation });
        }
        context.append_path(Self::DEFAULT_BASE_URL, self.endpoint_path(context)?)
    }

    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()> {
        if !matches!(
            credential.kind(),
            CredentialKind::ApiKey | CredentialKind::Oauth
        ) {
            return Err(AdaptorError::UnsupportedCredential {
                kind: credential.kind(),
            });
        }

        for name in [
            HeaderName::from_static("api-key"),
            HeaderName::from_static("x-api-key"),
            HeaderName::from_static("x-goog-api-key"),
            HeaderName::from_static("x-auth-token"),
            HeaderName::from_static("x-access-token"),
            HeaderName::from_static("x-client-secret"),
            HeaderName::from_static("x-amz-security-token"),
        ] {
            headers.remove(name);
        }

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

impl fmt::Debug for OpenAiAdaptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiAdaptor")
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

    #[test]
    fn openai_metadata_and_models_are_stable() {
        let adaptor = OpenAiAdaptor::with_supported_models(["gpt-test", "deepseek-test"]);
        assert_eq!(adaptor.channel_type(), ChannelType::OpenAi);
        assert_eq!(adaptor.default_protocol(), Protocol::OpenAiChat);
        assert_eq!(adaptor.default_base_url(), "https://api.openai.com");
        assert_eq!(adaptor.supported_models(), ["gpt-test", "deepseek-test"]);
    }

    #[test]
    fn build_url_supports_root_and_v1_base_paths() {
        let adaptor = OpenAiAdaptor::new();
        assert_eq!(
            adaptor.build_url(&context(), Operation::Chat).unwrap(),
            "https://api.openai.com/v1/chat/completions"
        );
        for base_url in [
            "https://gateway.example/proxy/openai",
            "https://gateway.example/proxy/openai/v1",
            "https://gateway.example/proxy/openai/v1/",
        ] {
            let target = adaptor
                .build_url(&context().with_base_url(base_url).unwrap(), Operation::Chat)
                .unwrap();
            assert!(target.ends_with("/proxy/openai/v1/chat/completions"));
        }
        let uppercase_v1 = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/openai/V1/")
                    .unwrap(),
                Operation::Chat,
            )
            .unwrap();
        assert!(uppercase_v1.ends_with("/proxy/openai/V1/v1/chat/completions"));
        assert_eq!(
            adaptor
                .build_url(&context(), Operation::Responses)
                .unwrap_err(),
            AdaptorError::UnsupportedOperation {
                operation: Operation::Responses
            }
        );
    }

    #[test]
    fn setup_headers_writes_bearer_json_and_request_id_headers() {
        let adaptor = OpenAiAdaptor::new();
        let context = context().with_request_id("request-openai-1").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer stale"),
        );
        headers.insert(
            HeaderName::from_static("api-key"),
            HeaderValue::from_static("stale-api-key"),
        );
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("stale-x-api-key"),
        );
        headers.insert(
            HeaderName::from_static("x-goog-api-key"),
            HeaderValue::from_static("stale-google-key"),
        );
        headers.insert(
            HeaderName::from_static("x-auth-token"),
            HeaderValue::from_static("stale-auth-token"),
        );
        headers.insert(
            HeaderName::from_static("x-access-token"),
            HeaderValue::from_static("stale-access-token"),
        );
        headers.insert(
            HeaderName::from_static("x-client-secret"),
            HeaderValue::from_static("stale-client-secret"),
        );
        headers.insert(
            HeaderName::from_static("x-amz-security-token"),
            HeaderValue::from_static("stale-amz-token"),
        );
        adaptor
            .setup_headers(
                &mut headers,
                &Credential::oauth("oauth-access").unwrap(),
                &context,
            )
            .unwrap();
        assert_eq!(headers["authorization"], "Bearer oauth-access");
        assert_eq!(headers["content-type"], "application/json");
        assert_eq!(headers["accept"], "application/json");
        assert_eq!(headers["x-request-id"], "request-openai-1");
        assert!(headers.get("api-key").is_none());
        assert!(headers.get("x-api-key").is_none());
        assert!(headers.get("x-goog-api-key").is_none());
        assert!(headers.get("x-auth-token").is_none());
        assert!(headers.get("x-access-token").is_none());
        assert!(headers.get("x-client-secret").is_none());
        assert!(headers.get("x-amz-security-token").is_none());
        let debug = format!("{headers:?}");
        assert!(!debug.contains("oauth-access"));
    }

    #[test]
    fn debug_redacts_configured_model_names() {
        let adaptor = OpenAiAdaptor::with_supported_models(["private-model"]);
        let debug = format!("{adaptor:?}");
        assert!(!debug.contains("private-model"));
        assert!(debug.contains("supported_model_count"));
    }
}
