use std::{future::Future, pin::Pin};

use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal};
use af_protocol::CanonicalImageGenerationRequest;
use af_relay::ImageResponse;

/// Images 业务服务的一次异步调用结果。
pub type ImageServiceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ImageResponse, AfError>> + Send + 'a>>;

/// HTTP 层依赖的对象安全 Images 服务端口。
///
/// 端口只接收已完成协议解析、模型白名单校验和鉴权的不可变请求，不负责 HTTP wire。
pub trait ImageService: Send + Sync {
    /// 转发一次非流式图片生成请求，并在内部闭合最终计费。
    fn generate<'a>(
        &'a self,
        principal: &'a GatewayPrincipal,
        user_concurrency: Option<ConcurrencyLimit>,
        request: CanonicalImageGenerationRequest,
        request_id: &'a str,
    ) -> ImageServiceFuture<'a>;
}
