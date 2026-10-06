use std::{future::Future, pin::Pin};

use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal};
use af_protocol::CanonicalRerankRequest;
use af_relay::RerankResponse;

/// Rerank 业务服务的一次异步调用结果。
pub type RerankServiceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RerankResponse, AfError>> + Send + 'a>>;

/// HTTP 层依赖的对象安全 Rerank 服务端口。
///
/// 端口只接收已经完成协议解析、模型白名单校验和鉴权的不可变请求，不负责 HTTP wire。
pub trait RerankService: Send + Sync {
    /// 转发一次非流式 Rerank 请求并返回公开响应与可选真实用量。
    fn rerank<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalRerankRequest,
        request_id: &'a str,
    ) -> RerankServiceFuture<'a>;
}
