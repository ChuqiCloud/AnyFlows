use std::{fmt, time::SystemTime};

use af_domain::{ChannelType, CredentialKind, Operation, Protocol};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue, Method};
use async_trait::async_trait;
use aws_credential_types::Credentials as AwsCredentials;
use aws_sigv4::{
    http_request::{SignableBody, SignableRequest, SigningParams, SigningSettings, sign},
    sign::v4,
};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use crate::credential::clear_authentication_headers;
use crate::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, Credential,
    MAX_UPSTREAM_REQUEST_TARGET_BYTES, RelayContext, ResponseMode, UpstreamRequest,
};

/// AWS 官方 `InvokeModelIdentifier` 的最大字节数。
pub const MAX_BEDROCK_MODEL_ID_BYTES: usize = 2_048;
/// AWS 区域配置的本地容量边界。
pub const MAX_AWS_REGION_BYTES: usize = 64;

const AWS_SIGNING_SERVICE: &str = "bedrock";
const AWS_CREDENTIAL_PROVIDER_NAME: &str = "anyflows-bedrock";
const BEDROCK_STREAM_ACCEPT: &str = "application/vnd.amazon.eventstream";
const BEDROCK_JSON: &str = "application/json";
const MODEL_SEGMENT_PLACEHOLDER: &str = "__anyflows_bedrock_model__";

// Bedrock 的 modelId 是非贪婪 URI 标签，ARN 分隔符必须留在单个编码路径段内。
const BEDROCK_MODEL_PATH_ENCODE_SET: &AsciiSet = &CONTROLS.add(b'/').add(b':');

/// Amazon Bedrock Runtime 的 SigV4 适配器。
///
/// 当前实例承载 Anthropic Messages 正文，并负责区域端点、`InvokeModel` 路径和完整
/// HTTP 请求签名。其他模型供应商正文、Converse API 与 EventStream 解包由后续独立
/// 协议/传输切片实现，不能在本适配器中猜测转换。
#[derive(Clone, Eq, PartialEq)]
pub struct BedrockAdaptor {
    region: String,
    default_base_url: String,
    supported_models: Vec<String>,
}

impl BedrockAdaptor {
    /// 使用显式 AWS 区域创建不携带模型清单的适配器。
    pub fn new(region: impl Into<String>) -> AdaptorResult<Self> {
        Self::with_supported_models(region, std::iter::empty::<String>())
    }

    /// 使用显式 AWS 区域和已校验模型清单创建适配器。
    pub fn with_supported_models<I, S>(region: impl Into<String>, models: I) -> AdaptorResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let region = region.into();
        let default_base_url = default_base_url(&region)?;
        Ok(Self {
            region,
            default_base_url,
            supported_models: models.into_iter().map(Into::into).collect(),
        })
    }

    /// 返回签名使用的 AWS 区域。
    #[must_use]
    pub fn region(&self) -> &str {
        &self.region
    }

    fn build_invoke_url(
        &self,
        context: &RelayContext,
        model: &str,
        response_mode: ResponseMode,
    ) -> AdaptorResult<String> {
        if !is_valid_model_id(model) {
            return Err(AdaptorError::InvalidRequestTarget);
        }

        let encoded_model = utf8_percent_encode(model, BEDROCK_MODEL_PATH_ENCODE_SET).to_string();
        let action = match response_mode {
            ResponseMode::Full => "invoke",
            ResponseMode::Stream => "invoke-with-response-stream",
        };
        let mut target = context.resolve_base_url(&self.default_base_url)?;
        {
            let mut segments = target
                .path_segments_mut()
                .map_err(|_| AdaptorError::InvalidBaseUrl)?;
            segments.pop_if_empty();
            segments.push("model");
            segments.push(MODEL_SEGMENT_PLACEHOLDER);
            segments.push(action);
        }
        // URL 库负责前缀与普通段；这里只替换固定占位段，避免 `%` 被二次编码。
        let mut target = String::from(target);
        let placeholder_start = target
            .rfind(MODEL_SEGMENT_PLACEHOLDER)
            .ok_or(AdaptorError::InvalidRequestTarget)?;
        target.replace_range(
            placeholder_start..placeholder_start + MODEL_SEGMENT_PLACEHOLDER.len(),
            &encoded_model,
        );
        if target.len() > MAX_UPSTREAM_REQUEST_TARGET_BYTES {
            return Err(AdaptorError::InvalidRequestTarget);
        }
        Ok(target)
    }

    fn sign_request_at(
        &self,
        request: UpstreamRequest,
        credential: &Credential,
        signing_time: SystemTime,
    ) -> AdaptorResult<UpstreamRequest> {
        let Some((access_key_id, secret_access_key, session_token)) = credential.bedrock_parts()
        else {
            return Err(AdaptorError::UnsupportedCredential {
                kind: credential.kind(),
            });
        };
        let (method, target, mut headers, body, response_mode, response_body_limit) =
            request.into_parts();
        if method != Method::POST {
            return Err(AdaptorError::UnsupportedRequestMethod);
        }

        // 覆盖头已在此之前应用；签名前重建认证载体和 Bedrock 必需内容协商头。
        clear_authentication_headers(&mut headers);
        headers.remove(HeaderName::from_static("host"));
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static(BEDROCK_JSON),
        );
        match response_mode {
            ResponseMode::Full => {
                headers.insert(
                    HeaderName::from_static("accept"),
                    HeaderValue::from_static(BEDROCK_JSON),
                );
                headers.remove(HeaderName::from_static("x-amzn-bedrock-accept"));
            }
            ResponseMode::Stream => {
                headers.insert(
                    HeaderName::from_static("accept"),
                    HeaderValue::from_static(BEDROCK_STREAM_ACCEPT),
                );
                headers.insert(
                    HeaderName::from_static("x-amzn-bedrock-accept"),
                    HeaderValue::from_static(BEDROCK_JSON),
                );
            }
        }

        let aws_credentials = AwsCredentials::new(
            access_key_id.to_owned(),
            secret_access_key.to_owned(),
            session_token.map(str::to_owned),
            None,
            AWS_CREDENTIAL_PROVIDER_NAME,
        );
        let identity = aws_credentials.into();
        let signing_settings = SigningSettings::default();
        let signing_params: SigningParams<'_> = v4::SigningParams::builder()
            .identity(&identity)
            .region(&self.region)
            .name(AWS_SIGNING_SERVICE)
            .time(signing_time)
            .settings(signing_settings)
            .build()
            .map_err(|_| AdaptorError::RequestSigning)?
            .into();

        let instructions = {
            let signable_headers = headers
                .iter()
                .map(|(name, value)| {
                    value
                        .to_str()
                        .map(|value| (name.as_str(), value))
                        .map_err(|_| AdaptorError::InvalidHeader)
                })
                .collect::<AdaptorResult<Vec<_>>>()?;
            let body_bytes = body.as_ref().map_or(&[][..], AsRef::as_ref);
            let signable_request = SignableRequest::new(
                method.as_str(),
                &target,
                signable_headers.into_iter(),
                SignableBody::Bytes(body_bytes),
            )
            .map_err(|_| AdaptorError::RequestSigning)?;
            sign(signable_request, &signing_params)
                .map_err(|_| AdaptorError::RequestSigning)?
                .into_parts()
                .0
        };

        let (signing_headers, signing_query) = instructions.into_parts();
        if !signing_query.is_empty() {
            return Err(AdaptorError::RequestSigning);
        }
        for header in signing_headers {
            let name = HeaderName::from_bytes(header.name().as_bytes())
                .map_err(|_| AdaptorError::InvalidHeader)?;
            let mut value =
                HeaderValue::from_str(header.value()).map_err(|_| AdaptorError::InvalidHeader)?;
            if header.sensitive()
                || matches!(name.as_str(), "authorization" | "x-amz-security-token")
            {
                value.set_sensitive(true);
            }
            headers.insert(name, value);
        }

        UpstreamRequest::new(method, target, headers, body)
            .and_then(|request| request.with_response_body_limit(response_body_limit))
            .map(|request| request.with_response_mode(response_mode))
    }
}

#[async_trait]
impl Adaptor for BedrockAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Bedrock
    }

    fn default_protocol(&self) -> Protocol {
        Protocol::Anthropic
    }

    fn default_base_url(&self) -> &str {
        &self.default_base_url
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
        self.build_invoke_url(context, target.model(), target.response_mode())
    }

    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()> {
        if credential.kind() != CredentialKind::Bedrock {
            return Err(AdaptorError::UnsupportedCredential {
                kind: credential.kind(),
            });
        }
        clear_authentication_headers(headers);
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static(BEDROCK_JSON),
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static(BEDROCK_JSON),
        );
        if let Some(request_id) = context.request_id() {
            let value =
                HeaderValue::from_str(request_id).map_err(|_| AdaptorError::InvalidHeader)?;
            headers.insert(HeaderName::from_static("x-request-id"), value);
        }
        Ok(())
    }

    async fn finalize_request(
        &self,
        request: UpstreamRequest,
        credential: &Credential,
        _context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        self.sign_request_at(request, credential, SystemTime::now())
    }
}

impl fmt::Debug for BedrockAdaptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BedrockAdaptor")
            .field("region", &"<已脱敏>")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}

pub(crate) fn is_valid_aws_region(region: &str) -> bool {
    let isolated_partition = ["us-iso-", "us-isob-", "eu-isoe-", "us-isof-"]
        .iter()
        .any(|prefix| region.starts_with(prefix));
    if region.is_empty() || region.len() > MAX_AWS_REGION_BYTES || isolated_partition {
        return false;
    }
    let mut part_count = 0;
    let valid_parts = region.split('-').all(|part| {
        part_count += 1;
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    });
    valid_parts && part_count >= 3 && region.as_bytes().last().is_some_and(u8::is_ascii_digit)
}

fn default_base_url(region: &str) -> AdaptorResult<String> {
    if !is_valid_aws_region(region) {
        return Err(AdaptorError::InvalidAwsRegion);
    }
    let dns_suffix = if region.starts_with("cn-") {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };
    Ok(format!("https://bedrock-runtime.{region}.{dns_suffix}"))
}

fn is_valid_model_id(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_BEDROCK_MODEL_ID_BYTES
        && model.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use af_httpclient::{Bytes, HttpClientConfig, HttpClientPool};

    use super::*;

    fn context() -> RelayContext {
        RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        )
    }

    fn target(model: &str, response_mode: ResponseMode) -> AdaptorTarget<'_> {
        AdaptorTarget::new(model, Operation::Chat, response_mode)
    }

    fn credential() -> Credential {
        Credential::bedrock(
            "AKIDEXAMPLE00000001",
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            Some("temporary-session-token".to_owned()),
        )
        .unwrap()
    }

    #[test]
    fn metadata_endpoints_models_and_debug_are_stable() {
        let adaptor = BedrockAdaptor::with_supported_models(
            "us-east-1",
            [
                "anthropic.claude-test-v1:0",
                "us.anthropic.claude-test-v1:0",
            ],
        )
        .unwrap();
        assert_eq!(adaptor.channel_type(), ChannelType::Bedrock);
        assert_eq!(adaptor.default_protocol(), Protocol::Anthropic);
        assert_eq!(adaptor.region(), "us-east-1");
        assert_eq!(
            adaptor.default_base_url(),
            "https://bedrock-runtime.us-east-1.amazonaws.com"
        );
        assert_eq!(
            adaptor.supported_models(),
            [
                "anthropic.claude-test-v1:0",
                "us.anthropic.claude-test-v1:0"
            ]
        );

        let debug = format!("{adaptor:?}");
        assert!(debug.contains("supported_model_count: 2"));
        assert!(!debug.contains("us-east-1"));
        assert!(!debug.contains("claude-test"));
    }

    #[test]
    fn region_validation_supports_public_partitions_and_rejects_ambiguous_hosts() {
        assert_eq!(
            BedrockAdaptor::new("cn-north-1")
                .unwrap()
                .default_base_url(),
            "https://bedrock-runtime.cn-north-1.amazonaws.com.cn"
        );
        assert_eq!(
            BedrockAdaptor::new("us-gov-west-1")
                .unwrap()
                .default_base_url(),
            "https://bedrock-runtime.us-gov-west-1.amazonaws.com"
        );
        for invalid in [
            "",
            "US-EAST-1",
            "us_east_1",
            "us-east-1.evil.example",
            "us--east-1",
            "us-iso-east-1",
        ] {
            assert_eq!(
                BedrockAdaptor::new(invalid).unwrap_err(),
                AdaptorError::InvalidAwsRegion
            );
        }
    }

    #[test]
    fn build_url_encodes_model_as_one_official_path_label() {
        let adaptor = BedrockAdaptor::new("us-east-1").unwrap();
        assert_eq!(
            adaptor
                .build_url(
                    &context(),
                    target("us.anthropic.claude-test-v1:0", ResponseMode::Full),
                )
                .unwrap(),
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/us.anthropic.claude-test-v1%3A0/invoke"
        );
        assert_eq!(
            adaptor
                .build_url(
                    &context(),
                    target("us.anthropic.claude-test-v1:0", ResponseMode::Stream),
                )
                .unwrap(),
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/us.anthropic.claude-test-v1%3A0/invoke-with-response-stream"
        );
        assert_eq!(
            adaptor
                .build_url(
                    &context()
                        .with_base_url("https://gateway.example/proxy/bedrock/")
                        .unwrap(),
                    target(
                        "arn:aws:bedrock:us-east-1::foundation-model/anthropic.claude-test-v1:0",
                        ResponseMode::Full,
                    ),
                )
                .unwrap(),
            "https://gateway.example/proxy/bedrock/model/arn%3Aaws%3Abedrock%3Aus-east-1%3A%3Afoundation-model%2Fanthropic.claude-test-v1%3A0/invoke"
        );
    }

    #[test]
    fn build_url_rejects_model_injection_and_unsupported_operations() {
        let adaptor = BedrockAdaptor::new("us-east-1").unwrap();
        for invalid in [
            "",
            "model?token=secret",
            "model#fragment",
            "model%2Finvoke",
            "model\\invoke",
            "model name",
            "模型",
        ] {
            assert_eq!(
                adaptor
                    .build_url(&context(), target(invalid, ResponseMode::Full))
                    .unwrap_err(),
                AdaptorError::InvalidRequestTarget
            );
        }
        assert_eq!(
            adaptor
                .build_url(
                    &context(),
                    AdaptorTarget::new(
                        "anthropic.claude-test-v1:0",
                        Operation::Responses,
                        ResponseMode::Full,
                    ),
                )
                .unwrap_err(),
            AdaptorError::UnsupportedOperation {
                operation: Operation::Responses
            }
        );
    }

    #[test]
    fn setup_headers_requires_bedrock_credentials_and_clears_stale_signatures() {
        let adaptor = BedrockAdaptor::new("us-east-1").unwrap();
        let context = context().with_request_id("request-bedrock-1").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("stale-authorization"),
        );
        headers.insert(
            HeaderName::from_static("x-amz-date"),
            HeaderValue::from_static("stale-date"),
        );
        headers.insert(
            HeaderName::from_static("x-amz-security-token"),
            HeaderValue::from_static("stale-session"),
        );
        adaptor
            .setup_headers(&mut headers, &credential(), &context)
            .unwrap();
        assert!(headers.get("authorization").is_none());
        assert!(headers.get("x-amz-date").is_none());
        assert!(headers.get("x-amz-security-token").is_none());
        assert_eq!(headers["content-type"], BEDROCK_JSON);
        assert_eq!(headers["accept"], BEDROCK_JSON);
        assert_eq!(headers["x-request-id"], "request-bedrock-1");

        assert_eq!(
            adaptor
                .setup_headers(
                    &mut HeaderMap::new(),
                    &Credential::api_key("not-aws").unwrap(),
                    &context,
                )
                .unwrap_err(),
            AdaptorError::UnsupportedCredential {
                kind: CredentialKind::ApiKey
            }
        );
    }

    #[test]
    fn sigv4_is_deterministic_covers_body_and_includes_session_token() {
        let adaptor = BedrockAdaptor::new("us-east-1").unwrap();
        let signing_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let build_request = |body: &'static [u8]| {
            UpstreamRequest::new(
                Method::POST,
                adaptor
                    .build_url(
                        &context(),
                        target("anthropic.claude-test-v1:0", ResponseMode::Full),
                    )
                    .unwrap(),
                HeaderMap::new(),
                Some(Bytes::from_static(body)),
            )
            .unwrap()
        };

        let first = adaptor
            .sign_request_at(
                build_request(br#"{"messages":[]}"#),
                &credential(),
                signing_time,
            )
            .unwrap();
        let second = adaptor
            .sign_request_at(
                build_request(br#"{"messages":[]}"#),
                &credential(),
                signing_time,
            )
            .unwrap();
        let changed = adaptor
            .sign_request_at(
                build_request(br#"{"messages":[{}]}"#),
                &credential(),
                signing_time,
            )
            .unwrap();
        let authorization = first.headers()["authorization"].to_str().unwrap();
        assert_eq!(
            authorization,
            second.headers()["authorization"].to_str().unwrap()
        );
        assert_ne!(
            authorization,
            changed.headers()["authorization"].to_str().unwrap()
        );
        assert!(authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE00000001/"));
        assert!(authorization.contains("/us-east-1/bedrock/aws4_request"));
        assert!(authorization.contains("SignedHeaders="));
        assert_eq!(first.headers()["x-amz-date"], "20231114T221320Z");
        assert_eq!(
            first.headers()["x-amz-security-token"],
            "temporary-session-token"
        );

        let debug = format!("{first:?}\n{:?}", first.headers());
        for secret in [
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "temporary-session-token",
        ] {
            assert!(!debug.contains(secret));
        }
    }

    #[test]
    fn stream_signing_restores_bedrock_content_negotiation_after_overrides() {
        let adaptor = BedrockAdaptor::new("us-east-1").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer malicious-override"),
        );
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("text/plain"),
        );
        let request = UpstreamRequest::new(
            Method::POST,
            adaptor
                .build_url(
                    &context(),
                    target("anthropic.claude-test-v1:0", ResponseMode::Stream),
                )
                .unwrap(),
            headers,
            Some(Bytes::from_static(br#"{"messages":[]}"#)),
        )
        .unwrap()
        .with_response_mode(ResponseMode::Stream);
        let signed = adaptor
            .sign_request_at(request, &credential(), SystemTime::UNIX_EPOCH)
            .unwrap();

        assert_eq!(signed.headers()["content-type"], BEDROCK_JSON);
        assert_eq!(signed.headers()["accept"], BEDROCK_STREAM_ACCEPT);
        assert_eq!(signed.headers()["x-amzn-bedrock-accept"], BEDROCK_JSON);
        assert!(signed.headers().get("host").is_none());
        assert!(
            signed.headers()["authorization"]
                .to_str()
                .unwrap()
                .starts_with("AWS4-HMAC-SHA256 ")
        );
        assert!(!format!("{:?}", signed.headers()).contains("malicious-override"));
    }
}
