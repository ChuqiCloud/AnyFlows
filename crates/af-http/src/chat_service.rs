use std::{future::Future, pin::Pin};

use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal, Protocol};
use af_protocol::CanonicalRequestEnvelope;
use af_relay::{ChatResponse, RelayDiagnosticInput, RelayService};

/// Chat 业务服务的一次异步调用结果，保持完整响应与流式 usage 句柄不变。
pub type ChatServiceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ChatResponse, AfError>> + Send + 'a>>;

/// HTTP 层依赖的对象安全 Chat 服务端口。
///
/// 端口只接收已经完成协议归一和鉴权的不可变请求，不负责鉴权、计费或调度。
pub trait ChatService: Send + Sync {
    /// 转发一次 Chat 请求，并保留非流式 usage 或流式 usage 句柄供上层消费。
    fn chat_completions<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &'a str,
        diagnostic: RelayDiagnosticInput,
    ) -> ChatServiceFuture<'a>;
}

impl ChatService for RelayService {
    fn chat_completions<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        _user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRequestEnvelope,
        response_protocol: Protocol,
        request_id: &'a str,
        _diagnostic: RelayDiagnosticInput,
    ) -> ChatServiceFuture<'a> {
        Box::pin(async move {
            RelayService::chat_completions(self, principal, request, response_protocol, request_id)
                .await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use af_domain::{GroupId, Operation, TokenId, UserId};
    use axum::body::Bytes;

    use super::*;

    struct StubChatService;

    impl ChatService for StubChatService {
        fn chat_completions<'a>(
            &'a self,
            _principal: &'a GatewayPrincipal,
            _user_concurrency: Option<ConcurrencyLimit>,
            _request: CanonicalRequestEnvelope,
            _response_protocol: Protocol,
            _request_id: &'a str,
            _diagnostic: RelayDiagnosticInput,
        ) -> ChatServiceFuture<'a> {
            Box::pin(async {
                Ok(ChatResponse::Full {
                    body: Bytes::from_static(b"{}"),
                    usage: Err(af_relay::UsageResolutionError::UnsupportedContent),
                })
            })
        }
    }

    #[tokio::test]
    async fn port_can_be_used_through_an_object_safe_arc() {
        let service: Arc<dyn ChatService> = Arc::new(StubChatService);
        let principal = GatewayPrincipal::new(
            TokenId::new(1).unwrap(),
            UserId::new(2).unwrap(),
            GroupId::new(3).unwrap(),
        );
        let request = af_protocol::CanonicalRequest::new(
            Operation::Chat,
            "test-model".to_owned(),
            vec![],
            false,
        )
        .into();

        let response = service
            .chat_completions(
                &principal,
                None,
                request,
                Protocol::OpenAiChat,
                "request-id",
                RelayDiagnosticInput::capture(
                    "POST",
                    "/v1/chat/completions",
                    &http::HeaderMap::new(),
                    &axum::body::Bytes::from_static(b"{}"),
                ),
            )
            .await
            .unwrap();
        assert!(matches!(response, ChatResponse::Full { .. }));
    }
}
