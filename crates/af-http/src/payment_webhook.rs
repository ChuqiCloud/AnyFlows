use std::sync::Arc;
use std::time::SystemTime;

use af_billing::{
    EASYPAY_PAYMENT_PROVIDER, PaymentWebhookHandler, PaymentWebhookHandlerError,
    PaymentWebhookHandlerOutcome, PaymentWebhookHeader, PaymentWebhookRequest,
};
use af_telemetry::{MetricPaymentConfirmationOutcome, record_payment_confirmation};
use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use http::{HeaderValue, header::CONTENT_TYPE};

/// 构建不依赖登录会话的支付 webhook 路由。
///
/// Stripe 使用原始 POST 正文，易支付同时接受 GET 查询串与 POST 表单；Provider 未配置时
/// 固定返回 503，避免被前端 fallback 误认为成功。
pub trait PaymentWebhookProcessorRegistry: Send + Sync {
    /// 返回当前支付运行时快照中的 Provider 处理器。
    fn processor(&self, provider: &str) -> Option<Arc<dyn PaymentWebhookHandler>>;
}

/// 构建从稳定运行时门面按请求读取处理器的 webhook 路由。
pub fn build_payment_webhook_router(registry: Arc<dyn PaymentWebhookProcessorRegistry>) -> Router {
    Router::new()
        .route(
            "/api/payment/webhook/{provider}",
            get(receive_payment_webhook).post(receive_payment_webhook),
        )
        .with_state(PaymentWebhookHttpState { registry })
}

#[derive(Clone)]
struct PaymentWebhookHttpState {
    registry: Arc<dyn PaymentWebhookProcessorRegistry>,
}

/// 接收 Provider 原始 webhook；签名头、查询串和正文在验签前保持原始字节语义。
async fn receive_payment_webhook(
    Path(provider): Path<String>,
    State(state): State<PaymentWebhookHttpState>,
    uri: http::Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(processor) = state.registry.processor(&provider) else {
        record_payment_confirmation(MetricPaymentConfirmationOutcome::Unavailable);
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let payload = if provider == EASYPAY_PAYMENT_PROVIDER && body.is_empty() {
        match uri.query() {
            Some(query) if !query.is_empty() => Bytes::copy_from_slice(query.as_bytes()),
            _ => {
                record_payment_confirmation(MetricPaymentConfirmationOutcome::InvalidRequest);
                return provider_failure(&provider, StatusCode::BAD_REQUEST);
            }
        }
    } else {
        body
    };
    let received_at = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => {
            record_payment_confirmation(MetricPaymentConfirmationOutcome::Invariant);
            return provider_failure(&provider, StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let mut webhook_headers = Vec::with_capacity(headers.len());
    for (name, value) in &headers {
        let Ok(value) = value.to_str() else {
            record_payment_confirmation(MetricPaymentConfirmationOutcome::InvalidRequest);
            return provider_failure(&provider, StatusCode::BAD_REQUEST);
        };
        let Ok(header) = PaymentWebhookHeader::new(name.as_str(), value) else {
            record_payment_confirmation(MetricPaymentConfirmationOutcome::InvalidRequest);
            return provider_failure(&provider, StatusCode::BAD_REQUEST);
        };
        webhook_headers.push(header);
    }
    let request = match PaymentWebhookRequest::new(&payload, &webhook_headers, received_at) {
        Ok(request) => request,
        Err(_) => {
            record_payment_confirmation(MetricPaymentConfirmationOutcome::InvalidRequest);
            return provider_failure(&provider, StatusCode::BAD_REQUEST);
        }
    };
    match processor.handle(request).await {
        Ok(outcome) => {
            record_payment_confirmation(metric_outcome(outcome));
            match outcome {
                PaymentWebhookHandlerOutcome::Applied
                | PaymentWebhookHandlerOutcome::Acknowledged
                | PaymentWebhookHandlerOutcome::Existing => provider_success(&provider),
                PaymentWebhookHandlerOutcome::IgnoredNonTerminal => {
                    if provider == EASYPAY_PAYMENT_PROVIDER {
                        provider_failure(&provider, StatusCode::CONFLICT)
                    } else {
                        provider_success(&provider)
                    }
                }
                PaymentWebhookHandlerOutcome::RecordedUnprocessed
                | PaymentWebhookHandlerOutcome::NotFound => {
                    provider_failure(&provider, StatusCode::CONFLICT)
                }
            }
        }
        // 无订单或事实不一致不能提前确认，否则 Provider 不再重试会形成漏账。
        Err(error) => {
            record_payment_confirmation(metric_error(error));
            provider_failure(&provider, map_processor_error(error))
        }
    }
}

fn metric_outcome(outcome: PaymentWebhookHandlerOutcome) -> MetricPaymentConfirmationOutcome {
    match outcome {
        PaymentWebhookHandlerOutcome::Applied => MetricPaymentConfirmationOutcome::Applied,
        PaymentWebhookHandlerOutcome::Acknowledged => {
            MetricPaymentConfirmationOutcome::Acknowledged
        }
        PaymentWebhookHandlerOutcome::Existing => MetricPaymentConfirmationOutcome::Existing,
        PaymentWebhookHandlerOutcome::RecordedUnprocessed => {
            MetricPaymentConfirmationOutcome::Rejected
        }
        PaymentWebhookHandlerOutcome::IgnoredNonTerminal => {
            MetricPaymentConfirmationOutcome::IgnoredNonTerminal
        }
        PaymentWebhookHandlerOutcome::NotFound => MetricPaymentConfirmationOutcome::NotFound,
    }
}

fn metric_error(error: PaymentWebhookHandlerError) -> MetricPaymentConfirmationOutcome {
    match error {
        PaymentWebhookHandlerError::VerificationRejected => {
            MetricPaymentConfirmationOutcome::VerificationRejected
        }
        PaymentWebhookHandlerError::Conflict => MetricPaymentConfirmationOutcome::Conflict,
        PaymentWebhookHandlerError::BindingConflict => {
            MetricPaymentConfirmationOutcome::BindingConflict
        }
        PaymentWebhookHandlerError::OutcomeUnknown => {
            MetricPaymentConfirmationOutcome::OutcomeUnknown
        }
        PaymentWebhookHandlerError::Unavailable => MetricPaymentConfirmationOutcome::Unavailable,
        PaymentWebhookHandlerError::Invariant => MetricPaymentConfirmationOutcome::Invariant,
    }
}

fn provider_success(provider: &str) -> Response {
    if provider == EASYPAY_PAYMENT_PROVIDER {
        plain_text(StatusCode::OK, "success")
    } else {
        StatusCode::OK.into_response()
    }
}

fn provider_failure(provider: &str, status: StatusCode) -> Response {
    if provider == EASYPAY_PAYMENT_PROVIDER {
        plain_text(status, "fail")
    } else {
        status.into_response()
    }
}

fn plain_text(status: StatusCode, value: &'static str) -> Response {
    let mut response = (status, value).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn map_processor_error(error: PaymentWebhookHandlerError) -> StatusCode {
    match error {
        PaymentWebhookHandlerError::VerificationRejected => StatusCode::BAD_REQUEST,
        PaymentWebhookHandlerError::Conflict => StatusCode::CONFLICT,
        PaymentWebhookHandlerError::BindingConflict => StatusCode::CONFLICT,
        PaymentWebhookHandlerError::OutcomeUnknown | PaymentWebhookHandlerError::Unavailable => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        PaymentWebhookHandlerError::Invariant => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
