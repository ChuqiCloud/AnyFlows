use std::sync::Arc;
use std::time::SystemTime;

use af_billing::{
    PaymentWebhookHeader, PaymentWebhookRequest, RefundReceiptHandler, RefundReceiptHandlerOutcome,
    RefundReceiptProcessorError,
};
use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};

/// 退款回执处理器注册表；每次请求读取同一份运行时快照中的 Provider。
pub trait RefundReceiptProcessorRegistry: Send + Sync {
    fn processor(&self, provider: &str) -> Option<Arc<dyn RefundReceiptHandler>>;
}

/// 构建不依赖登录会话的退款回执路由。
pub fn build_refund_receipt_webhook_router(
    registry: Arc<dyn RefundReceiptProcessorRegistry>,
) -> Router {
    Router::new()
        .route(
            "/api/refund/webhook/{provider}",
            post(receive_refund_webhook),
        )
        .with_state(RefundReceiptWebhookHttpState { registry })
}

#[derive(Clone)]
struct RefundReceiptWebhookHttpState {
    registry: Arc<dyn RefundReceiptProcessorRegistry>,
}

/// 接收 Provider 原始退款回执；验签前不修改正文和签名头语义。
async fn receive_refund_webhook(
    Path(provider): Path<String>,
    State(state): State<RefundReceiptWebhookHttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(processor) = state.registry.processor(&provider) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let received_at = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut webhook_headers = Vec::with_capacity(headers.len());
    for (name, value) in &headers {
        let Ok(value) = value.to_str() else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let Ok(header) = PaymentWebhookHeader::new(name.as_str(), value) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        webhook_headers.push(header);
    }
    let request = match PaymentWebhookRequest::new(&body, &webhook_headers, received_at) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    match processor.handle(request).await {
        Ok(RefundReceiptHandlerOutcome::Applied | RefundReceiptHandlerOutcome::Existing) => {
            StatusCode::OK.into_response()
        }
        Ok(RefundReceiptHandlerOutcome::NotFound) => StatusCode::CONFLICT.into_response(),
        Err(error) => refund_receipt_failure(error),
    }
}

fn refund_receipt_failure(error: RefundReceiptProcessorError) -> Response {
    let status = match error {
        RefundReceiptProcessorError::VerificationRejected => StatusCode::BAD_REQUEST,
        RefundReceiptProcessorError::Conflict => StatusCode::CONFLICT,
        RefundReceiptProcessorError::OutcomeUnknown | RefundReceiptProcessorError::Unavailable => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        RefundReceiptProcessorError::Invariant => StatusCode::INTERNAL_SERVER_ERROR,
    };
    status.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_billing::RefundReceiptHandlerFuture;
    use axum::body::Body;
    use http::Request;
    use tower::ServiceExt;

    struct FixedRegistry {
        processor: Option<Arc<dyn RefundReceiptHandler>>,
    }

    impl RefundReceiptProcessorRegistry for FixedRegistry {
        fn processor(&self, _provider: &str) -> Option<Arc<dyn RefundReceiptHandler>> {
            self.processor.clone()
        }
    }

    struct FixedHandler {
        outcome: Result<RefundReceiptHandlerOutcome, RefundReceiptProcessorError>,
    }

    impl RefundReceiptHandler for FixedHandler {
        fn handle<'a>(
            &'a self,
            _request: PaymentWebhookRequest<'a>,
        ) -> RefundReceiptHandlerFuture<'a> {
            let outcome = self.outcome;
            Box::pin(async move { outcome })
        }
    }

    #[tokio::test]
    async fn route_preserves_closed_provider_responses() {
        let router = build_refund_receipt_webhook_router(Arc::new(FixedRegistry {
            processor: Some(Arc::new(FixedHandler {
                outcome: Ok(RefundReceiptHandlerOutcome::Applied),
            })),
        }));
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/refund/webhook/stripe")
                    .header("stripe-signature", "opaque")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let unknown =
            build_refund_receipt_webhook_router(Arc::new(FixedRegistry { processor: None }))
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/refund/webhook/stripe")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
        assert_eq!(unknown.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
