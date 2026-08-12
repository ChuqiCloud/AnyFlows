use std::{future::Future, pin::Pin};

use af_domain::{AfError, AsyncTaskId, AsyncTaskRequestId, GatewayPrincipal, TaskStatus};
use af_protocol::{CanonicalTaskOutput, CanonicalVideoGenerationRequest};

/// 视频任务公开服务的一次异步调用结果。
pub type VideoTaskServiceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<VideoTaskSnapshot, AfError>> + Send + 'a>>;

/// 视频任务历史列表的一次异步调用结果。
pub type VideoTaskListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<VideoTaskPage, AfError>> + Send + 'a>>;

/// 视频任务历史页的稳定倒序位置，不包含任务绑定或凭据信息。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoTaskListCursor {
    task_id: AsyncTaskId,
}

impl VideoTaskListCursor {
    /// 使用上一页末项的公开任务标识构造稳定游标。
    #[must_use]
    pub const fn new(task_id: AsyncTaskId) -> Self {
        Self { task_id }
    }

    /// 返回分页锚点的公开任务标识。
    #[must_use]
    pub const fn task_id(self) -> AsyncTaskId {
        self.task_id
    }
}

/// 已持久化视频任务的公开历史摘要，不包含临时输出地址或调度绑定。
#[derive(Clone, Eq, PartialEq)]
pub struct VideoTaskListItem {
    task_id: AsyncTaskId,
    model: String,
    status: TaskStatus,
    created_at: u64,
    updated_at: u64,
}

impl std::fmt::Debug for VideoTaskListItem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("VideoTaskListItem(<脱敏>)")
    }
}

impl VideoTaskListItem {
    /// 从服务端已经校验的持久化事实构造公开摘要。
    #[must_use]
    pub fn new(
        task_id: AsyncTaskId,
        model: String,
        status: TaskStatus,
        created_at: u64,
        updated_at: u64,
    ) -> Self {
        Self {
            task_id,
            model,
            status,
            created_at,
            updated_at,
        }
    }

    #[must_use]
    pub const fn task_id(&self) -> AsyncTaskId {
        self.task_id
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub const fn status(&self) -> &TaskStatus {
        &self.status
    }

    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }
}

/// 一页 owner-scoped 视频任务历史。
pub struct VideoTaskPage {
    items: Vec<VideoTaskListItem>,
    next_cursor: Option<VideoTaskListCursor>,
}

impl std::fmt::Debug for VideoTaskPage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("VideoTaskPage(<脱敏>)")
    }
}

impl VideoTaskPage {
    /// 组合公开摘要与下一页位置。
    #[must_use]
    pub fn new(items: Vec<VideoTaskListItem>, next_cursor: Option<VideoTaskListCursor>) -> Self {
        Self { items, next_cursor }
    }

    #[must_use]
    pub fn items(&self) -> &[VideoTaskListItem] {
        &self.items
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<VideoTaskListCursor> {
        self.next_cursor
    }
}

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
    /// 只读取当前认证用户已持久化的视频任务摘要，不触发上游轮询。
    fn list<'a>(
        &'a self,
        principal: GatewayPrincipal,
        before: Option<VideoTaskListCursor>,
        limit: usize,
    ) -> VideoTaskListFuture<'a>;

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
