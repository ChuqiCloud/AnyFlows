use std::{future::Future, pin::Pin};

use af_domain::{AfError, ConcurrencyLimit, GatewayPrincipal};
use af_protocol::{CanonicalImageEditRequest, CanonicalImageGenerationRequest};
use af_relay::ImageResponse;

/// Images 服务与调度器共用的请求形态。
#[derive(Clone, Eq, PartialEq)]
pub enum ImageRequest {
    /// JSON 图像生成请求。
    Generation(CanonicalImageGenerationRequest),
    /// multipart 图像编辑请求。
    Edit(CanonicalImageEditRequest),
}

impl ImageRequest {
    /// 返回客户端请求的模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        match self {
            Self::Generation(request) => request.model(),
            Self::Edit(request) => request.model(),
        }
    }

    /// 返回用于日志与路由的规范化 operation。
    #[must_use]
    pub const fn operation(&self) -> af_domain::Operation {
        match self {
            Self::Generation(_) => af_domain::Operation::Image,
            Self::Edit(_) => af_domain::Operation::ImageEdit,
        }
    }

    /// 返回估算输入成本所需的提示词字节数。
    #[must_use]
    pub fn prompt_bytes(&self) -> usize {
        match self {
            Self::Generation(request) => request.prompt_bytes(),
            Self::Edit(request) => request.prompt_bytes(),
        }
    }

    /// 返回已校验的输出选项。
    #[must_use]
    pub const fn options(&self) -> af_protocol::ImageGenerationOptions {
        match self {
            Self::Generation(request) => request.options(),
            Self::Edit(request) => request.options(),
        }
    }
}

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

    /// 转发一次 OpenAI Images multipart 编辑请求，并在内部闭合最终计费。
    fn edit<'a>(
        &'a self,
        _principal: &'a GatewayPrincipal,
        _user_concurrency: Option<ConcurrencyLimit>,
        _request: CanonicalImageEditRequest,
        _request_id: &'a str,
    ) -> ImageServiceFuture<'a> {
        Box::pin(async { Err(AfError::InvalidRequest) })
    }
}
