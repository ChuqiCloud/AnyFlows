use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use af_adapter::{AdaptorResult, Bytes, UpstreamBody, UpstreamBodyStream};
use af_domain::UpstreamError;
use af_protocol::Usage;
use futures_core::Stream;

use crate::UsageResolutionError;

/// 生成流结束回调执行异步收尾的对象安全 Future。
pub type GenerationCompletionFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// 生成流结束或中断时接收最终 usage 的一次性回调。
///
/// 回调与流共同持有计费生命周期；流提前释放时回调也会析构，从而保留 RAII 收尾责任。
pub trait GenerationCompletionHook: Send + 'static {
    /// 接收上游 usage 或闭合的中断分类；实现不得依赖原始响应字节。
    fn on_complete(
        self: Box<Self>,
        usage: Result<Usage, UsageResolutionError>,
    ) -> GenerationCompletionFuture;
}

/// 已完成协议校验与下游重编码的统一生成 SSE 流。
pub trait GenerationStream:
    Stream<Item = Result<Bytes, UpstreamError>> + Send + Unpin + 'static
{
    /// 安装只消费一次的计费或审计完成回调。
    fn with_completion_hook(
        self: Box<Self>,
        hook: Box<dyn GenerationCompletionHook>,
    ) -> Box<dyn GenerationStream>;
}

/// 把完整响应体和异步传输流收敛为同一个轮询边界。
pub(crate) enum StreamSource {
    Full(Option<Bytes>),
    Stream(UpstreamBodyStream),
}

impl From<UpstreamBody> for StreamSource {
    fn from(body: UpstreamBody) -> Self {
        match body {
            UpstreamBody::Full(bytes) => Self::Full(Some(bytes)),
            UpstreamBody::Stream(stream) => Self::Stream(stream),
        }
    }
}

impl StreamSource {
    pub(crate) fn poll_next(
        &mut self,
        context: &mut Context<'_>,
    ) -> Poll<Option<AdaptorResult<Bytes>>> {
        match self {
            Self::Full(bytes) => Poll::Ready(bytes.take().map(Ok)),
            Self::Stream(stream) => Pin::new(stream).poll_next(context),
        }
    }
}
