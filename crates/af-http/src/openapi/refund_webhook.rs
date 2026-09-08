//! 原路退款 Provider webhook 回执的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

#[utoipa::path(
    post,
    path = "/api/refund/webhook/{provider}",
    operation_id = "receiveRefundWebhook",
    tag = "退款回执",
    summary = "接收原路退款 Provider 回执",
    params(
        (
            "provider" = String,
            Path,
            min_length = 1,
            max_length = 32,
            description = "退款 Provider 标识"
        ),
        (
            "stripe-signature" = String,
            Header,
            description = "Stripe 原始签名；服务端会先验签再解析退款正文"
        )
    ),
    request_body(
        description = "必须保留 Provider 原始退款回执正文，服务端在任何解析前完成验签",
        content_type = "application/json"
    ),
    responses(
        (status = 200, description = "退款回执已验签并完成幂等处理"),
        (status = 400, description = "请求正文或签名无效"),
        (status = 405, description = "请求方法不受支持"),
        (status = 409, description = "退款回执与已有退款事实发生冲突"),
        (status = 503, description = "回执 Provider 未启用或持久化暂不可用"),
        (status = 500, description = "服务内部状态异常")
    )
)]
fn receive_refund_webhook() {}

#[derive(OpenApi)]
#[openapi(paths(receive_refund_webhook))]
struct RefundWebhookApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    RefundWebhookApi::openapi()
}
