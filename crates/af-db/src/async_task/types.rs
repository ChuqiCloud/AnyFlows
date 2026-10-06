use std::fmt;

use af_domain::{
    AsyncTaskId, AsyncTaskRequestId, ChannelId, CredentialId, GatewayPrincipal,
    MAX_MODEL_NAME_BYTES, Protocol, TaskStatus, UpstreamTaskId,
};
use thiserror::Error;

/// 创建异步任务时固化的不可变调度事实。
pub struct AsyncTaskCreate {
    pub(super) task_id: AsyncTaskId,
    pub(super) request_id: AsyncTaskRequestId,
    pub(super) principal: GatewayPrincipal,
    pub(super) protocol: Protocol,
    pub(super) requested_model: String,
    pub(super) upstream_model: String,
    pub(super) channel_id: ChannelId,
    pub(super) credential_id: CredentialId,
    pub(super) credential_revision: u64,
    pub(super) upstream_task_id: UpstreamTaskId,
    pub(super) status: TaskStatus,
    pub(super) created_at: u64,
    pub(super) observed_at: u64,
}

impl AsyncTaskCreate {
    /// 校验模型名称和时间边界后构造任务持久化事实。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与任务不可变绑定事实一一对应"
    )]
    pub fn new(
        task_id: AsyncTaskId,
        request_id: AsyncTaskRequestId,
        principal: GatewayPrincipal,
        protocol: Protocol,
        requested_model: String,
        upstream_model: String,
        channel_id: ChannelId,
        credential_id: CredentialId,
        credential_revision: u64,
        upstream_task_id: UpstreamTaskId,
        status: TaskStatus,
        created_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        Self::new_with_observed_at(
            task_id,
            request_id,
            principal,
            protocol,
            requested_model,
            upstream_model,
            channel_id,
            credential_id,
            credential_revision,
            upstream_task_id,
            status,
            created_at,
            created_at,
        )
    }

    /// 分别固化 claim 创建时间和上游初始状态观察时间。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与任务不可变绑定事实及初始状态审计一一对应"
    )]
    pub fn new_with_observed_at(
        task_id: AsyncTaskId,
        request_id: AsyncTaskRequestId,
        principal: GatewayPrincipal,
        protocol: Protocol,
        requested_model: String,
        upstream_model: String,
        channel_id: ChannelId,
        credential_id: CredentialId,
        credential_revision: u64,
        upstream_task_id: UpstreamTaskId,
        status: TaskStatus,
        created_at: u64,
        observed_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        if !valid_model_name(&requested_model) || !valid_model_name(&upstream_model) {
            return Err(AsyncTaskInputError::InvalidModel);
        }
        validate_time(created_at)?;
        validate_time(observed_at)?;
        if observed_at < created_at {
            return Err(AsyncTaskInputError::InvalidTiming);
        }
        Ok(Self {
            task_id,
            request_id,
            principal,
            protocol,
            requested_model,
            upstream_model,
            channel_id,
            credential_id,
            credential_revision,
            upstream_task_id,
            status,
            created_at,
            observed_at,
        })
    }
}

impl fmt::Debug for AsyncTaskCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskCreate(<脱敏>)")
    }
}

/// 使用乐观锁推进异步任务状态的命令。
pub struct AsyncTaskTransition {
    pub(super) task_id: AsyncTaskId,
    pub(super) user_id: af_domain::UserId,
    pub(super) expected_version: i64,
    pub(super) status: TaskStatus,
    pub(super) observed_at: u64,
}

impl AsyncTaskTransition {
    /// 校验预期版本和状态观察时间后构造 CAS 命令。
    pub fn new(
        task_id: AsyncTaskId,
        user_id: af_domain::UserId,
        expected_version: u64,
        status: TaskStatus,
        observed_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        let expected_version =
            i64::try_from(expected_version).map_err(|_| AsyncTaskInputError::InvalidVersion)?;
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(AsyncTaskInputError::InvalidVersion);
        }
        validate_time(observed_at)?;
        Ok(Self {
            task_id,
            user_id,
            expected_version,
            status,
            observed_at,
        })
    }
}

impl fmt::Debug for AsyncTaskTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskTransition(<脱敏>)")
    }
}

/// 已持久化异步任务的所有者范围状态快照。
#[derive(Clone, Eq, PartialEq)]
pub struct AsyncTaskRecord {
    pub(super) database_id: i64,
    pub(super) task_id: AsyncTaskId,
    pub(super) request_id: AsyncTaskRequestId,
    pub(super) principal: GatewayPrincipal,
    pub(super) protocol: Protocol,
    pub(super) requested_model: String,
    pub(super) upstream_model: String,
    pub(super) channel_id: ChannelId,
    pub(super) credential_id: CredentialId,
    pub(super) credential_revision: u64,
    pub(super) upstream_task_id: UpstreamTaskId,
    pub(super) status: TaskStatus,
    pub(super) version: u64,
    pub(super) terminal_at: Option<u64>,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

/// 异步任务倒序分页游标，只携带稳定排序所需的非敏感位置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AsyncTaskPageCursor {
    pub(super) task_id: AsyncTaskId,
}

impl AsyncTaskPageCursor {
    /// 使用已经校验的公开任务标识构造游标。
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

/// 一页 owner-scoped 异步任务，只包含仓储内已持久化的状态快照。
pub struct AsyncTaskPageRecord {
    pub(super) tasks: Vec<AsyncTaskRecord>,
    pub(super) next_cursor: Option<AsyncTaskPageCursor>,
}

impl AsyncTaskPageRecord {
    /// 消费页面并返回任务快照和下一页游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<AsyncTaskRecord>, Option<AsyncTaskPageCursor>) {
        (self.tasks, self.next_cursor)
    }
}

impl fmt::Debug for AsyncTaskPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskPageRecord(<脱敏>)")
    }
}

impl AsyncTaskRecord {
    /// 返回任务公开标识。
    #[must_use]
    pub const fn task_id(&self) -> AsyncTaskId {
        self.task_id
    }

    /// 返回客户端创建任务时使用的幂等标识。
    #[must_use]
    pub const fn request_id(&self) -> AsyncTaskRequestId {
        self.request_id
    }

    /// 返回创建任务时通过鉴权的主体快照。
    #[must_use]
    pub const fn principal(&self) -> GatewayPrincipal {
        self.principal
    }

    /// 返回任务使用的规范协议。
    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// 返回客户端请求模型；调用方不得把该值直接写入日志。
    #[must_use]
    pub fn requested_model(&self) -> &str {
        &self.requested_model
    }

    /// 返回完成模型映射后的上游模型；调用方不得把该值直接写入日志。
    #[must_use]
    pub fn upstream_model(&self) -> &str {
        &self.upstream_model
    }

    /// 返回任务绑定渠道。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回任务绑定凭据。
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.credential_id
    }

    /// 返回完整凭据密文封套派生的版本。
    #[must_use]
    pub const fn credential_revision(&self) -> u64 {
        self.credential_revision
    }

    /// 返回上游任务标识；调用方不得把该值直接写入日志。
    #[must_use]
    pub const fn upstream_task_id(&self) -> &UpstreamTaskId {
        &self.upstream_task_id
    }

    /// 返回当前闭合任务状态。
    #[must_use]
    pub const fn status(&self) -> &TaskStatus {
        &self.status
    }

    /// 返回当前乐观锁版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回进入成功或失败终态的时间。
    #[must_use]
    pub const fn terminal_at(&self) -> Option<u64> {
        self.terminal_at
    }

    /// 返回任务创建时间。
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// 返回最近一次状态迁移时间。
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub(super) fn matches_create(&self, write: &AsyncTaskCreate) -> bool {
        self.task_id == write.task_id
            && self.request_id == write.request_id
            && self.principal == write.principal
            && self.protocol == write.protocol
            && self.requested_model == write.requested_model
            && self.upstream_model == write.upstream_model
            && self.channel_id == write.channel_id
            && self.credential_id == write.credential_id
            && self.credential_revision == write.credential_revision
            && self.upstream_task_id == write.upstream_task_id
        // 状态和服务端接收时间是可变事实，不参与客户端幂等载荷比较。
    }
}

impl fmt::Debug for AsyncTaskRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskRecord(<脱敏>)")
    }
}

/// 幂等创建异步任务后的闭合结果。
pub enum AsyncTaskCreateOutcome {
    /// 本次调用创建了新任务。
    Created(AsyncTaskRecord),
    /// 相同用户、幂等键和不可变载荷已经存在。
    Existing(AsyncTaskRecord),
    /// 所有者或绑定引用不存在，或引用关系不一致。
    NotFound,
}

/// CAS 状态推进后的闭合结果。
pub enum AsyncTaskTransitionOutcome {
    /// 本次调用完成了一次合法前向迁移。
    Applied(AsyncTaskRecord),
    /// 相同状态命令已经生效，或当前版本已经保存相同状态。
    Existing(AsyncTaskRecord),
    /// 所有者范围内不存在该任务。
    NotFound,
}

/// 异步任务输入构造错误；不携带模型或任务标识。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AsyncTaskInputError {
    /// 请求模型或上游模型不符合固定容量与规范化边界。
    #[error("异步任务模型名称无效")]
    InvalidModel,
    /// 时间戳超出数据库可表示范围。
    #[error("异步任务时间边界无效")]
    InvalidTiming,
    /// CAS 版本不是可递增的正整数。
    #[error("异步任务版本无效")]
    InvalidVersion,
    /// 提交尝试等待时间无法安全持久化或超出渠道硬上限。
    #[error("异步任务提交超时无效")]
    InvalidAttemptTimeout,
    /// 已接受 claim 缺少恢复任务所需的完整绑定。
    #[error("异步任务提交绑定无效")]
    InvalidBinding,
    /// 任务计费快照、额度、价目或状态参数无效。
    #[error("异步任务计费输入无效")]
    InvalidBilling,
}

/// 异步任务仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AsyncTaskRepositoryConfigError {
    /// 零超时无法形成有效数据库操作截止时间。
    #[error("异步任务仓储操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 异步任务仓储错误；不携带模型、上游任务或幂等键内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AsyncTaskRepositoryError {
    /// 幂等键载荷冲突、非法状态迁移或乐观锁版本冲突。
    #[error("异步任务状态或幂等载荷冲突")]
    Conflict,
    /// 获取连接或执行确定未提交的数据库操作失败。
    #[error("异步任务数据库操作失败")]
    Query,
    /// 写入超时或提交失败，调用方必须按原标识查询后重放。
    #[error("异步任务操作结果未知")]
    OutcomeUnknown,
    /// 只读操作超过受控截止时间。
    #[error("异步任务查询超时")]
    Timeout,
    /// 数据库内容违反领域不变量。
    #[error("异步任务持久化状态无效")]
    Invariant,
}

fn valid_model_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MODEL_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn validate_time(value: u64) -> Result<(), AsyncTaskInputError> {
    i64::try_from(value)
        .map(|_| ())
        .map_err(|_| AsyncTaskInputError::InvalidTiming)
}
