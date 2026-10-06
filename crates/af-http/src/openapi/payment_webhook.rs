//! 支付 Provider webhook 回调的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

#[utoipa::path(
    post,
    path = "/api/payment/webhook/{provider}",
    operation_id = "receivePaymentWebhook",
    tag = "支付回调",
    summary = "接收支付 Provider webhook",
    params(
        (
            "provider" = String,
            Path,
            min_length = 1,
            max_length = 32,
            description = "支付 Provider 标识"
        ),
        (
            "stripe-signature" = String,
            Header,
            description = "Stripe 原始签名；服务端会先验签再解析正文"
        )
    ),
    request_body(
        description = "必须保留 Provider 原始请求正文，服务端在任何解析前完成验签",
        content_type = "application/json"
    ),
    responses(
        (status = 200, description = "事件已验签并完成幂等确认"),
        (status = 400, description = "请求正文或签名无效"),
        (status = 405, description = "请求方法不受支持"),
        (status = 409, description = "支付事件与已有记录发生冲突"),
        (status = 503, description = "回调未启用或持久化暂不可用"),
        (status = 500, description = "服务内部状态异常")
    )
)]
fn receive_payment_webhook() {}

#[derive(OpenApi)]
#[openapi(paths(receive_payment_webhook))]
struct PaymentWebhookApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PaymentWebhookApi::openapi()
}
