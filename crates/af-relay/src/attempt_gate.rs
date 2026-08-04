use std::{future::Future, pin::Pin};

use af_protocol::Usage;
use thiserror::Error;

use crate::{GenerationCompletionFuture, GenerationCompletionHook, UsageResolutionError};

/// 单候选等待并发许可的对象安全 Future。
pub type RelayAttemptGateFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Box<dyn RelayAttemptPermit>, RelayAttemptGateError>> + Send + 'a,
    >,
>;

/// 候选发送前的异步许可边界；实现不得持有请求正文或明文凭据。
pub trait RelayAttemptGate: Send + Sync + 'static {
    /// 等待并取得当前候选许可，或返回闭合的限流/内部故障分类。
    fn acquire(&self) -> RelayAttemptGateFuture<'_>;
}

/// 已取得的候选许可；显式释放失败由实现记录，不能覆盖业务结果。
pub trait RelayAttemptPermit: Send + 'static {
    /// 异步释放当前许可。
    fn release(self: Box<Self>) -> RelayAttemptReleaseFuture;
}

/// 候选许可释放 Future。
pub type RelayAttemptReleaseFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// 候选并发许可失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RelayAttemptGateError {
    /// 当前候选在自己的等待期限内没有可用槽位，状态机应尝试下一候选。
    #[error("候选并发槽位暂不可用")]
    Limited,
    /// Redis 或内部协调边界失败，不能继续发送上游请求。
    #[error("候选并发协调失败")]
    Internal,
}

/// 把成功候选许可转为生成流收尾回调，使槽位覆盖完整 SSE 生命周期。
pub(crate) fn permit_completion_hook(
    permit: Box<dyn RelayAttemptPermit>,
) -> Box<dyn GenerationCompletionHook> {
    Box::new(RelayAttemptPermitCompletionHook {
        permit: Some(permit),
    })
}

struct RelayAttemptPermitCompletionHook {
    permit: Option<Box<dyn RelayAttemptPermit>>,
}

impl GenerationCompletionHook for RelayAttemptPermitCompletionHook {
    fn on_complete(
        mut self: Box<Self>,
        _usage: Result<Usage, UsageResolutionError>,
    ) -> GenerationCompletionFuture {
        let permit = self.permit.take();
        Box::pin(async move {
            if let Some(permit) = permit {
                permit.release().await;
            }
        })
    }
}
