use std::{future::Future, pin::Pin};

use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal};
use af_protocol::CanonicalResponsesCompactionRequest;
use af_relay::ResponsesCompactionResponse;

/// Responses Compact 业务服务的一次异步调用结果。
pub type ResponsesCompactServiceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ResponsesCompactionResponse, AfError>> + Send + 'a>>;

/// HTTP 层依赖的对象安全 Responses Compact 服务端口。
///
/// 端口只接收已完成协议解析、模型白名单校验和鉴权的不可变请求，并负责覆盖完整
/// 专用调度与单次计费生命周期，不允许回退普通 Responses 执行意图。
pub trait ResponsesCompactService: Send + Sync {
    /// 压缩一次完整上下文窗口并返回独立计费用量。
    fn compact<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalResponsesCompactionRequest,
        request_id: &'a str,
    ) -> ResponsesCompactServiceFuture<'a>;
}
