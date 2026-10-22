use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use af_adapter::Bytes;
use af_domain::UpstreamError;
use af_protocol::Usage;
use futures_core::Stream;

use crate::{
    GenerationCompletionFuture, GenerationCompletionHook, GenerationStream, UsageResolutionError,
};

/// 下游响应体的终态，不将客户端取消归咎于上游凭据。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamDeliveryOutcome {
    /// HTTP Body 已被完整消费。
    Completed,
    /// 客户端提前释放响应体。
    ClientClosed,
    /// 上游协议或响应体失败。
    UpstreamFailed,
    /// 下游持续背压超过写入时限。
    WriteTimeout,
    /// 整个响应超过硬期限。
    MaxDuration,
    /// 交付任务异常退出。
    Internal,
}

/// 一次性交付收尾回调；丢弃时由内部生命周期执行取消兜底。
pub trait StreamDeliveryHook: Send + 'static {
    /// 仅在交付终态确定后调用。
    fn on_delivery(self: Box<Self>, outcome: StreamDeliveryOutcome) -> GenerationCompletionFuture;
}

type FinalUsage = Arc<Mutex<Option<Result<Usage, UsageResolutionError>>>>;

/// 将需要等待客户端交付的反馈与上游生成完成解耦。
pub fn with_delivery_completion(
    mut body: Box<dyn GenerationStream>,
    hook: Box<dyn GenerationCompletionHook>,
) -> Box<dyn GenerationStream> {
    let usage = Arc::new(Mutex::new(None));
    let capture = Box::new(CaptureUsage(usage.clone()));
    let hook: Box<dyn StreamDeliveryHook> = Box::new(Completion { hook, usage });
    let hook = match body.take_delivery_hook() {
        Some(previous) => Box::new(ChainedDeliveryHook {
            first: previous,
            second: hook,
        }) as Box<dyn StreamDeliveryHook>,
        None => hook,
    };
    Box::new(DeliveryStream {
        source: body.with_completion_hook(capture),
        hook: Some(hook),
        completion: None,
        terminal_error: None,
        terminated: false,
    })
}

struct CaptureUsage(FinalUsage);

impl GenerationCompletionHook for CaptureUsage {
    fn on_complete(
        self: Box<Self>,
        usage: Result<Usage, UsageResolutionError>,
    ) -> GenerationCompletionFuture {
        *self.0.lock().expect("usage mutex poisoned") = Some(usage);
        Box::pin(async {})
    }
}

struct Completion {
    hook: Box<dyn GenerationCompletionHook>,
    usage: FinalUsage,
}

impl StreamDeliveryHook for Completion {
    fn on_delivery(self: Box<Self>, outcome: StreamDeliveryOutcome) -> GenerationCompletionFuture {
        let usage = self
            .usage
            .lock()
            .expect("usage mutex poisoned")
            .unwrap_or(Err(UsageResolutionError::Interrupted));
        self.hook.on_delivery(usage, outcome)
    }
}

struct ChainedDeliveryHook {
    first: Box<dyn StreamDeliveryHook>,
    second: Box<dyn StreamDeliveryHook>,
}

impl StreamDeliveryHook for ChainedDeliveryHook {
    fn on_delivery(self: Box<Self>, outcome: StreamDeliveryOutcome) -> GenerationCompletionFuture {
        let first = self.first.on_delivery(outcome);
        let second = self.second.on_delivery(outcome);
        Box::pin(async move {
            let ((), ()) = tokio::join!(first, second);
        })
    }
}

struct DeliveryStream {
    source: Box<dyn GenerationStream>,
    hook: Option<Box<dyn StreamDeliveryHook>>,
    completion: Option<GenerationCompletionFuture>,
    terminal_error: Option<UpstreamError>,
    terminated: bool,
}

impl GenerationStream for DeliveryStream {
    fn with_completion_hook(
        mut self: Box<Self>,
        hook: Box<dyn GenerationCompletionHook>,
    ) -> Box<dyn GenerationStream> {
        self.source = self.source.with_completion_hook(hook);
        self
    }

    fn take_delivery_hook(&mut self) -> Option<Box<dyn StreamDeliveryHook>> {
        match (self.source.take_delivery_hook(), self.hook.take()) {
            (Some(first), Some(second)) => Some(Box::new(ChainedDeliveryHook { first, second })),
            (first, second) => first.or(second),
        }
    }
}

impl Stream for DeliveryStream {
    type Item = Result<Bytes, UpstreamError>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.terminated {
            return Poll::Ready(None);
        }
        if self.completion.is_none() {
            let result = match Pin::new(&mut self.source).poll_next(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(bytes))) => return Poll::Ready(Some(Ok(bytes))),
                Poll::Ready(result) => result,
            };
            let Some(hook) = self.hook.take() else {
                self.terminated = true;
                return Poll::Ready(result);
            };
            let outcome = if let Some(Err(error)) = result {
                self.terminal_error = Some(error);
                StreamDeliveryOutcome::UpstreamFailed
            } else {
                StreamDeliveryOutcome::Completed
            };
            self.completion = Some(hook.on_delivery(outcome));
        }
        if self
            .completion
            .as_mut()
            .expect("completion must exist")
            .as_mut()
            .poll(context)
            .is_pending()
        {
            return Poll::Pending;
        }
        self.completion = None;
        self.terminated = true;
        Poll::Ready(self.terminal_error.take().map(Err))
    }
}
