use std::{future::Future, pin::Pin};

use af_domain::{AfError, AsyncTaskId, AsyncTaskRequestId, GatewayPrincipal, TaskStatus};
use af_protocol::{CanonicalTaskOutput, CanonicalVideoGenerationRequest};

/// 视频任务公开服务的一次异步调用结果。
pub type VideoTaskServiceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<VideoTaskSnapshot, AfError>> + Send + 'a>>;

/// 公开 HTTP 边界可见的 owner-scoped 视频任务快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoTaskSnapshot {
    task_id: AsyncTaskId,
    status: TaskStatus,
    output: Option<CanonicalTaskOutput>,
}

impl VideoTaskSnapshot {
    /// 组合本地公开任务标识、持久化状态和本次查询取得的临时输出。
    #[must_use]
    pub const fn new(
        task_id: AsyncTaskId,
        status: TaskStatus,
        output: Option<CanonicalTaskOutput>,
    ) -> Self {
        Self {
            task_id,
            status,
            output,
        }
    }

    /// 返回只对当前用户公开的稳定本地任务标识。
    #[must_use]
    pub const fn task_id(&self) -> AsyncTaskId {
        self.task_id
    }

    /// 返回闭合任务状态。
    #[must_use]
    pub const fn status(&self) -> &TaskStatus {
        &self.status
    }

    /// 返回本次查询从原上游任务重新取得的临时输出。
    #[must_use]
    pub const fn output(&self) -> Option<&CanonicalTaskOutput> {
        self.output.as_ref()
    }
}

/// HTTP 层依赖的对象安全视频异步任务服务端口。
pub trait VideoTaskService: Send + Sync {
    /// 使用客户端幂等键提交一次付费视频任务。
    fn submit<'a>(
        &'a self,
        principal: GatewayPrincipal,
        request_id: AsyncTaskRequestId,
        request: CanonicalVideoGenerationRequest,
        relay_request_id: &'a str,
    ) -> VideoTaskServiceFuture<'a>;

    /// 只在认证用户范围内查询并按需推进一个既有任务。
    fn poll<'a>(
        &'a self,
        principal: GatewayPrincipal,
        task_id: AsyncTaskId,
        relay_request_id: &'a str,
    ) -> VideoTaskServiceFuture<'a>;
}
