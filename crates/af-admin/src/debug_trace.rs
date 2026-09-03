use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    DebugTraceAttemptOutcome, DebugTraceAttemptRecord, DebugTraceDetailRecord,
    DebugTraceFailureKind, DebugTraceListQuery, DebugTraceOperation, DebugTraceOutcome,
    DebugTracePageRecord, DebugTraceProtocol, DebugTraceRepository, DebugTraceRepositoryError,
    DebugTraceSettingsRecord, DebugTraceSettingsWrite, DebugTraceSnapshotRecord,
    DebugTraceSnapshotScope, DebugTraceSummaryRecord,
};
use af_domain::{ChannelId, CredentialId, GroupId, TokenId, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroize as _;

use crate::{SessionPrincipal, SessionRole};

/// 管理端调试追踪列表的默认页大小。
pub const DEFAULT_ADMIN_DEBUG_TRACE_PAGE_SIZE: usize = 50;

/// 管理端可见的调试追踪设置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDebugTraceSettings {
    enabled: bool,
    sample_per_million: i64,
    retention_hours: i32,
    capture_headers: bool,
    capture_bodies: bool,
    max_body_bytes: i32,
    version: i64,
}

impl AdminDebugTraceSettings {
    fn from_record(record: DebugTraceSettingsRecord) -> Self {
        Self {
            enabled: record.enabled(),
            sample_per_million: record.sample_per_million(),
            retention_hours: record.retention_hours(),
            capture_headers: record.capture_headers(),
            capture_bodies: record.capture_bodies(),
            max_body_bytes: record.max_body_bytes(),
            version: record.version(),
        }
    }

    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn sample_per_million(self) -> i64 {
        self.sample_per_million
    }

    #[must_use]
    pub const fn retention_hours(self) -> i32 {
        self.retention_hours
    }

    #[must_use]
    pub const fn capture_headers(self) -> bool {
        self.capture_headers
    }

    #[must_use]
    pub const fn capture_bodies(self) -> bool {
        self.capture_bodies
    }

    #[must_use]
    pub const fn max_body_bytes(self) -> i32 {
        self.max_body_bytes
    }

    #[must_use]
    pub const fn version(self) -> i64 {
        self.version
    }
}

/// 管理员完整覆盖调试追踪设置的命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDebugTraceSettingsCommand {
    write: DebugTraceSettingsWrite,
}

impl AdminDebugTraceSettingsCommand {
    pub fn new(
        enabled: bool,
        sample_per_million: i64,
        retention_hours: i32,
        capture_headers: bool,
        capture_bodies: bool,
        max_body_bytes: i32,
    ) -> Result<Self, AdminDebugTraceError> {
        let write = DebugTraceSettingsWrite::new(enabled, sample_per_million, retention_hours)
            .and_then(|write| {
                write.with_diagnostics(capture_headers, capture_bodies, max_body_bytes)
            })
            .map_err(|_| AdminDebugTraceError::InvalidInput)?;
        Ok(Self { write })
    }
}

/// 已校验的管理端调试追踪列表查询。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDebugTraceListQuery {
    before: Option<i64>,
    limit: usize,
    outcome: Option<AdminDebugTraceOutcome>,
    requested_model: Option<String>,
    request_id: Option<String>,
}

impl AdminDebugTraceListQuery {
    pub fn new(
        before: Option<i64>,
        limit: usize,
        outcome: Option<AdminDebugTraceOutcome>,
        requested_model: Option<String>,
        request_id: Option<String>,
    ) -> Result<Self, AdminDebugTraceError> {
        let requested_model = requested_model.filter(|model| !model.is_empty());
        DebugTraceListQuery::new(
            before,
            limit,
            outcome.map(AdminDebugTraceOutcome::into_database),
            requested_model.clone(),
            request_id.clone(),
        )
        .map_err(|_| AdminDebugTraceError::InvalidInput)?;
        Ok(Self {
            before,
            limit,
            outcome,
            requested_model,
            request_id,
        })
    }

    fn into_database(self) -> Result<DebugTraceListQuery, AdminDebugTraceError> {
        DebugTraceListQuery::new(
            self.before,
            self.limit,
            self.outcome.map(AdminDebugTraceOutcome::into_database),
            self.requested_model,
            self.request_id,
        )
        .map_err(|_| AdminDebugTraceError::InvalidInput)
    }
}

impl Default for AdminDebugTraceListQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: DEFAULT_ADMIN_DEBUG_TRACE_PAGE_SIZE,
            outcome: None,
            requested_model: None,
            request_id: None,
        }
    }
}

macro_rules! admin_trace_enum {
    (
        $(#[$meta:meta])*
        $name:ident, $database:ty, { $($variant:ident),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            const fn from_database(value: $database) -> Self {
                match value {
                    $(<$database>::$variant => Self::$variant),+
                }
            }

            #[allow(dead_code)]
            const fn into_database(self) -> $database {
                match self {
                    $(Self::$variant => <$database>::$variant),+
                }
            }
        }
    };
}

admin_trace_enum!(
    /// 请求进入和离开网关时使用的协议。
    AdminDebugTraceProtocol,
    DebugTraceProtocol,
    { OpenAiChat, OpenAiResponses, Anthropic, Gemini }
);
admin_trace_enum!(
    /// 请求执行的规范化操作。
    AdminDebugTraceOperation,
    DebugTraceOperation,
    { Chat, Responses }
);
admin_trace_enum!(
    /// 请求级追踪结果。
    AdminDebugTraceOutcome,
    DebugTraceOutcome,
    { Succeeded, Failed }
);
admin_trace_enum!(
    /// 单个候选的追踪结果。
    AdminDebugTraceAttemptOutcome,
    DebugTraceAttemptOutcome,
    { Succeeded, Failed }
);
admin_trace_enum!(
    /// 不包含原始错误文本的闭合失败分类。
    AdminDebugTraceFailureKind,
    DebugTraceFailureKind,
    {
        AuthExpired,
        AuthRevoked,
        AccountDisabled,
        RateLimited,
        Overloaded,
        QuotaExhausted,
        ModelUnsupported,
        ProtocolError,
        ServerError,
        BadRequest,
        Network
    }
);

/// 管理端调试追踪摘要。
pub struct AdminDebugTrace {
    id: i64,
    request_id: String,
    user_id: UserId,
    token_id: TokenId,
    group_id: GroupId,
    requested_model: String,
    downstream_protocol: AdminDebugTraceProtocol,
    upstream_protocol: AdminDebugTraceProtocol,
    operation: AdminDebugTraceOperation,
    outcome: AdminDebugTraceOutcome,
    selected_channel_id: Option<ChannelId>,
    selected_credential_id: Option<CredentialId>,
    routing_elapsed_ms: i64,
    attempt_count: i32,
    downstream_method: Option<String>,
    downstream_path: Option<String>,
    created_at: i64,
}

impl AdminDebugTrace {
    fn from_record(record: DebugTraceSummaryRecord) -> Result<Self, AdminDebugTraceError> {
        Ok(Self {
            id: record.id(),
            request_id: record.request_id().to_owned(),
            user_id: UserId::new(record.user_id()).map_err(|_| AdminDebugTraceError::Internal)?,
            token_id: TokenId::new(record.token_id())
                .map_err(|_| AdminDebugTraceError::Internal)?,
            group_id: GroupId::new(record.group_id())
                .map_err(|_| AdminDebugTraceError::Internal)?,
            requested_model: record.requested_model().to_owned(),
            downstream_protocol: AdminDebugTraceProtocol::from_database(
                record.downstream_protocol(),
            ),
            upstream_protocol: AdminDebugTraceProtocol::from_database(record.upstream_protocol()),
            operation: AdminDebugTraceOperation::from_database(record.operation()),
            outcome: AdminDebugTraceOutcome::from_database(record.outcome()),
            selected_channel_id: record
                .selected_channel_id()
                .map(ChannelId::new)
                .transpose()
                .map_err(|_| AdminDebugTraceError::Internal)?,
            selected_credential_id: record
                .selected_credential_id()
                .map(CredentialId::new)
                .transpose()
                .map_err(|_| AdminDebugTraceError::Internal)?,
            routing_elapsed_ms: record.routing_elapsed_ms(),
            attempt_count: record.attempt_count(),
            downstream_method: record.downstream_method().map(str::to_owned),
            downstream_path: record.downstream_path().map(str::to_owned),
            created_at: record.created_at().unix_timestamp(),
        })
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
    #[must_use]
    pub const fn token_id(&self) -> TokenId {
        self.token_id
    }
    #[must_use]
    pub const fn group_id(&self) -> GroupId {
        self.group_id
    }
    #[must_use]
    pub fn requested_model(&self) -> &str {
        &self.requested_model
    }
    #[must_use]
    pub const fn downstream_protocol(&self) -> AdminDebugTraceProtocol {
        self.downstream_protocol
    }
    #[must_use]
    pub const fn upstream_protocol(&self) -> AdminDebugTraceProtocol {
        self.upstream_protocol
    }
    #[must_use]
    pub const fn operation(&self) -> AdminDebugTraceOperation {
        self.operation
    }
    #[must_use]
    pub const fn outcome(&self) -> AdminDebugTraceOutcome {
        self.outcome
    }
    #[must_use]
    pub const fn selected_channel_id(&self) -> Option<ChannelId> {
        self.selected_channel_id
    }
    #[must_use]
    pub const fn selected_credential_id(&self) -> Option<CredentialId> {
        self.selected_credential_id
    }
    #[must_use]
    pub const fn routing_elapsed_ms(&self) -> i64 {
        self.routing_elapsed_ms
    }
    #[must_use]
    pub const fn attempt_count(&self) -> i32 {
        self.attempt_count
    }
    #[must_use]
    pub fn downstream_method(&self) -> Option<&str> {
        self.downstream_method.as_deref()
    }
    #[must_use]
    pub fn downstream_path(&self) -> Option<&str> {
        self.downstream_path.as_deref()
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
}

impl fmt::Debug for AdminDebugTrace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminDebugTrace(<已脱敏>)")
    }
}

/// 管理端可见的单个候选时间线节点。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDebugTraceAttempt {
    candidate_index: i16,
    channel_id: ChannelId,
    credential_id: CredentialId,
    outcome: AdminDebugTraceAttemptOutcome,
    failure_kind: Option<AdminDebugTraceFailureKind>,
    upstream_status: Option<i16>,
    retry_decision: bool,
    elapsed_ms: i64,
    client_simulation_profile: Option<af_domain::ClientSimulationProfile>,
    client_simulation_result: Option<af_domain::ClientSimulationResult>,
    client_simulation_body_profile: Option<af_domain::ClientSimulationBodyProfile>,
    client_simulation_body_result: Option<af_domain::ClientSimulationBodyPatchResult>,
    request_method: Option<String>,
    request_url: Option<String>,
    response_status: Option<i16>,
    response_streamed: bool,
}

impl AdminDebugTraceAttempt {
    fn from_record(record: &DebugTraceAttemptRecord) -> Result<Self, AdminDebugTraceError> {
        Ok(Self {
            candidate_index: record.candidate_index(),
            channel_id: ChannelId::new(record.channel_id())
                .map_err(|_| AdminDebugTraceError::Internal)?,
            credential_id: CredentialId::new(record.credential_id())
                .map_err(|_| AdminDebugTraceError::Internal)?,
            outcome: AdminDebugTraceAttemptOutcome::from_database(record.outcome()),
            failure_kind: record
                .failure_kind()
                .map(AdminDebugTraceFailureKind::from_database),
            upstream_status: record.upstream_status(),
            retry_decision: record.retry_decision(),
            elapsed_ms: record.elapsed_ms(),
            client_simulation_profile: record.client_simulation_profile(),
            client_simulation_result: record.client_simulation_result(),
            client_simulation_body_profile: record.client_simulation_body_profile(),
            client_simulation_body_result: record.client_simulation_body_result(),
            request_method: record.request_method().map(str::to_owned),
            request_url: record.request_url().map(str::to_owned),
            response_status: record.response_status(),
            response_streamed: record.response_streamed(),
        })
    }

    #[must_use]
    pub const fn candidate_index(&self) -> i16 {
        self.candidate_index
    }
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.credential_id
    }
    #[must_use]
    pub const fn outcome(&self) -> AdminDebugTraceAttemptOutcome {
        self.outcome
    }
    #[must_use]
    pub const fn failure_kind(&self) -> Option<AdminDebugTraceFailureKind> {
        self.failure_kind
    }
    #[must_use]
    pub const fn upstream_status(&self) -> Option<i16> {
        self.upstream_status
    }
    #[must_use]
    pub const fn retry_decision(&self) -> bool {
        self.retry_decision
    }
    #[must_use]
    pub const fn elapsed_ms(&self) -> i64 {
        self.elapsed_ms
    }
    #[must_use]
    pub const fn client_simulation_profile(&self) -> Option<af_domain::ClientSimulationProfile> {
        self.client_simulation_profile
    }
    #[must_use]
    pub const fn client_simulation_result(&self) -> Option<af_domain::ClientSimulationResult> {
        self.client_simulation_result
    }
    #[must_use]
    pub const fn client_simulation_body_profile(
        &self,
    ) -> Option<af_domain::ClientSimulationBodyProfile> {
        self.client_simulation_body_profile
    }
    #[must_use]
    pub const fn client_simulation_body_result(
        &self,
    ) -> Option<af_domain::ClientSimulationBodyPatchResult> {
        self.client_simulation_body_result
    }
    #[must_use]
    pub fn request_method(&self) -> Option<&str> {
        self.request_method.as_deref()
    }
    #[must_use]
    pub fn request_url(&self) -> Option<&str> {
        self.request_url.as_deref()
    }
    #[must_use]
    pub const fn response_status(&self) -> Option<i16> {
        self.response_status
    }
    #[must_use]
    pub const fn response_streamed(&self) -> bool {
        self.response_streamed
    }
}

/// 一页调试追踪摘要。
pub struct AdminDebugTracePage {
    traces: Vec<AdminDebugTrace>,
    next_cursor: Option<i64>,
}

impl AdminDebugTracePage {
    fn from_record(page: DebugTracePageRecord) -> Result<Self, AdminDebugTraceError> {
        Ok(Self {
            traces: page
                .traces()
                .iter()
                .cloned()
                .map(AdminDebugTrace::from_record)
                .collect::<Result<Vec<_>, _>>()?,
            next_cursor: page.next_cursor(),
        })
    }

    #[must_use]
    pub fn traces(&self) -> &[AdminDebugTrace] {
        &self.traces
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

/// 单条调试追踪及其候选时间线。
pub struct AdminDebugTraceDetail {
    trace: AdminDebugTrace,
    attempts: Vec<AdminDebugTraceAttempt>,
}

/// 管理端显式选择的敏感快照读取范围。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminDebugTraceSnapshotScope {
    Headers,
    Bodies,
}

impl AdminDebugTraceSnapshotScope {
    const fn into_database(self) -> DebugTraceSnapshotScope {
        match self {
            Self::Headers => DebugTraceSnapshotScope::Headers,
            Self::Bodies => DebugTraceSnapshotScope::Bodies,
        }
    }

    const fn from_database(value: DebugTraceSnapshotScope) -> Self {
        match value {
            DebugTraceSnapshotScope::Headers => Self::Headers,
            DebugTraceSnapshotScope::Bodies => Self::Bodies,
        }
    }
}

/// 单个候选在本次授权范围内的敏感快照。
pub struct AdminDebugTraceAttemptSnapshot {
    candidate_index: i16,
    request_json: Option<String>,
    response_json: Option<String>,
}

impl AdminDebugTraceAttemptSnapshot {
    #[must_use]
    pub const fn candidate_index(&self) -> i16 {
        self.candidate_index
    }

    #[must_use]
    pub fn request_json(&self) -> Option<&str> {
        self.request_json.as_deref()
    }

    #[must_use]
    pub fn response_json(&self) -> Option<&str> {
        self.response_json.as_deref()
    }
}

impl fmt::Debug for AdminDebugTraceAttemptSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminDebugTraceAttemptSnapshot(<已脱敏>)")
    }
}

impl Drop for AdminDebugTraceAttemptSnapshot {
    fn drop(&mut self) {
        if let Some(value) = self.request_json.as_mut() {
            value.zeroize();
        }
        if let Some(value) = self.response_json.as_mut() {
            value.zeroize();
        }
    }
}

/// 一次已经写入读取审计的 Header 或正文结果。
pub struct AdminDebugTraceSnapshots {
    scope: AdminDebugTraceSnapshotScope,
    downstream_json: Option<String>,
    attempts: Vec<AdminDebugTraceAttemptSnapshot>,
}

impl AdminDebugTraceSnapshots {
    fn from_record(record: &DebugTraceSnapshotRecord) -> Self {
        Self {
            scope: AdminDebugTraceSnapshotScope::from_database(record.scope()),
            downstream_json: record.downstream_json().map(str::to_owned),
            attempts: record
                .attempts()
                .iter()
                .map(|attempt| AdminDebugTraceAttemptSnapshot {
                    candidate_index: attempt.candidate_index(),
                    request_json: attempt.request_json().map(str::to_owned),
                    response_json: attempt.response_json().map(str::to_owned),
                })
                .collect(),
        }
    }

    #[must_use]
    pub const fn scope(&self) -> AdminDebugTraceSnapshotScope {
        self.scope
    }

    #[must_use]
    pub fn downstream_json(&self) -> Option<&str> {
        self.downstream_json.as_deref()
    }

    #[must_use]
    pub fn attempts(&self) -> &[AdminDebugTraceAttemptSnapshot] {
        &self.attempts
    }
}

impl fmt::Debug for AdminDebugTraceSnapshots {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminDebugTraceSnapshots(<已脱敏>)")
    }
}

impl Drop for AdminDebugTraceSnapshots {
    fn drop(&mut self) {
        if let Some(value) = self.downstream_json.as_mut() {
            value.zeroize();
        }
    }
}

impl AdminDebugTraceDetail {
    fn from_record(record: DebugTraceDetailRecord) -> Result<Self, AdminDebugTraceError> {
        Ok(Self {
            trace: AdminDebugTrace::from_record(record.summary().clone())?,
            attempts: record
                .attempts()
                .iter()
                .map(AdminDebugTraceAttempt::from_record)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    #[must_use]
    pub const fn trace(&self) -> &AdminDebugTrace {
        &self.trace
    }
    #[must_use]
    pub fn attempts(&self) -> &[AdminDebugTraceAttempt] {
        &self.attempts
    }
}

/// 设置保存后更新当前进程快照的窄接口。
pub trait AdminDebugTraceSettingsRuntimeApplier: Send + Sync {
    fn apply(&self, record: DebugTraceSettingsRecord);
}

/// 管理端调试追踪错误边界。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminDebugTraceError {
    #[error("调试追踪输入无效")]
    InvalidInput,
    #[error("当前会话无权管理调试追踪")]
    Forbidden,
    #[error("调试追踪记录不存在")]
    NotFound,
    #[error("调试追踪服务内部失败")]
    Internal,
}

pub type AdminDebugTraceSettingsFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminDebugTraceSettings, AdminDebugTraceError>> + Send + 'a>,
>;
pub type AdminDebugTraceUpdateFuture<'a> = AdminDebugTraceSettingsFuture<'a>;
pub type AdminDebugTraceListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminDebugTracePage, AdminDebugTraceError>> + Send + 'a>>;
pub type AdminDebugTraceGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminDebugTraceDetail, AdminDebugTraceError>> + Send + 'a>>;
pub type AdminDebugTraceDetailFuture<'a> = AdminDebugTraceGetFuture<'a>;
pub type AdminDebugTraceSnapshotsFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminDebugTraceSnapshots, AdminDebugTraceError>> + Send + 'a>,
>;

/// 管理员调试追踪设置与只读时间线端口。
pub trait AdminDebugTraceService: Send + Sync {
    fn settings<'a>(&'a self, principal: SessionPrincipal) -> AdminDebugTraceSettingsFuture<'a>;
    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminDebugTraceSettingsCommand,
    ) -> AdminDebugTraceUpdateFuture<'a>;
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminDebugTraceListQuery,
    ) -> AdminDebugTraceListFuture<'a>;
    fn detail<'a>(
        &'a self,
        principal: SessionPrincipal,
        trace_id: i64,
    ) -> AdminDebugTraceGetFuture<'a>;
    fn snapshots<'a>(
        &'a self,
        principal: SessionPrincipal,
        trace_id: i64,
        scope: AdminDebugTraceSnapshotScope,
    ) -> AdminDebugTraceSnapshotsFuture<'a>;
}

/// 使用数据库仓储和当前实例快照实现管理员调试追踪用例。
pub struct DatabaseAdminDebugTraceService {
    repository: DebugTraceRepository,
    runtime: Arc<dyn AdminDebugTraceSettingsRuntimeApplier>,
}

impl DatabaseAdminDebugTraceService {
    #[must_use]
    pub fn new(
        repository: DebugTraceRepository,
        runtime: Arc<dyn AdminDebugTraceSettingsRuntimeApplier>,
    ) -> Self {
        Self {
            repository,
            runtime,
        }
    }
}

impl AdminDebugTraceService for DatabaseAdminDebugTraceService {
    fn settings<'a>(&'a self, principal: SessionPrincipal) -> AdminDebugTraceSettingsFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .settings()
                .await
                .map(AdminDebugTraceSettings::from_record)
                .map_err(map_repository_error)
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminDebugTraceSettingsCommand,
    ) -> AdminDebugTraceUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let saved = self
                .repository
                .update_settings(command.write)
                .await
                .map_err(map_repository_error)?;
            self.runtime.apply(saved);
            Ok(AdminDebugTraceSettings::from_record(saved))
        })
    }

    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminDebugTraceListQuery,
    ) -> AdminDebugTraceListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .repository
                .list(query.into_database()?)
                .await
                .map_err(map_repository_error)?;
            AdminDebugTracePage::from_record(page)
        })
    }

    fn detail<'a>(
        &'a self,
        principal: SessionPrincipal,
        trace_id: i64,
    ) -> AdminDebugTraceGetFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            if trace_id < 1 {
                return Err(AdminDebugTraceError::InvalidInput);
            }
            let detail = self
                .repository
                .detail(trace_id)
                .await
                .map_err(map_repository_error)?;
            AdminDebugTraceDetail::from_record(detail)
        })
    }

    fn snapshots<'a>(
        &'a self,
        principal: SessionPrincipal,
        trace_id: i64,
        scope: AdminDebugTraceSnapshotScope,
    ) -> AdminDebugTraceSnapshotsFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            if trace_id < 1 {
                return Err(AdminDebugTraceError::InvalidInput);
            }
            let snapshots = self
                .repository
                .snapshots(trace_id, principal.user_id().get(), scope.into_database())
                .await
                .map_err(map_repository_error)?;
            Ok(AdminDebugTraceSnapshots::from_record(&snapshots))
        })
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminDebugTraceError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminDebugTraceError::Forbidden)
    }
}

fn map_repository_error(error: DebugTraceRepositoryError) -> AdminDebugTraceError {
    match error {
        DebugTraceRepositoryError::InvalidInput => AdminDebugTraceError::InvalidInput,
        DebugTraceRepositoryError::NotFound => AdminDebugTraceError::NotFound,
        DebugTraceRepositoryError::Query
        | DebugTraceRepositoryError::Timeout
        | DebugTraceRepositoryError::Invariant
        | DebugTraceRepositoryError::Decrypt => AdminDebugTraceError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_role_boundaries_fail_closed() {
        assert_eq!(AdminDebugTraceListQuery::default().limit, 50);
        assert_eq!(
            AdminDebugTraceListQuery::new(Some(0), 1, None, None, None),
            Err(AdminDebugTraceError::InvalidInput)
        );
        assert_eq!(
            AdminDebugTraceListQuery::new(
                None,
                af_db::MAX_DEBUG_TRACE_PAGE_SIZE + 1,
                None,
                None,
                None,
            ),
            Err(AdminDebugTraceError::InvalidInput)
        );
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminDebugTraceError::Forbidden)
        );
    }
}
