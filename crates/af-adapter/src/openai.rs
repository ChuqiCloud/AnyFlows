use std::fmt;

use af_domain::{ChannelType, CredentialKind, Operation, Protocol};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};

use crate::credential::clear_authentication_headers;
use crate::{Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, Credential, RelayContext};

/// OpenAI Chat Completions、Responses、Responses Compact、Embeddings、Images 与 Audio 兼容渠道适配器。
///
/// 渠道基础地址由 `RelayContext` 覆盖，可传服务根或反向代理前缀；末尾非空段精确为
/// `v1`（可带尾斜杠）时不会重复追加版本路径，其他路径会追加目标原生端点。
/// 空模型清单表示无内建模型限制，具体模型由渠道配置提供。当前实现 Bearer-compatible
/// OpenAI 原生端点的 URL 与认证 Header 构造，Azure deployment 等专用参数留待后续适配器切片。
#[derive(Clone, Eq, PartialEq)]
pub struct OpenAiAdaptor {
    protocol: Protocol,
    supported_models: Vec<String>,
}

impl Default for OpenAiAdaptor {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiAdaptor {
    /// OpenAI 官方 API 的默认根地址。
    pub const DEFAULT_BASE_URL: &'static str = "https://api.openai.com";
    /// Audio multipart 使用的固定高熵边界；编码器会拒绝正文中的碰撞。
    pub const AUDIO_MULTIPART_BOUNDARY: &'static str =
        "anyflows-audio-6f4d2e1c9b8a7350d4e6f1a2c3b5d798";

    /// 创建不携带内建模型清单的适配器。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            protocol: Protocol::OpenAiChat,
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
            protocol: Protocol::OpenAiChat,
            supported_models: models.into_iter().map(Into::into).collect(),
        }
    }

    /// 创建绑定单一 OpenAI 原生协议的适配器。
    pub fn with_protocol_and_supported_models<I, S>(
        protocol: Protocol,
        models: I,
    ) -> AdaptorResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if !matches!(
            protocol,
            Protocol::OpenAiChat
                | Protocol::OpenAiResponses
                | Protocol::OpenAiEmbeddings
                | Protocol::OpenAiImages
                | Protocol::OpenAiAudio
                | Protocol::OpenAiSpeech
        ) {
            return Err(AdaptorError::InvalidChannelSettings {
                channel_type: ChannelType::OpenAi,
            });
        }
        Ok(Self {
            protocol,
            supported_models: models.into_iter().map(Into::into).collect(),
        })
    }

    fn endpoint_path(
        &self,
        context: &RelayContext,
        operation: Operation,
    ) -> AdaptorResult<&'static str> {
        let endpoint = match (self.protocol, operation) {
            (Protocol::OpenAiChat, Operation::Chat) => "chat/completions",
            (Protocol::OpenAiResponses, Operation::Responses) => "responses",
            (Protocol::OpenAiResponses, Operation::ResponsesCompact) => "responses/compact",
            (Protocol::OpenAiEmbeddings, Operation::Embedding) => "embeddings",
            (Protocol::OpenAiImages, Operation::Image) => "images/generations",
            (Protocol::OpenAiAudio, Operation::Audio) => "audio/transcriptions",
            (Protocol::OpenAiSpeech, Operation::Audio) => "audio/speech",
            (_, operation) => return Err(AdaptorError::UnsupportedOperation { operation }),
        };
        let base_url = context.resolve_base_url(Self::DEFAULT_BASE_URL)?;
        let has_v1_suffix = base_url
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .is_some_and(|segment| segment == "v1");
        Ok(if has_v1_suffix {
            endpoint
        } else {
            match endpoint {
                "chat/completions" => "v1/chat/completions",
                "responses" => "v1/responses",
                "responses/compact" => "v1/responses/compact",
                "embeddings" => "v1/embeddings",
                "images/generations" => "v1/images/generations",
                "audio/transcriptions" => "v1/audio/transcriptions",
                "audio/speech" => "v1/audio/speech",
                _ => unreachable!("OpenAI 端点集合保持闭合"),
            }
        })
    }
}

impl Adaptor for OpenAiAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::OpenAi
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
        context.append_path(
            Self::DEFAULT_BASE_URL,
            self.endpoint_path(context, target.operation())?,
        )
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

        clear_authentication_headers(headers);

        let mut authorization =
            HeaderValue::from_str(&format!("Bearer {}", credential.expose_secret()))
                .map_err(|_| AdaptorError::InvalidHeader)?;
        authorization.set_sensitive(true);
        headers.insert(HeaderName::from_static("authorization"), authorization);
        let content_type = if self.protocol == Protocol::OpenAiAudio {
            HeaderValue::from_str(&format!(
                "multipart/form-data; boundary={}",
                Self::AUDIO_MULTIPART_BOUNDARY
            ))
            .map_err(|_| AdaptorError::InvalidHeader)?
        } else {
            HeaderValue::from_static("application/json")
        };
        headers.insert(HeaderName::from_static("content-type"), content_type);
        let accept = if self.protocol == Protocol::OpenAiSpeech {
            HeaderValue::from_static("application/octet-stream")
        } else {
            HeaderValue::from_static("application/json")
        };
        headers.insert(HeaderName::from_static("accept"), accept);
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
            .field("protocol", &self.protocol)
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
        AdaptorTarget::new("gpt-test", operation, crate::ResponseMode::Full)
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
            adaptor
                .build_url(&context(), target(Operation::Chat))
                .unwrap(),
            "https://api.openai.com/v1/chat/completions"
        );
        for base_url in [
            "https://gateway.example/proxy/openai",
            "https://gateway.example/proxy/openai/v1",
            "https://gateway.example/proxy/openai/v1/",
        ] {
            let target = adaptor
                .build_url(
                    &context().with_base_url(base_url).unwrap(),
                    target(Operation::Chat),
                )
                .unwrap();
            assert!(target.ends_with("/proxy/openai/v1/chat/completions"));
        }
        let uppercase_v1 = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/openai/V1/")
                    .unwrap(),
                target(Operation::Chat),
            )
            .unwrap();
        assert!(uppercase_v1.ends_with("/proxy/openai/V1/v1/chat/completions"));
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
    fn responses_protocol_uses_the_native_endpoint_only() {
        let adaptor = OpenAiAdaptor::with_protocol_and_supported_models(
            Protocol::OpenAiResponses,
            ["gpt-test"],
        )
        .unwrap();
        assert_eq!(adaptor.default_protocol(), Protocol::OpenAiResponses);
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Responses))
                .unwrap(),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::ResponsesCompact))
                .unwrap(),
            "https://api.openai.com/v1/responses/compact"
        );
        let proxied = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/openai/v1")
                    .unwrap(),
                target(Operation::Responses),
            )
            .unwrap();
        assert!(proxied.ends_with("/proxy/openai/v1/responses"));
        let proxied_compact = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/openai/v1")
                    .unwrap(),
                target(Operation::ResponsesCompact),
            )
            .unwrap();
        assert!(proxied_compact.ends_with("/proxy/openai/v1/responses/compact"));
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Chat))
                .unwrap_err(),
            AdaptorError::UnsupportedOperation {
                operation: Operation::Chat
            }
        );
    }

    #[test]
    fn embeddings_protocol_uses_the_native_endpoint_only() {
        let adaptor = OpenAiAdaptor::with_protocol_and_supported_models(
            Protocol::OpenAiEmbeddings,
            ["text-embedding-test"],
        )
        .unwrap();
        assert_eq!(adaptor.default_protocol(), Protocol::OpenAiEmbeddings);
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Embedding))
                .unwrap(),
            "https://api.openai.com/v1/embeddings"
        );
        let proxied = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/openai/v1")
                    .unwrap(),
                target(Operation::Embedding),
            )
            .unwrap();
        assert!(proxied.ends_with("/proxy/openai/v1/embeddings"));
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Chat))
                .unwrap_err(),
            AdaptorError::UnsupportedOperation {
                operation: Operation::Chat
            }
        );
    }

    #[test]
    fn images_protocol_uses_the_native_generation_endpoint_only() {
        let adaptor = OpenAiAdaptor::with_protocol_and_supported_models(
            Protocol::OpenAiImages,
            ["gpt-image-test"],
        )
        .unwrap();
        assert_eq!(adaptor.default_protocol(), Protocol::OpenAiImages);
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Image))
                .unwrap(),
            "https://api.openai.com/v1/images/generations"
        );
        let proxied = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/openai/v1")
                    .unwrap(),
                target(Operation::Image),
            )
            .unwrap();
        assert!(proxied.ends_with("/proxy/openai/v1/images/generations"));
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Chat))
                .unwrap_err(),
            AdaptorError::UnsupportedOperation {
                operation: Operation::Chat
            }
        );
    }

    #[test]
    fn audio_protocol_uses_native_transcription_endpoint_and_multipart_header() {
        let adaptor = OpenAiAdaptor::with_protocol_and_supported_models(
            Protocol::OpenAiAudio,
            ["gpt-audio-test"],
        )
        .unwrap();
        assert_eq!(adaptor.default_protocol(), Protocol::OpenAiAudio);
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Audio))
                .unwrap(),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        let proxied = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/openai/v1")
                    .unwrap(),
                target(Operation::Audio),
            )
            .unwrap();
        assert!(proxied.ends_with("/proxy/openai/v1/audio/transcriptions"));

        let mut headers = HeaderMap::new();
        adaptor
            .setup_headers(
                &mut headers,
                &Credential::api_key("audio-key").unwrap(),
                &context(),
            )
            .unwrap();
        assert_eq!(
            headers["content-type"],
            format!(
                "multipart/form-data; boundary={}",
                OpenAiAdaptor::AUDIO_MULTIPART_BOUNDARY
            )
        );
        assert_eq!(headers["accept"], "application/json");
    }

    #[test]
    fn speech_protocol_uses_native_json_endpoint_and_binary_accept_header() {
        let adaptor = OpenAiAdaptor::with_protocol_and_supported_models(
            Protocol::OpenAiSpeech,
            ["gpt-speech-test"],
        )
        .unwrap();
        assert_eq!(adaptor.default_protocol(), Protocol::OpenAiSpeech);
        assert_eq!(
            adaptor
                .build_url(&context(), target(Operation::Audio))
                .unwrap(),
            "https://api.openai.com/v1/audio/speech"
        );
        let proxied = adaptor
            .build_url(
                &context()
                    .with_base_url("https://gateway.example/proxy/openai/v1")
                    .unwrap(),
                target(Operation::Audio),
            )
            .unwrap();
        assert!(proxied.ends_with("/proxy/openai/v1/audio/speech"));

        let mut headers = HeaderMap::new();
        adaptor
            .setup_headers(
                &mut headers,
                &Credential::api_key("speech-key").unwrap(),
                &context(),
            )
            .unwrap();
        assert_eq!(headers["content-type"], "application/json");
        assert_eq!(headers["accept"], "application/octet-stream");
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
