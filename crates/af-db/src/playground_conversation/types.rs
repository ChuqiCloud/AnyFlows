use std::fmt;

use sea_orm::entity::prelude::Json;
use thiserror::Error;

/// 单个用户最多保留的 Playground 私有会话数量。
pub const MAX_PLAYGROUND_CONVERSATIONS_PER_USER: usize = 50;

/// 已由应用层构造、等待幂等创建或版本更新的会话快照。
pub struct PlaygroundConversationWrite {
    pub(super) conversation_id: String,
    pub(super) title: String,
    pub(super) models: Vec<String>,
    pub(super) snapshot: Json,
    pub(super) expected_revision: Option<i64>,
}

impl PlaygroundConversationWrite {
    /// 组装随机标识、服务端派生摘要、快照和可选预期版本。
    #[must_use]
    pub fn new(
        conversation_id: String,
        title: String,
        models: Vec<String>,
        snapshot: Json,
        expected_revision: Option<i64>,
    ) -> Self {
        Self {
            conversation_id,
            title,
            models,
            snapshot,
            expected_revision,
        }
    }
}

impl fmt::Debug for PlaygroundConversationWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundConversationWrite")
            .field("expected_revision", &self.expected_revision)
            .finish_non_exhaustive()
    }
}

/// 当前用户可读取的一份完整私有会话。
#[derive(PartialEq)]
pub struct PlaygroundConversationRecord {
    pub(super) conversation_id: String,
    pub(super) title: String,
    pub(super) models: Vec<String>,
    pub(super) snapshot: Json,
    pub(super) revision: i64,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
}

impl PlaygroundConversationRecord {
    /// 消费完整记录并返回所有者响应所需字段。
    #[must_use]
    pub fn into_parts(self) -> (String, String, Vec<String>, Json, i64, i64, i64) {
        (
            self.conversation_id,
            self.title,
            self.models,
            self.snapshot,
            self.revision,
            self.created_at,
            self.updated_at,
        )
    }

    /// 返回当前乐观并发版本。
    #[must_use]
    pub const fn revision(&self) -> i64 {
        self.revision
    }
}

impl fmt::Debug for PlaygroundConversationRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundConversationRecord")
            .field("revision", &self.revision)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

/// 历史抽屉使用的非正文摘要。
pub struct PlaygroundConversationSummaryRecord {
    pub(super) conversation_id: String,
    pub(super) title: String,
    pub(super) models: Vec<String>,
    pub(super) revision: i64,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
}

impl PlaygroundConversationSummaryRecord {
    /// 消费摘要并返回列表响应字段。
    #[must_use]
    pub fn into_parts(self) -> (String, String, Vec<String>, i64, i64, i64) {
        (
            self.conversation_id,
            self.title,
            self.models,
            self.revision,
            self.created_at,
            self.updated_at,
        )
    }
}

impl fmt::Debug for PlaygroundConversationSummaryRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundConversationSummaryRecord")
            .field("model_count", &self.models.len())
            .field("revision", &self.revision)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

/// 所有者删除会话后的闭合结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaygroundConversationDeleteOutcome {
    /// 本次请求删除了一条当前用户的会话。
    Deleted,
    /// 标识未知、格式无效或不属于当前用户。
    NotFound,
}

/// 会话历史仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlaygroundConversationRepositoryConfigError {
    /// 零超时无法形成有效的数据库操作截止时间。
    #[error("Playground 会话历史仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 会话历史仓储错误；不携带标识、标题、模型或正文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlaygroundConversationRepositoryError {
    /// 标识、摘要、快照或版本不满足持久化边界。
    #[error("Playground 会话历史写入参数无效")]
    InvalidInput,
    /// 当前会话用户在写入前已经失效。
    #[error("Playground 会话历史所有者不可用")]
    OwnerUnavailable,
    /// 当前用户的会话历史已经达到容量上限。
    #[error("Playground 会话历史数量已达上限")]
    LimitReached,
    /// 更新目标不存在于当前用户范围。
    #[error("Playground 会话历史不存在")]
    NotFound,
    /// 随机标识或预期 revision 与当前状态冲突。
    #[error("Playground 会话历史版本冲突")]
    Conflict,
    /// 获取连接、事务或执行数据库语句失败。
    #[error("Playground 会话历史数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("Playground 会话历史数据库操作超时")]
    Timeout,
    /// 持久化记录违反会话历史不变量。
    #[error("Playground 会话历史持久化状态损坏")]
    Invariant,
}
