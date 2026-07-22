use std::{fmt, sync::Arc};

use af_adapter::{
    Adaptor, AdaptorSendExt, AdaptorSettings, Bytes, ChannelType, Credential, HeaderMap, Method,
    OpenAiAdaptorSettings, Operation, PooledClient, RelayContext, StatusCode, UpstreamRequest,
    get_adaptor,
};
use af_domain::{AfError, GatewayPrincipal, MAX_MODEL_NAME_BYTES, UpstreamError};
use af_protocol::{
    CanonicalRequest,
    openai_chat::{build_request, parse_response},
};

use crate::{
    RelayBuildError,
    error::{classify_upstream_status, map_adaptor_error},
};

/// M0 单 OpenAI-compatible 上游的非流式转发服务。
///
/// 本类型刻意不包含鉴权、计费、调度、重试或数据库渠道读取；这些能力将在 M1
/// 替换静态单上游装配，但 HTTP 层仍只依赖本 crate 的转发入口。
#[derive(Clone)]
pub struct RelayService {
    adaptor: Arc<dyn Adaptor>,
    context: RelayContext,
    credential: Credential,
    model: String,
}

impl RelayService {
    /// 使用已创建的受控 Client 和启动期静态上游配置构造 M0 转发服务。
    pub fn new(
        client: PooledClient,
        base_url: &str,
        model: &str,
        api_key: &str,
    ) -> Result<Self, RelayBuildError> {
        if !is_valid_model(model) {
            return Err(RelayBuildError::InvalidModel);
        }
        let context = RelayContext::new(client).with_base_url(base_url)?;
        let credential = Credential::api_key(api_key)?;
        let adaptor = get_adaptor(
            ChannelType::OpenAi,
            AdaptorSettings::OpenAi(OpenAiAdaptorSettings::new(vec![model.to_owned()])),
        )?;
        Ok(Self {
            adaptor,
            context,
            credential,
            model: model.to_owned(),
        })
    }

    /// 转发一次已认证且完成入站规范化的非流式请求，并返回经过协议校验的同协议响应。
    ///
    /// 身份参数用于强制调用方保留请求归属，当前静态单上游阶段不会把它写入
    /// `RelayContext`、适配器或上游请求。
    pub async fn chat_completions(
        &self,
        _principal: &GatewayPrincipal,
        canonical: CanonicalRequest,
        request_id: &str,
    ) -> Result<OpenAiChatResponse, AfError> {
        if canonical.model != self.model {
            return Err(UpstreamError::ModelUnsupported.into());
        }
        let outbound_value = build_request(&canonical).map_err(|_| AfError::InvalidRequest)?;
        let outbound_body = serde_json::to_vec(&outbound_value)
            .map(Bytes::from)
            .map_err(|_| AfError::Internal)?;

        let context = self
            .context
            .clone()
            .with_request_id(request_id.to_owned())
            .map_err(|_| AfError::Internal)?;
        let mut headers = HeaderMap::new();
        self.adaptor
            .setup_headers(&mut headers, &self.credential, &context)
            .map_err(|_| AfError::Internal)?;
        let target = self
            .adaptor
            .build_url(&context, Operation::Chat)
            .map_err(|_| AfError::Internal)?;
        let request = UpstreamRequest::new(Method::POST, target, headers, Some(outbound_body))
            .map_err(|_| AfError::Internal)?;
        let response = self
            .adaptor
            .send(request, &context)
            .await
            .map_err(|error| AfError::from(map_adaptor_error(error)))?;
        let status = response.status();
        let body = response
            .into_body()
            .into_bytes()
            .await
            .map_err(|error| AfError::from(map_adaptor_error(error)))?;

        if status != StatusCode::OK {
            return Err(classify_upstream_status(status, &body).into());
        }
        parse_response(&body).map_err(|_| UpstreamError::ProtocolError)?;
        Ok(OpenAiChatResponse { body })
    }
}

impl fmt::Debug for RelayService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayService")
            .field("adaptor", &"<OpenAI-compatible>")
            .field("context", &self.context)
            .field("credential", &self.credential)
            .field("model", &"<已脱敏>")
            .finish()
    }
}

/// 已完成 OpenAI Chat 协议校验的非流式同协议响应。
///
/// 当前协议层尚无成功响应 encoder，因此只允许 OpenAI Chat → OpenAI Chat 在完整
/// 解析成功后保留原始字节；未来跨协议响应不得复用本类型。
pub struct OpenAiChatResponse {
    body: Bytes,
}

impl OpenAiChatResponse {
    /// 消费响应并返回可写入 HTTP body 的已验证 JSON 字节。
    #[must_use]
    pub fn into_body(self) -> Bytes {
        self.body
    }
}

impl fmt::Debug for OpenAiChatResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiChatResponse")
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

fn is_valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use af_domain::UpstreamServerStatus;

    use super::*;

    #[test]
    fn model_validation_matches_openai_protocol_boundary() {
        assert!(is_valid_model("gpt-test"));
        for model in ["", " gpt-test", "gpt-test ", "gpt\ntest"] {
            assert!(!is_valid_model(model));
        }
        assert!(!is_valid_model(&"x".repeat(MAX_MODEL_NAME_BYTES + 1)));
    }

    #[test]
    fn status_and_structured_signals_map_to_closed_domain_errors() {
        let cases = [
            (
                StatusCode::BAD_REQUEST,
                b"{}".as_slice(),
                UpstreamError::BadRequest,
            ),
            (
                StatusCode::UNAUTHORIZED,
                b"{}".as_slice(),
                UpstreamError::AuthExpired,
            ),
            (
                StatusCode::UNAUTHORIZED,
                br#"{"error":{"code":"invalid_api_key"}}"#.as_slice(),
                UpstreamError::AuthRevoked,
            ),
            (
                StatusCode::FORBIDDEN,
                b"{}".as_slice(),
                UpstreamError::ProtocolError,
            ),
            (
                StatusCode::PAYMENT_REQUIRED,
                b"{}".as_slice(),
                UpstreamError::QuotaExhausted,
            ),
            (
                StatusCode::NOT_FOUND,
                br#"{"error":{"code":"model_not_found"}}"#.as_slice(),
                UpstreamError::ModelUnsupported,
            ),
            (
                StatusCode::NOT_FOUND,
                b"{}".as_slice(),
                UpstreamError::ProtocolError,
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                br#"{"error":{"code":"rate_limit_exceeded"}}"#.as_slice(),
                UpstreamError::RateLimited,
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                br#"{"error":{"type":"insufficient_quota"}}"#.as_slice(),
                UpstreamError::QuotaExhausted,
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                b"{}".as_slice(),
                UpstreamError::Overloaded,
            ),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                b"{}".as_slice(),
                UpstreamError::ServerError {
                    status: UpstreamServerStatus::new(503).unwrap(),
                },
            ),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                br#"{"error":{"code":"model_not_found"}}"#.as_slice(),
                UpstreamError::ServerError {
                    status: UpstreamServerStatus::new(503).unwrap(),
                },
            ),
            (
                StatusCode::NO_CONTENT,
                b"{}".as_slice(),
                UpstreamError::ProtocolError,
            ),
        ];
        for (status, body, expected) in cases {
            assert_eq!(classify_upstream_status(status, body), expected);
        }
    }

    #[test]
    fn verified_response_debug_never_contains_body() {
        let response = OpenAiChatResponse {
            body: Bytes::from_static(b"response-body-secret"),
        };
        let debug = format!("{response:?}");
        assert!(debug.contains("body_bytes"));
        assert!(!debug.contains("response-body-secret"));
    }

    #[test]
    fn response_boundary_errors_map_without_payloads() {
        assert_eq!(
            AfError::from(map_adaptor_error(
                af_adapter::AdaptorError::InvalidResponseHeader
            )),
            AfError::from(UpstreamError::ProtocolError)
        );
        assert_eq!(
            AfError::from(map_adaptor_error(
                af_adapter::AdaptorError::ResponseBodyTooLarge
            )),
            AfError::from(UpstreamError::ProtocolError)
        );
    }
}
