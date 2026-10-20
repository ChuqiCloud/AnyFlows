use std::fmt;

use af_domain::{ChannelType, CredentialKind, Operation, Protocol};
use af_httpclient::HeaderMap;
use async_trait::async_trait;

use crate::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, Bytes, Credential, OpenAiAdaptor,
    RelayContext, UpstreamRequest,
};

/// NVIDIA API Catalog 和自建 NIM 的 Chat Completions 适配器。
#[derive(Clone, Eq, PartialEq)]
pub struct NvidiaAdaptor {
    inner: OpenAiAdaptor,
}

impl NvidiaAdaptor {
    /// NVIDIA 托管 API 的默认服务根地址。
    pub const DEFAULT_BASE_URL: &'static str = "https://integrate.api.nvidia.com";

    /// 模型清单由渠道提供，保留 NVIDIA 的完整命名空间。
    #[must_use]
    pub fn with_supported_models(models: Vec<String>) -> Self {
        Self {
            inner: OpenAiAdaptor::with_supported_models(models),
        }
    }
}

#[async_trait]
impl Adaptor for NvidiaAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Nvidia
    }

    fn default_protocol(&self) -> Protocol {
        Protocol::OpenAiChat
    }

    fn default_base_url(&self) -> &str {
        Self::DEFAULT_BASE_URL
    }

    fn supported_models(&self) -> Vec<String> {
        self.inner.supported_models()
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
        let base = context.resolve_base_url(Self::DEFAULT_BASE_URL)?;
        let has_v1 = base
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .is_some_and(|segment| segment == "v1");
        context.append_path(
            Self::DEFAULT_BASE_URL,
            if has_v1 {
                "chat/completions"
            } else {
                "v1/chat/completions"
            },
        )
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
        self.inner.setup_headers(headers, credential, context)
    }

    async fn finalize_request(
        &self,
        request: UpstreamRequest,
        _credential: &Credential,
        _context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        let (method, target, headers, body, mode, limit) = request.into_parts();
        // 模型目录 GET 没有正文，也不应用生成参数转换。
        let Some(body) = body else {
            return UpstreamRequest::new(method, target, headers, None)
                .and_then(|request| request.with_response_body_limit(limit))
                .map(|request| request.with_response_mode(mode));
        };
        let mut value: serde_json::Value =
            serde_json::from_slice(&body).map_err(|_| AdaptorError::InvalidTaskRequest)?;
        let object = value
            .as_object_mut()
            .ok_or(AdaptorError::InvalidTaskRequest)?;
        // 公共协议统一使用 max_completion_tokens；NIM 使用 max_tokens。
        // 在所有渠道覆盖完成后转换，保留实际输出上限。
        if let Some(max_tokens) = object.remove("max_completion_tokens") {
            object.insert("max_tokens".to_owned(), max_tokens);
        }
        if !object.contains_key("tools")
            && object
                .get("tool_choice")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|choice| matches!(choice, "auto" | "none"))
        {
            object.remove("tool_choice");
        }
        let body = serde_json::to_vec(&value).map_err(|_| AdaptorError::InvalidTaskRequest)?;
        UpstreamRequest::new(method, target, headers, Some(Bytes::from(body)))
            .and_then(|request| request.with_response_body_limit(limit))
            .map(|request| request.with_response_mode(mode))
    }
}

impl fmt::Debug for NvidiaAdaptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NvidiaAdaptor")
            .field("inner", &self.inner)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpClientConfig, HttpClientPool, Method, ResponseMode};
    use serde_json::json;

    fn context() -> RelayContext {
        RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        )
    }

    #[test]
    fn nvidia_url_authentication_and_protocol_boundaries() {
        let adaptor = NvidiaAdaptor::with_supported_models(vec!["nvidia/model".to_owned()]);
        let target = AdaptorTarget::new("nvidia/model", Operation::Chat, ResponseMode::Stream);
        assert_eq!(
            adaptor.build_url(&context(), target).unwrap(),
            "https://integrate.api.nvidia.com/v1/chat/completions"
        );
        for base in [
            "https://nim.example/proxy",
            "https://nim.example/proxy/v1",
            "https://nim.example/proxy/v1/",
        ] {
            assert_eq!(
                adaptor
                    .build_url(&context().with_base_url(base).unwrap(), target)
                    .unwrap(),
                "https://nim.example/proxy/v1/chat/completions"
            );
        }
        let mut headers = HeaderMap::new();
        adaptor
            .setup_headers(
                &mut headers,
                &Credential::api_key("nvidia-secret-canary").unwrap(),
                &context(),
            )
            .unwrap();
        assert_eq!(headers["authorization"], "Bearer nvidia-secret-canary");
        assert!(!format!("{headers:?}").contains("nvidia-secret-canary"));
        assert!(
            adaptor
                .setup_headers(
                    &mut headers,
                    &Credential::oauth("token").unwrap(),
                    &context()
                )
                .is_err()
        );
        assert!(
            adaptor
                .build_url(
                    &context(),
                    AdaptorTarget::new("nvidia/model", Operation::Responses, ResponseMode::Full)
                )
                .is_err()
        );
    }

    #[tokio::test]
    async fn nvidia_finalization_preserves_budget_tools_and_stream_semantics() {
        let adaptor = NvidiaAdaptor::with_supported_models(Vec::new());
        let credential = Credential::api_key("canary").unwrap();
        for tools in [false, true] {
            let mut value = json!({"model":"meta/llama", "max_completion_tokens":128, "stream":true, "tool_choice":"auto", "chat_template_kwargs":{"thinking":true}});
            if tools {
                value["tools"] = json!([{"type":"function","function":{"name":"lookup","parameters":{"type":"object"}}}]);
            }
            let request = UpstreamRequest::new(
                Method::POST,
                "https://integrate.api.nvidia.com/v1/chat/completions",
                HeaderMap::new(),
                Some(Bytes::from(serde_json::to_vec(&value).unwrap())),
            )
            .unwrap()
            .with_response_mode(ResponseMode::Stream);
            let request = adaptor
                .finalize_request(request, &credential, &context())
                .await
                .unwrap();
            let (_, _, _, body, mode, _) = request.into_parts();
            let actual: serde_json::Value = serde_json::from_slice(&body.unwrap()).unwrap();
            assert_eq!(mode, ResponseMode::Stream);
            assert_eq!(actual["max_tokens"], 128);
            assert!(actual.get("max_completion_tokens").is_none());
            assert_eq!(actual["model"], "meta/llama");
            assert_eq!(actual["stream"], true);
            assert_eq!(
                actual["chat_template_kwargs"],
                value["chat_template_kwargs"]
            );
            assert_eq!(actual.get("tool_choice").is_some(), tools);
            assert_eq!(actual.get("tools"), value.get("tools"));
        }
    }
}
