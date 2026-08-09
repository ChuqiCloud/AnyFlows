use std::{future::Future, pin::Pin};

use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal};
use af_protocol::CanonicalEmbeddingRequest;
use af_relay::EmbeddingResponse;

/// Embeddings 业务服务的一次异步调用结果。
pub type EmbeddingServiceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<EmbeddingResponse, AfError>> + Send + 'a>>;

/// HTTP 层依赖的对象安全 Embeddings 服务端口。
///
/// 端口只接收已经完成协议解析、模型白名单校验和鉴权的不可变请求，不负责 HTTP wire。
pub trait EmbeddingService: Send + Sync {
    /// 转发一次非流式 Embeddings 请求并返回最终计费用量。
    fn embeddings<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalEmbeddingRequest,
        request_id: &'a str,
    ) -> EmbeddingServiceFuture<'a>;
}
