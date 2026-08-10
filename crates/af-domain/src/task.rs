use std::{fmt, str::FromStr};

use thiserror::Error;

/// 上游任务标识的最大 UTF-8 字节数。
pub const MAX_UPSTREAM_TASK_ID_BYTES: usize = 512;
/// 归一化失败原因的最大 UTF-8 字节数。
pub const MAX_TASK_FAILURE_REASON_BYTES: usize = 2_048;

/// 上游任务标识校验错误；不保留原始标识。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskIdentifierError {
    /// 标识不能为空。
    #[error("上游任务标识不能为空")]
    Empty,
    /// 标识超过固定容量上限。
    #[error("上游任务标识过长")]
    TooLong,
    /// 标识包含空白或控制字符。
    #[error("上游任务标识包含非法字符")]
    InvalidCharacter,
}

/// 受限且 Debug 脱敏的供应商任务标识。
///
/// 任务标识只作为上游轮询和本地幂等边界使用，不实现 `Display`、Serde 或直接的
/// Debug 输出，避免供应商标识意外进入日志与公开响应。
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UpstreamTaskId(String);

impl UpstreamTaskId {
    /// 校验并创建上游任务标识。
    pub fn new(value: impl Into<String>) -> Result<Self, TaskIdentifierError> {
        let value = value.into();
        if value.is_empty() {
            return Err(TaskIdentifierError::Empty);
        }
        if value.len() > MAX_UPSTREAM_TASK_ID_BYTES {
            return Err(TaskIdentifierError::TooLong);
        }
        if value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(TaskIdentifierError::InvalidCharacter);
        }
        Ok(Self(value))
    }

    /// 返回已校验的上游任务标识；调用方不得直接记录该值。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for UpstreamTaskId {
    type Error = TaskIdentifierError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for UpstreamTaskId {
    type Error = TaskIdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Debug for UpstreamTaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UpstreamTaskId(<脱敏>)")
    }
}

/// 任务进度校验错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskProgressError {
    /// 进度超过 100% 的万分比上界。
    #[error("任务进度必须处于 0 到 10000 个基点之间")]
    OutOfRange,
}

/// 以基点表示的任务进度，`10000` 表示 100%。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskProgress(u16);

impl TaskProgress {
    /// 进度下界。
    pub const ZERO: Self = Self(0);
    /// 进度上界。
    pub const COMPLETE: Self = Self(10_000);

    /// 校验并创建任务进度。
    pub const fn new(basis_points: u16) -> Result<Self, TaskProgressError> {
        if basis_points > Self::COMPLETE.0 {
            Err(TaskProgressError::OutOfRange)
        } else {
            Ok(Self(basis_points))
        }
    }

    /// 返回万分比基点值。
    #[must_use]
    pub const fn basis_points(self) -> u16 {
        self.0
    }
}

/// 任务失败原因的闭合类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskFailureKind {
    /// 上游拒绝了任务参数或策略。
    Rejected,
    /// 任务被用户或上游取消。
    Cancelled,
    /// 任务超过上游允许的执行时间。
    TimedOut,
    /// 上游报告了无法进一步细分的失败。
    Upstream,
}

/// 任务失败原因校验错误；不保留原始原因内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskFailureError {
    /// 原因字符串不能为空。
    #[error("任务失败原因不能为空")]
    Empty,
    /// 原因字符串超过容量上限。
    #[error("任务失败原因过长")]
    TooLong,
    /// 原因字符串包含控制字符。
    #[error("任务失败原因包含非法字符")]
    InvalidCharacter,
}

/// 已归一化的任务失败信息；Debug 只保留类别，不输出原因正文。
#[derive(Clone, Eq, PartialEq)]
pub struct TaskFailure {
    kind: TaskFailureKind,
    reason: Option<String>,
}

impl TaskFailure {
    /// 创建不带供应商原文的失败信息。
    #[must_use]
    pub const fn without_reason(kind: TaskFailureKind) -> Self {
        Self { kind, reason: None }
    }

    /// 校验并创建带脱敏边界的失败原因。
    pub fn with_reason(
        kind: TaskFailureKind,
        reason: impl Into<String>,
    ) -> Result<Self, TaskFailureError> {
        let reason = reason.into();
        if reason.is_empty() {
            return Err(TaskFailureError::Empty);
        }
        if reason.len() > MAX_TASK_FAILURE_REASON_BYTES {
            return Err(TaskFailureError::TooLong);
        }
        if reason.chars().any(char::is_control) {
            return Err(TaskFailureError::InvalidCharacter);
        }
        Ok(Self {
            kind,
            reason: Some(reason),
        })
    }

    /// 返回归一化失败类别。
    #[must_use]
    pub const fn kind(&self) -> TaskFailureKind {
        self.kind
    }

    /// 返回受边界约束的失败原因；调用方不得直接写入日志。
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

impl fmt::Debug for TaskFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskFailure")
            .field("kind", &self.kind)
            .field("reason", &self.reason.as_ref().map(|_| "<脱敏>"))
            .finish()
    }
}

/// 任务状态值错误；未知字符串必须显式失败，禁止静默降级。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskStateError {
    /// 输入不属于当前闭合集合。
    #[error("任务状态未知")]
    Unknown,
}

/// 供应商无关的闭合任务状态集合。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    /// 任务刚提交，等待上游排队。
    Submitted,
    /// 任务已排队等待执行。
    Queued,
    /// 上游已开始执行。
    Running,
    /// 上游已成功完成。
    Succeeded,
    /// 上游已明确失败。
    Failed,
}

impl TaskState {
    /// 返回内部稳定字符串；仅供已审计的协议或持久化边界使用。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    /// 判断任务是否进入终态。
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

impl FromStr for TaskState {
    type Err = TaskStateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "submitted" => Ok(Self::Submitted),
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            _ => Err(TaskStateError::Unknown),
        }
    }
}

/// 已归一化的任务轮询状态。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskStatus {
    /// 任务刚提交，进度通常为零。
    Submitted { progress: TaskProgress },
    /// 任务已排队等待执行。
    Queued { progress: TaskProgress },
    /// 任务正在执行。
    Running { progress: TaskProgress },
    /// 任务成功完成，进度固定为 100%。
    Succeeded,
    /// 任务明确失败，失败原因正文受限且不参与 Debug 输出。
    Failed { failure: TaskFailure },
}

impl TaskStatus {
    /// 返回不含详细原因的闭合状态。
    #[must_use]
    pub const fn state(&self) -> TaskState {
        match self {
            Self::Submitted { .. } => TaskState::Submitted,
            Self::Queued { .. } => TaskState::Queued,
            Self::Running { .. } => TaskState::Running,
            Self::Succeeded => TaskState::Succeeded,
            Self::Failed { .. } => TaskState::Failed,
        }
    }

    /// 返回当前进度；失败任务没有可靠进度时统一归零。
    #[must_use]
    pub const fn progress(&self) -> TaskProgress {
        match self {
            Self::Submitted { progress }
            | Self::Queued { progress }
            | Self::Running { progress } => *progress,
            Self::Succeeded => TaskProgress::COMPLETE,
            Self::Failed { .. } => TaskProgress::ZERO,
        }
    }

    /// 返回失败信息；非失败状态返回 `None`。
    #[must_use]
    pub const fn failure(&self) -> Option<&TaskFailure> {
        match self {
            Self::Failed { failure } => Some(failure),
            Self::Submitted { .. }
            | Self::Queued { .. }
            | Self::Running { .. }
            | Self::Succeeded => None,
        }
    }

    /// 判断是否已经进入成功或失败终态。
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.state().is_terminal()
    }
}

/// 提交上游后返回的归一化任务句柄和初始状态。
#[derive(Clone, Eq, PartialEq)]
pub struct TaskSubmission {
    task_id: UpstreamTaskId,
    status: TaskStatus,
}

impl TaskSubmission {
    /// 创建一次提交结果。
    #[must_use]
    pub fn new(task_id: UpstreamTaskId, status: TaskStatus) -> Self {
        Self { task_id, status }
    }

    /// 返回后续轮询必须复用的上游任务标识。
    #[must_use]
    pub const fn task_id(&self) -> &UpstreamTaskId {
        &self.task_id
    }

    /// 返回供应商无关的初始任务状态。
    #[must_use]
    pub const fn status(&self) -> &TaskStatus {
        &self.status
    }
}

impl fmt::Debug for TaskSubmission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskSubmission")
            .field("task_id", &self.task_id)
            .field("status", &self.status)
            .finish()
    }
}
