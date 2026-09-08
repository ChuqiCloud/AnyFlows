use std::{fmt, time::Duration};

use af_domain::{
    AsyncTaskAttemptId, AsyncTaskBindingFingerprint, AsyncTaskId, AsyncTaskRequestFingerprint,
    AsyncTaskRequestId, ChannelId, CredentialId, GatewayPrincipal, GroupId, Protocol, TaskStatus,
    UpstreamTaskId, UserId,
};

use crate::async_task::{AsyncTaskCreate, AsyncTaskInputError, AsyncTaskRecord};

const MAX_ATTEMPT_TIMEOUT_MILLIS: u64 = 900_000;

/// 持久化提交 claim 的闭合生命周期。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncTaskSubmissionState {
    /// 尚未开始发网上游，可由新的随机尝试所有者领取。
    Claimed,
    /// 某个尝试所有者可能已经发网上游，其他请求不得自动重提。
    Submitting,
    /// 上游已返回可轮询任务标识，完整绑定已经持久化。
    Accepted,
}

/// 异步视频任务提交时明确携带并可在轮询阶段恢复的分辨率档位。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncTaskVideoResolution {
    /// 480p 输出。
    P480,
    /// 720p 输出。
    P720,
    /// 1080p 输出。
    P1080,
}

impl AsyncTaskVideoResolution {
    pub(super) const fn database_value(self) -> i16 {
        match self {
            Self::P480 => 1,
            Self::P720 => 2,
            Self::P1080 => 3,
        }
    }

    pub(super) const fn from_database(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::P480),
            2 => Some(Self::P720),
            3 => Some(Self::P1080),
            _ => None,
        }
    }

    /// 返回跨数据库与协议边界稳定的分辨率文本。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::P480 => "480p",
            Self::P720 => "720p",
            Self::P1080 => "1080p",
        }
    }
}

/// 创建异步任务提交 claim 时固化的幂等事实。
pub struct AsyncTaskSubmissionClaim {
    pub(super) task_id: AsyncTaskId,
    pub(super) request_id: AsyncTaskRequestId,
    pub(super) principal: GatewayPrincipal,
    pub(super) protocol: Protocol,
    pub(super) requested_model: String,
    pub(super) request_fingerprint: AsyncTaskRequestFingerprint,
    pub(super) video_duration_seconds: Option<u8>,
    pub(super) created_at: u64,
}

impl AsyncTaskSubmissionClaim {
    /// 校验模型和审计时间后构造原子提交 claim。
    pub fn new(
        task_id: AsyncTaskId,
        request_id: AsyncTaskRequestId,
        principal: GatewayPrincipal,
        protocol: Protocol,
        requested_model: String,
        request_fingerprint: AsyncTaskRequestFingerprint,
        video_duration_seconds: Option<u8>,
        created_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        validate_model(&requested_model)?;
        validate_time(created_at)?;
        if video_duration_seconds.is_some_and(|value| !(1..=15).contains(&value)) {
            return Err(AsyncTaskInputError::InvalidBilling);
        }
        Ok(Self {
            task_id,
            request_id,
            principal,
            protocol,
            requested_model,
            request_fingerprint,
            video_duration_seconds,
            created_at,
        })
    }

    #[must_use]
    pub const fn request_id(&self) -> AsyncTaskRequestId {
        self.request_id
    }

    #[must_use]
    pub const fn principal(&self) -> GatewayPrincipal {
        self.principal
    }

    #[must_use]
    pub const fn request_fingerprint(&self) -> AsyncTaskRequestFingerprint {
        self.request_fingerprint
    }

    /// 返回请求明确携带的视频时长；缺失时由计费快照采用上游默认值。
    #[must_use]
    pub const fn video_duration_seconds(&self) -> Option<u8> {
        self.video_duration_seconds
    }
}

impl fmt::Debug for AsyncTaskSubmissionClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskSubmissionClaim(<脱敏>)")
    }
}

/// 把 claim 从可领取状态推进为某个随机尝试所有者独占。
pub struct AsyncTaskSubmissionBegin {
    pub(super) task_id: AsyncTaskId,
    pub(super) user_id: UserId,
    pub(super) expected_version: i64,
    pub(super) attempt_id: AsyncTaskAttemptId,
    pub(super) observed_at: u64,
}

impl AsyncTaskSubmissionBegin {
    /// 构造提交尝试 CAS 命令。
    pub fn new(
        task_id: AsyncTaskId,
        user_id: UserId,
        expected_version: u64,
        attempt_id: AsyncTaskAttemptId,
        observed_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        Ok(Self {
            task_id,
            user_id,
            expected_version: validate_version(expected_version)?,
            attempt_id,
            observed_at: validate_time(observed_at)?,
        })
    }
}

impl fmt::Debug for AsyncTaskSubmissionBegin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskSubmissionBegin(<脱敏>)")
    }
}

/// 确定没有被上游接受时释放提交尝试所有权。
pub struct AsyncTaskSubmissionRelease {
    pub(super) task_id: AsyncTaskId,
    pub(super) user_id: UserId,
    pub(super) expected_version: i64,
    pub(super) attempt_id: AsyncTaskAttemptId,
    pub(super) observed_at: u64,
}

impl AsyncTaskSubmissionRelease {
    /// 构造只允许原尝试所有者执行的释放命令。
    pub fn new(
        task_id: AsyncTaskId,
        user_id: UserId,
        expected_version: u64,
        attempt_id: AsyncTaskAttemptId,
        observed_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        Ok(Self {
            task_id,
            user_id,
            expected_version: validate_version(expected_version)?,
            attempt_id,
            observed_at: validate_time(observed_at)?,
        })
    }
}

impl fmt::Debug for AsyncTaskSubmissionRelease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskSubmissionRelease(<脱敏>)")
    }
}

/// 上游确认接收后写入 claim 的完整恢复绑定。
pub struct AsyncTaskSubmissionAccept {
    pub(super) task_id: AsyncTaskId,
    pub(super) user_id: UserId,
    pub(super) expected_version: i64,
    pub(super) attempt_id: AsyncTaskAttemptId,
    pub(super) target_group_id: GroupId,
    pub(super) upstream_model: String,
    pub(super) channel_id: ChannelId,
    pub(super) credential_id: CredentialId,
    pub(super) credential_revision: u64,
    pub(super) upstream_task_id: UpstreamTaskId,
    pub(super) binding_fingerprint: AsyncTaskBindingFingerprint,
    pub(super) attempt_timeout_millis: i64,
    pub(super) video_resolution: Option<AsyncTaskVideoResolution>,
    pub(super) status: TaskStatus,
    pub(super) observed_at: u64,
}

impl AsyncTaskSubmissionAccept {
    /// 校验恢复绑定和初始任务状态后构造接受事实 CAS 命令。
    #[allow(clippy::too_many_arguments, reason = "字段与持久化恢复绑定一一对应")]
    pub fn new(
        task_id: AsyncTaskId,
        user_id: UserId,
        expected_version: u64,
        attempt_id: AsyncTaskAttemptId,
        target_group_id: GroupId,
        upstream_model: String,
        channel_id: ChannelId,
        credential_id: CredentialId,
        credential_revision: u64,
        upstream_task_id: UpstreamTaskId,
        binding_fingerprint: AsyncTaskBindingFingerprint,
        attempt_timeout: Duration,
        video_resolution: Option<AsyncTaskVideoResolution>,
        status: TaskStatus,
        observed_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        validate_model(&upstream_model)?;
        let attempt_timeout_millis = u64::try_from(attempt_timeout.as_millis())
            .ok()
            .filter(|value| (1..=MAX_ATTEMPT_TIMEOUT_MILLIS).contains(value))
            .and_then(|value| i64::try_from(value).ok())
            .ok_or(AsyncTaskInputError::InvalidAttemptTimeout)?;
        Ok(Self {
            task_id,
            user_id,
            expected_version: validate_version(expected_version)?,
            attempt_id,
            target_group_id,
            upstream_model,
            channel_id,
            credential_id,
            credential_revision,
            upstream_task_id,
            binding_fingerprint,
            attempt_timeout_millis,
            video_resolution,
            status,
            observed_at: validate_time(observed_at)?,
        })
    }
}

impl fmt::Debug for AsyncTaskSubmissionAccept {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskSubmissionAccept(<脱敏>)")
    }
}

/// 所有者范围内的提交 claim 快照。
#[derive(Clone, Eq, PartialEq)]
pub struct AsyncTaskSubmissionRecord {
    pub(super) database_id: i64,
    pub(super) task_id: AsyncTaskId,
    pub(super) request_id: AsyncTaskRequestId,
    pub(super) principal: GatewayPrincipal,
    pub(super) protocol: Protocol,
    pub(super) requested_model: String,
    pub(super) request_fingerprint: AsyncTaskRequestFingerprint,
    pub(super) video_duration_seconds: Option<u8>,
    pub(super) state: AsyncTaskSubmissionState,
    pub(super) attempt_id: Option<AsyncTaskAttemptId>,
    pub(super) target_group_id: Option<GroupId>,
    pub(super) upstream_model: Option<String>,
    pub(super) channel_id: Option<ChannelId>,
    pub(super) credential_id: Option<CredentialId>,
    pub(super) credential_revision: Option<u64>,
    pub(super) upstream_task_id: Option<UpstreamTaskId>,
    pub(super) binding_fingerprint: Option<AsyncTaskBindingFingerprint>,
    pub(super) attempt_timeout_millis: Option<u64>,
    pub(super) video_resolution: Option<AsyncTaskVideoResolution>,
    pub(super) status: Option<TaskStatus>,
    pub(super) version: u64,
    pub(super) accepted_at: Option<u64>,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

impl AsyncTaskSubmissionRecord {
    #[must_use]
    pub const fn task_id(&self) -> AsyncTaskId {
        self.task_id
    }

    #[must_use]
    pub const fn request_id(&self) -> AsyncTaskRequestId {
        self.request_id
    }

    #[must_use]
    pub const fn principal(&self) -> GatewayPrincipal {
        self.principal
    }

    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }

    #[must_use]
    pub fn requested_model(&self) -> &str {
        &self.requested_model
    }

    #[must_use]
    pub const fn request_fingerprint(&self) -> AsyncTaskRequestFingerprint {
        self.request_fingerprint
    }

    /// 返回请求明确携带的视频时长。
    #[must_use]
    pub const fn video_duration_seconds(&self) -> Option<u8> {
        self.video_duration_seconds
    }

    #[must_use]
    pub const fn state(&self) -> AsyncTaskSubmissionState {
        self.state
    }

    #[must_use]
    pub const fn attempt_id(&self) -> Option<AsyncTaskAttemptId> {
        self.attempt_id
    }

    #[must_use]
    pub const fn target_group_id(&self) -> Option<GroupId> {
        self.target_group_id
    }

    #[must_use]
    pub fn upstream_model(&self) -> Option<&str> {
        self.upstream_model.as_deref()
    }

    #[must_use]
    pub const fn channel_id(&self) -> Option<ChannelId> {
        self.channel_id
    }

    #[must_use]
    pub const fn credential_id(&self) -> Option<CredentialId> {
        self.credential_id
    }

    #[must_use]
    pub const fn credential_revision(&self) -> Option<u64> {
        self.credential_revision
    }

    #[must_use]
    pub const fn upstream_task_id(&self) -> Option<&UpstreamTaskId> {
        self.upstream_task_id.as_ref()
    }

    #[must_use]
    pub const fn binding_fingerprint(&self) -> Option<AsyncTaskBindingFingerprint> {
        self.binding_fingerprint
    }

    #[must_use]
    pub fn attempt_timeout(&self) -> Option<Duration> {
        self.attempt_timeout_millis.map(Duration::from_millis)
    }

    /// 返回提交请求明确携带的分辨率；缺失表示没有可靠档位事实。
    #[must_use]
    pub const fn video_resolution(&self) -> Option<AsyncTaskVideoResolution> {
        self.video_resolution
    }

    #[must_use]
    pub const fn status(&self) -> Option<&TaskStatus> {
        self.status.as_ref()
    }

    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub const fn accepted_at(&self) -> Option<u64> {
        self.accepted_at
    }

    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    /// 从已接受 claim 重建最终异步任务写入；未接受状态返回空。
    pub fn task_create(&self) -> Result<Option<AsyncTaskCreate>, AsyncTaskInputError> {
        if self.state != AsyncTaskSubmissionState::Accepted {
            return Ok(None);
        }
        AsyncTaskCreate::new_with_observed_at(
            self.task_id,
            self.request_id,
            self.principal,
            self.protocol,
            self.requested_model.clone(),
            self.upstream_model
                .clone()
                .ok_or(AsyncTaskInputError::InvalidBinding)?,
            self.channel_id.ok_or(AsyncTaskInputError::InvalidBinding)?,
            self.credential_id
                .ok_or(AsyncTaskInputError::InvalidBinding)?,
            self.credential_revision
                .ok_or(AsyncTaskInputError::InvalidBinding)?,
            self.upstream_task_id
                .clone()
                .ok_or(AsyncTaskInputError::InvalidBinding)?,
            self.status
                .clone()
                .ok_or(AsyncTaskInputError::InvalidBinding)?,
            self.created_at,
            self.accepted_at
                .ok_or(AsyncTaskInputError::InvalidBinding)?,
        )
        .map(Some)
    }

    /// 验证最终任务仍与该 claim 的不可变绑定一致。
    #[must_use]
    pub fn matches_task(&self, task: &AsyncTaskRecord) -> bool {
        self.state == AsyncTaskSubmissionState::Accepted
            && task.task_id() == self.task_id
            && task.request_id() == self.request_id
            && task.principal() == self.principal
            && task.protocol() == self.protocol
            && task.requested_model() == self.requested_model
            && self
                .upstream_model
                .as_deref()
                .is_some_and(|model| task.upstream_model() == model)
            && self.channel_id == Some(task.channel_id())
            && self.credential_id == Some(task.credential_id())
            && self.credential_revision == Some(task.credential_revision())
            && self.upstream_task_id.as_ref() == Some(task.upstream_task_id())
    }

    pub(super) fn matches_claim(&self, write: &AsyncTaskSubmissionClaim) -> bool {
        self.request_id == write.request_id
            && self.principal == write.principal
            && self.protocol == write.protocol
            && self.requested_model == write.requested_model
            && self.request_fingerprint == write.request_fingerprint
            && self.video_duration_seconds == write.video_duration_seconds
    }

    pub(super) fn matches_accept(&self, write: &AsyncTaskSubmissionAccept) -> bool {
        self.state == AsyncTaskSubmissionState::Accepted
            && self.attempt_id == Some(write.attempt_id)
            && self.target_group_id == Some(write.target_group_id)
            && self.upstream_model.as_deref() == Some(write.upstream_model.as_str())
            && self.channel_id == Some(write.channel_id)
            && self.credential_id == Some(write.credential_id)
            && self.credential_revision == Some(write.credential_revision)
            && self.upstream_task_id.as_ref() == Some(&write.upstream_task_id)
            && self.binding_fingerprint == Some(write.binding_fingerprint)
            && self.attempt_timeout_millis == u64::try_from(write.attempt_timeout_millis).ok()
            && self.video_resolution == write.video_resolution
            && self.status.as_ref() == Some(&write.status)
    }
}

impl fmt::Debug for AsyncTaskSubmissionRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsyncTaskSubmissionRecord")
            .field("state", &self.state)
            .field("version", &self.version)
            .field("has_attempt", &self.attempt_id.is_some())
            .field("has_binding", &self.binding_fingerprint.is_some())
            .finish()
    }
}

/// 幂等创建提交 claim 的闭合结果。
pub enum AsyncTaskSubmissionClaimOutcome {
    Created(AsyncTaskSubmissionRecord),
    Existing(AsyncTaskSubmissionRecord),
    NotFound,
}

/// 提交 claim 状态 CAS 的闭合结果。
pub enum AsyncTaskSubmissionMutationOutcome {
    Applied(AsyncTaskSubmissionRecord),
    Existing(AsyncTaskSubmissionRecord),
    NotFound,
}

fn validate_model(value: &str) -> Result<(), AsyncTaskInputError> {
    if value.is_empty()
        || value.len() > af_domain::MAX_MODEL_NAME_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(AsyncTaskInputError::InvalidModel)
    } else {
        Ok(())
    }
}

fn validate_version(value: u64) -> Result<i64, AsyncTaskInputError> {
    let value = i64::try_from(value).map_err(|_| AsyncTaskInputError::InvalidVersion)?;
    if value <= 0 || value == i64::MAX {
        Err(AsyncTaskInputError::InvalidVersion)
    } else {
        Ok(value)
    }
}

fn validate_time(value: u64) -> Result<u64, AsyncTaskInputError> {
    i64::try_from(value)
        .map(|_| value)
        .map_err(|_| AsyncTaskInputError::InvalidTiming)
}
