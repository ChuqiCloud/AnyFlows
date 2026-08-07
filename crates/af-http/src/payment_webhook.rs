use std::{sync::Arc, time::SystemTime};

use af_billing::{
    PaymentWebhookHeader, PaymentWebhookRequest, TopupWebhookOutcome, TopupWebhookProcessor,
    TopupWebhookProcessorError,
};
use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};

/// 构建不依赖登录会话的支付 webhook 路由。
///
/// Provider 未配置时仍保留固定路径并返回 503，避免被前端 fallback 误认为成功；
/// 已验签事件交给充值仓储的幂等处理器，路由本身不执行余额变更。
pub fn build_payment_webhook_router(
    provider: impl Into<String>,
    processor: Option<Arc<TopupWebhookProcessor>>,
) -> Router {
    Router::new()
        .route(
            "/api/payment/webhook/{provider}",
            post(receive_payment_webhook),
        )
        .with_state(PaymentWebhookHttpState {
            provider: provider.into(),
            processor,
        })
}

#[derive(Clone)]
struct PaymentWebhookHttpState {
    provider: String,
    processor: Option<Arc<TopupWebhookProcessor>>,
}

/// 接收 Provider 原始 webhook；签名头和正文在任何解析前保持原样。
async fn receive_payment_webhook(
    Path(provider): Path<String>,
    State(state): State<PaymentWebhookHttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    if provider != state.provider {
        return StatusCode::NOT_FOUND;
    }
    let Some(processor) = state.processor else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    let received_at = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };
    let mut webhook_headers = Vec::with_capacity(headers.len());
    for (name, value) in &headers {
        let Ok(value) = value.to_str() else {
            return StatusCode::BAD_REQUEST;
        };
        let Ok(header) = PaymentWebhookHeader::new(name.as_str(), value) else {
            return StatusCode::BAD_REQUEST;
        };
        webhook_headers.push(header);
    }
    let request = match PaymentWebhookRequest::new(&body, &webhook_headers, received_at) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    match processor.process(request).await {
        Ok(
            TopupWebhookOutcome::Applied(_)
            | TopupWebhookOutcome::Acknowledged(_)
            | TopupWebhookOutcome::Existing(_)
            | TopupWebhookOutcome::RecordedUnprocessed { .. }
            | TopupWebhookOutcome::IgnoredNonTerminal
            | TopupWebhookOutcome::NotFound,
        ) => {
            // 已验签但订单不存在或暂未处理的事件也确认接收，避免 Provider 无限重试。
            StatusCode::OK
        }
        Err(error) => map_processor_error(error),
    }
}

fn map_processor_error(error: TopupWebhookProcessorError) -> StatusCode {
    match error {
        TopupWebhookProcessorError::VerificationRejected => StatusCode::BAD_REQUEST,
        TopupWebhookProcessorError::Conflict => StatusCode::CONFLICT,
        TopupWebhookProcessorError::OutcomeUnknown | TopupWebhookProcessorError::Unavailable => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        TopupWebhookProcessorError::Invariant => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
