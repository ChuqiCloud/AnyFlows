use std::{fmt, fmt::Write as _};

use af_domain::{ChannelType, CredentialKind, OpenCodeEntry, Operation, Protocol};
use af_httpclient::{HeaderMap, HeaderValue};
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, Credential, RelayContext, UpstreamRequest,
    credential::clear_authentication_headers,
};

/// OpenCode 服务适配；原生协议与客户端身份分别管理。
#[derive(Clone, Eq, PartialEq)]
pub struct OpenCodeAdaptor {
    entry: OpenCodeEntry,
    protocol: Protocol,
    models: Vec<String>,
}

impl OpenCodeAdaptor {
    pub fn new(
        entry: OpenCodeEntry,
        protocol: Protocol,
        models: Vec<String>,
    ) -> AdaptorResult<Self> {
        if !matches!(
            protocol,
            Protocol::OpenAiChat | Protocol::OpenAiResponses | Protocol::Anthropic
        ) {
            return Err(AdaptorError::InvalidChannelSettings {
                channel_type: ChannelType::OpenCode,
            });
        }
        Ok(Self {
            entry,
            protocol,
            models,
        })
    }

    pub fn models_url(&self) -> String {
        format!("{}/v1/models", self.entry.root())
    }

    fn validate_context(&self, context: &RelayContext) -> AdaptorResult<()> {
        if context.base_url_override().is_some() || context.oauth_provider().is_some() {
            return Err(AdaptorError::InvalidChannelSettings {
                channel_type: ChannelType::OpenCode,
            });
        }
        Ok(())
    }
}

impl fmt::Debug for OpenCodeAdaptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenCodeAdaptor")
            .field("entry", &self.entry)
            .field("protocol", &self.protocol)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl Adaptor for OpenCodeAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::OpenCode
    }
    fn default_protocol(&self) -> Protocol {
        self.protocol
    }
    fn default_base_url(&self) -> &str {
        self.entry.root()
    }
    fn supported_models(&self) -> Vec<String> {
        self.models.clone()
    }

    fn build_url(
        &self,
        context: &RelayContext,
        target: AdaptorTarget<'_>,
    ) -> AdaptorResult<String> {
        self.validate_context(context)?;
        let path = match (self.protocol, target.operation()) {
            (Protocol::OpenAiChat, Operation::Chat) => "openai/v1/chat/completions",
            (Protocol::OpenAiResponses, Operation::Responses) => "openai/v1/responses",
            (Protocol::Anthropic, Operation::Chat) => "anthropic/v1/messages",
            (_, operation) => return Err(AdaptorError::UnsupportedOperation { operation }),
        };
        let path = if self.entry == OpenCodeEntry::ZenV1 {
            path.split_once('/').map_or(path, |(_, path)| path)
        } else {
            path
        };
        Ok(format!("{}/{path}", self.entry.root()))
    }

    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()> {
        self.validate_context(context)?;
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
        headers.insert("authorization", authorization);
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert("accept", HeaderValue::from_static("text/event-stream"));
        headers.insert(
            "user-agent",
            HeaderValue::from_static("opencode/latest/2.0.29/cli"),
        );
        headers.insert("x-opencode-client", HeaderValue::from_static("cli"));
        if self.protocol == Protocol::Anthropic {
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        }
        // 已隔离的主体/渠道/修订摘要再绑定凭据，避免私有上游状态跨账号续接。
        let scope = context
            .client_session_scope()
            .or_else(|| context.request_id());
        if let Some(scope) = scope {
            let mut digest = Sha256::new();
            for part in [
                b"AnyFlows:opencode-session:v1".as_slice(),
                scope.as_bytes(),
                credential.expose_secret().as_bytes(),
            ] {
                digest.update(u64::try_from(part.len()).unwrap_or(u64::MAX).to_be_bytes());
                digest.update(part);
            }
            let hash = digest.finalize();
            let mut encoded = String::with_capacity(64);
            for byte in hash {
                write!(&mut encoded, "{byte:02x}").expect("写入 String 不会失败");
            }
            let mut session = HeaderValue::from_str(&format!("ses_{encoded}"))
                .map_err(|_| AdaptorError::InvalidHeader)?;
            session.set_sensitive(true);
            for name in [
                "x-opencode-session",
                "x-opencode-session-id",
                "x-session-affinity",
                "x-session-id",
            ] {
                headers.insert(name, session.clone());
            }
            let mut project =
                HeaderValue::from_str(&encoded[..40]).map_err(|_| AdaptorError::InvalidHeader)?;
            project.set_sensitive(true);
            headers.insert("x-opencode-project", project);
        }
        Ok(())
    }

    async fn finalize_request(
        &self,
        request: UpstreamRequest,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        // GET 模型目录与推理共用认证。最终阶段重新写入服务托管字段，防止渠道覆盖。
        let (method, target, mut headers, mut body, mut mode, limit) = request.into_parts();
        let identity =
            ["user-agent", "x-opencode-client"].map(|name| (name, headers.get(name).cloned()));
        self.setup_headers(&mut headers, credential, context)?;
        for (name, value) in identity {
            if let Some(value) = value {
                headers.insert(name, value);
            }
        }
        if method == crate::Method::POST {
            let bytes = body.take().ok_or(AdaptorError::InvalidTaskRequest)?;
            let mut value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| AdaptorError::InvalidTaskRequest)?;
            let object = value
                .as_object_mut()
                .ok_or(AdaptorError::InvalidTaskRequest)?;
            // 私有 Responses 状态只能在持久凭据绑定后开放，当前要求完整历史。
            if object.contains_key("previous_response_id") || object.contains_key("conversation") {
                return Err(AdaptorError::InvalidTaskRequest);
            }
            object.insert("stream".to_owned(), serde_json::Value::Bool(true));
            if self.protocol == Protocol::OpenAiChat {
                let options = object
                    .entry("stream_options")
                    .or_insert_with(|| serde_json::json!({}))
                    .as_object_mut()
                    .ok_or(AdaptorError::InvalidTaskRequest)?;
                options.insert("include_usage".to_owned(), serde_json::Value::Bool(true));
            }
            body = Some(crate::Bytes::from(
                serde_json::to_vec(&value).map_err(|_| AdaptorError::InvalidTaskRequest)?,
            ));
            mode = crate::ResponseMode::Stream;
        }
        UpstreamRequest::new(method, target, headers, body)?
            .with_response_body_limit(limit)
            .map(|r| r.with_response_mode(mode))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpClientConfig, HttpClientPool, ResponseMode};

    #[test]
    fn fixed_endpoints_reject_address_and_oauth_overrides() {
        let context = RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        );
        for (protocol, operation, suffix) in [
            (
                Protocol::OpenAiChat,
                Operation::Chat,
                "openai/v1/chat/completions",
            ),
            (
                Protocol::OpenAiResponses,
                Operation::Responses,
                "openai/v1/responses",
            ),
            (
                Protocol::Anthropic,
                Operation::Chat,
                "anthropic/v1/messages",
            ),
        ] {
            for entry in [OpenCodeEntry::InferenceV1, OpenCodeEntry::ZenV1] {
                let adaptor = OpenCodeAdaptor::new(entry, protocol, vec![]).unwrap();
                let target = AdaptorTarget::new("model", operation, ResponseMode::Stream);
                assert_eq!(
                    adaptor.build_url(&context, target).unwrap(),
                    format!(
                        "{}/{}",
                        entry.root(),
                        if entry == OpenCodeEntry::ZenV1 {
                            suffix.split_once('/').unwrap().1
                        } else {
                            suffix
                        }
                    )
                );
                assert!(
                    adaptor
                        .build_url(
                            &context
                                .clone()
                                .with_base_url("https://example.com")
                                .unwrap(),
                            target
                        )
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn session_headers_are_stable_private_and_credential_isolated() {
        let context = RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        )
        .with_request_id("scope-a")
        .unwrap();
        let adaptor =
            OpenCodeAdaptor::new(OpenCodeEntry::InferenceV1, Protocol::OpenAiChat, vec![]).unwrap();
        let headers = |key| {
            let mut h = HeaderMap::new();
            adaptor
                .setup_headers(&mut h, &Credential::api_key(key).unwrap(), &context)
                .unwrap();
            h
        };
        let first = headers("test-a");
        assert_eq!(first["x-session-id"], headers("test-a")["x-session-id"]);
        assert_ne!(first["x-session-id"], headers("test-b")["x-session-id"]);
        assert_eq!(first["x-session-id"], first["x-opencode-session"]);
        assert!(first["authorization"].is_sensitive());
        assert!(first["x-session-id"].is_sensitive());
    }
}

#[cfg(test)]
mod request_tests {
    use super::*;
    use crate::{Bytes, HttpClientConfig, HttpClientPool, Method, ResponseMode};
    #[tokio::test]
    async fn nonstream_client_uses_upstream_sse_without_mutating_tools_or_history() {
        let adaptor =
            OpenCodeAdaptor::new(OpenCodeEntry::InferenceV1, Protocol::OpenAiChat, vec![]).unwrap();
        let context = RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        )
        .with_request_id("fixture")
        .unwrap();
        let input = serde_json::json!({"model":"fixture", "stream":false, "messages":[{"role":"user","content":"first"},{"role":"assistant","content":"answer"},{"role":"user","content":"second"}],"tools":[{"type":"function","function":{"name":"read","parameters":{"type":"object","properties":{"custom":{"type":"integer"}}}}}]});
        let request = UpstreamRequest::new(
            Method::POST,
            "https://opencode.ai/inference/openai/v1/chat/completions",
            HeaderMap::new(),
            Some(Bytes::from(serde_json::to_vec(&input).unwrap())),
        )
        .unwrap();
        let request = adaptor
            .finalize_request(
                request,
                &Credential::api_key("fixture-secret").unwrap(),
                &context,
            )
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(request.body().unwrap()).unwrap();
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["messages"], input["messages"]);
        assert_eq!(body["tools"], input["tools"]);
        assert_eq!(request.response_mode(), ResponseMode::Stream);
    }
}
