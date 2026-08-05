use std::{collections::BTreeSet, fmt};

use thiserror::Error;

use crate::DatabaseTimestamp;

pub const MAX_DEBUG_TRACE_SAMPLE_PER_MILLION: i64 = 1_000_000;
pub const MAX_DEBUG_TRACE_RETENTION_HOURS: i32 = 720;
pub const MAX_DEBUG_TRACE_PAGE_SIZE: usize = 100;
const MAX_DEBUG_TRACE_MODEL_BYTES: usize = 255;
const MAX_DEBUG_TRACE_REQUEST_ID_BYTES: usize = 64;
const MAX_DEBUG_TRACE_ATTEMPTS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugTraceProtocol {
    OpenAiChat,
    OpenAiResponses,
    Anthropic,
    Gemini,
}

impl DebugTraceProtocol {
    pub(super) const fn as_i16(self) -> i16 {
        match self {
            Self::OpenAiChat => 1,
            Self::OpenAiResponses => 2,
            Self::Anthropic => 3,
            Self::Gemini => 4,
        }
    }

    pub(super) fn from_i16(value: i16) -> Result<Self, DebugTraceWriteError> {
        match value {
            1 => Ok(Self::OpenAiChat),
            2 => Ok(Self::OpenAiResponses),
            3 => Ok(Self::Anthropic),
            4 => Ok(Self::Gemini),
            _ => Err(DebugTraceWriteError::Invariant),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugTraceOperation {
    Chat,
    Responses,
}

impl DebugTraceOperation {
    pub(super) const fn as_i16(self) -> i16 {
        match self {
            Self::Chat => 1,
            Self::Responses => 2,
        }
    }

    pub(super) fn from_i16(value: i16) -> Result<Self, DebugTraceWriteError> {
        match value {
            1 => Ok(Self::Chat),
            2 => Ok(Self::Responses),
            _ => Err(DebugTraceWriteError::Invariant),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugTraceOutcome {
    Succeeded,
    Failed,
}

impl DebugTraceOutcome {
    pub(super) const fn as_i16(self) -> i16 {
        match self {
            Self::Succeeded => 1,
            Self::Failed => 2,
        }
    }

    pub(super) fn from_i16(value: i16) -> Result<Self, DebugTraceWriteError> {
        match value {
            1 => Ok(Self::Succeeded),
            2 => Ok(Self::Failed),
            _ => Err(DebugTraceWriteError::Invariant),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugTraceAttemptOutcome {
    Succeeded,
    Failed,
}

impl DebugTraceAttemptOutcome {
    pub(super) const fn as_i16(self) -> i16 {
        match self {
            Self::Succeeded => 1,
            Self::Failed => 2,
        }
    }

    pub(super) fn from_i16(value: i16) -> Result<Self, DebugTraceWriteError> {
        match value {
            1 => Ok(Self::Succeeded),
            2 => Ok(Self::Failed),
            _ => Err(DebugTraceWriteError::Invariant),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugTraceFailureKind {
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
    Network,
}

impl DebugTraceFailureKind {
    pub(super) const fn as_i16(self) -> i16 {
        match self {
            Self::AuthExpired => 1,
            Self::AuthRevoked => 2,
            Self::AccountDisabled => 3,
            Self::RateLimited => 4,
            Self::Overloaded => 5,
            Self::QuotaExhausted => 6,
            Self::ModelUnsupported => 7,
            Self::ProtocolError => 8,
            Self::ServerError => 9,
            Self::BadRequest => 10,
            Self::Network => 11,
        }
    }

    pub(super) fn from_i16(value: i16) -> Result<Self, DebugTraceWriteError> {
        match value {
            1 => Ok(Self::AuthExpired),
            2 => Ok(Self::AuthRevoked),
            3 => Ok(Self::AccountDisabled),
            4 => Ok(Self::RateLimited),
            5 => Ok(Self::Overloaded),
            6 => Ok(Self::QuotaExhausted),
            7 => Ok(Self::ModelUnsupported),
            8 => Ok(Self::ProtocolError),
            9 => Ok(Self::ServerError),
            10 => Ok(Self::BadRequest),
            11 => Ok(Self::Network),
            _ => Err(DebugTraceWriteError::Invariant),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DebugTraceSettingsRecord {
    enabled: bool,
    sample_per_million: i64,
    retention_hours: i32,
    version: i64,
}

impl DebugTraceSettingsRecord {
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
    pub const fn version(self) -> i64 {
        self.version
    }

    pub(super) fn new(
        enabled: bool,
        sample_per_million: i64,
        retention_hours: i32,
        version: i64,
    ) -> Result<Self, DebugTraceWriteError> {
        validate_settings(sample_per_million, retention_hours)?;
        if version < 1 {
            return Err(DebugTraceWriteError::Invariant);
        }
        Ok(Self {
            enabled,
            sample_per_million,
            retention_hours,
            version,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DebugTraceSettingsWrite {
    pub(super) enabled: bool,
    pub(super) sample_per_million: i64,
    pub(super) retention_hours: i32,
}

impl DebugTraceSettingsWrite {
    pub fn new(
        enabled: bool,
        sample_per_million: i64,
        retention_hours: i32,
    ) -> Result<Self, DebugTraceWriteError> {
        validate_settings(sample_per_million, retention_hours)?;
        Ok(Self {
            enabled,
            sample_per_million,
            retention_hours,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugTraceAttemptWrite {
    pub(super) candidate_index: i16,
    pub(super) channel_id: i64,
    pub(super) credential_id: i64,
    pub(super) outcome: DebugTraceAttemptOutcome,
    pub(super) failure_kind: Option<DebugTraceFailureKind>,
    pub(super) upstream_status: Option<i16>,
    pub(super) retry_decision: bool,
    pub(super) elapsed_ms: i64,
}

impl DebugTraceAttemptWrite {
    pub fn succeeded(
        candidate_index: usize,
        channel_id: i64,
        credential_id: i64,
        elapsed_ms: i64,
    ) -> Result<Self, DebugTraceWriteError> {
        Self::new(
            candidate_index,
            channel_id,
            credential_id,
            DebugTraceAttemptOutcome::Succeeded,
            None,
            None,
            false,
            elapsed_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn failed(
        candidate_index: usize,
        channel_id: i64,
        credential_id: i64,
        failure_kind: DebugTraceFailureKind,
        upstream_status: Option<u16>,
        retry_decision: bool,
        elapsed_ms: i64,
    ) -> Result<Self, DebugTraceWriteError> {
        Self::new(
            candidate_index,
            channel_id,
            credential_id,
            DebugTraceAttemptOutcome::Failed,
            Some(failure_kind),
            upstream_status
                .map(i16::try_from)
                .transpose()
                .map_err(|_| DebugTraceWriteError::InvalidInput)?,
            retry_decision,
            elapsed_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        candidate_index: usize,
        channel_id: i64,
        credential_id: i64,
        outcome: DebugTraceAttemptOutcome,
        failure_kind: Option<DebugTraceFailureKind>,
        upstream_status: Option<i16>,
        retry_decision: bool,
        elapsed_ms: i64,
    ) -> Result<Self, DebugTraceWriteError> {
        let candidate_index =
            i16::try_from(candidate_index).map_err(|_| DebugTraceWriteError::InvalidInput)?;
        let shape_valid = match outcome {
            DebugTraceAttemptOutcome::Succeeded => {
                failure_kind.is_none() && upstream_status.is_none() && !retry_decision
            }
            DebugTraceAttemptOutcome::Failed => failure_kind.is_some(),
        };
        if !(0..64).contains(&candidate_index)
            || channel_id < 1
            || credential_id < 1
            || elapsed_ms < 0
            || !shape_valid
            || upstream_status.is_some_and(|status| !(500..=599).contains(&status))
        {
            return Err(DebugTraceWriteError::InvalidInput);
        }
        Ok(Self {
            candidate_index,
            channel_id,
            credential_id,
            outcome,
            failure_kind,
            upstream_status,
            retry_decision,
            elapsed_ms,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugTraceWrite {
    pub(super) request_id: String,
    pub(super) user_id: i64,
    pub(super) token_id: i64,
    pub(super) group_id: i64,
    pub(super) requested_model: String,
    pub(super) downstream_protocol: DebugTraceProtocol,
    pub(super) upstream_protocol: DebugTraceProtocol,
    pub(super) operation: DebugTraceOperation,
    pub(super) outcome: DebugTraceOutcome,
    pub(super) routing_elapsed_ms: i64,
    pub(super) attempts: Vec<DebugTraceAttemptWrite>,
}

impl DebugTraceWrite {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: String,
        user_id: i64,
        token_id: i64,
        group_id: i64,
        requested_model: String,
        downstream_protocol: DebugTraceProtocol,
        upstream_protocol: DebugTraceProtocol,
        operation: DebugTraceOperation,
        outcome: DebugTraceOutcome,
        routing_elapsed_ms: i64,
        attempts: Vec<DebugTraceAttemptWrite>,
    ) -> Result<Self, DebugTraceWriteError> {
        if !valid_request_id(&request_id)
            || user_id < 1
            || token_id < 1
            || group_id < 1
            || !valid_model(&requested_model)
            || routing_elapsed_ms < 0
            || attempts.len() > MAX_DEBUG_TRACE_ATTEMPTS
        {
            return Err(DebugTraceWriteError::InvalidInput);
        }
        let mut indices = BTreeSet::new();
        let successful = attempts
            .iter()
            .filter(|attempt| attempt.outcome == DebugTraceAttemptOutcome::Succeeded)
            .count();
        if attempts
            .iter()
            .any(|attempt| !indices.insert(attempt.candidate_index))
            || match outcome {
                DebugTraceOutcome::Succeeded => successful != 1,
                DebugTraceOutcome::Failed => successful != 0,
            }
        {
            return Err(DebugTraceWriteError::InvalidInput);
        }
        Ok(Self {
            request_id,
            user_id,
            token_id,
            group_id,
            requested_model,
            downstream_protocol,
            upstream_protocol,
            operation,
            outcome,
            routing_elapsed_ms,
            attempts,
        })
    }

    pub(super) fn selected_target(&self) -> Option<(i64, i64)> {
        self.attempts
            .iter()
            .find(|attempt| attempt.outcome == DebugTraceAttemptOutcome::Succeeded)
            .map(|attempt| (attempt.channel_id, attempt.credential_id))
    }
}

impl fmt::Display for DebugTraceWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DebugTraceWrite(<已脱敏>)")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugTraceListQuery {
    pub(super) before: Option<i64>,
    pub(super) limit: usize,
    pub(super) outcome: Option<DebugTraceOutcome>,
    pub(super) requested_model: Option<String>,
}

impl DebugTraceListQuery {
    pub fn new(
        before: Option<i64>,
        limit: usize,
        outcome: Option<DebugTraceOutcome>,
        requested_model: Option<String>,
    ) -> Result<Self, DebugTraceWriteError> {
        let requested_model = requested_model.filter(|model| !model.is_empty());
        if before.is_some_and(|value| value < 1)
            || !(1..=MAX_DEBUG_TRACE_PAGE_SIZE).contains(&limit)
            || requested_model
                .as_deref()
                .is_some_and(|model| !valid_model(model))
        {
            return Err(DebugTraceWriteError::InvalidInput);
        }
        Ok(Self {
            before,
            limit,
            outcome,
            requested_model,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugTraceAttemptRecord {
    pub(super) candidate_index: i16,
    pub(super) channel_id: i64,
    pub(super) credential_id: i64,
    pub(super) outcome: DebugTraceAttemptOutcome,
    pub(super) failure_kind: Option<DebugTraceFailureKind>,
    pub(super) upstream_status: Option<i16>,
    pub(super) retry_decision: bool,
    pub(super) elapsed_ms: i64,
}

impl DebugTraceAttemptRecord {
    #[must_use]
    pub const fn candidate_index(&self) -> i16 {
        self.candidate_index
    }
    #[must_use]
    pub const fn channel_id(&self) -> i64 {
        self.channel_id
    }
    #[must_use]
    pub const fn credential_id(&self) -> i64 {
        self.credential_id
    }
    #[must_use]
    pub const fn outcome(&self) -> DebugTraceAttemptOutcome {
        self.outcome
    }
    #[must_use]
    pub const fn failure_kind(&self) -> Option<DebugTraceFailureKind> {
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugTraceSummaryRecord {
    pub(super) id: i64,
    pub(super) request_id: String,
    pub(super) user_id: i64,
    pub(super) token_id: i64,
    pub(super) group_id: i64,
    pub(super) requested_model: String,
    pub(super) downstream_protocol: DebugTraceProtocol,
    pub(super) upstream_protocol: DebugTraceProtocol,
    pub(super) operation: DebugTraceOperation,
    pub(super) outcome: DebugTraceOutcome,
    pub(super) selected_channel_id: Option<i64>,
    pub(super) selected_credential_id: Option<i64>,
    pub(super) routing_elapsed_ms: i64,
    pub(super) attempt_count: i32,
    pub(super) created_at: DatabaseTimestamp,
}

impl DebugTraceSummaryRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    #[must_use]
    pub const fn user_id(&self) -> i64 {
        self.user_id
    }
    #[must_use]
    pub const fn token_id(&self) -> i64 {
        self.token_id
    }
    #[must_use]
    pub const fn group_id(&self) -> i64 {
        self.group_id
    }
    #[must_use]
    pub fn requested_model(&self) -> &str {
        &self.requested_model
    }
    #[must_use]
    pub const fn downstream_protocol(&self) -> DebugTraceProtocol {
        self.downstream_protocol
    }
    #[must_use]
    pub const fn upstream_protocol(&self) -> DebugTraceProtocol {
        self.upstream_protocol
    }
    #[must_use]
    pub const fn operation(&self) -> DebugTraceOperation {
        self.operation
    }
    #[must_use]
    pub const fn outcome(&self) -> DebugTraceOutcome {
        self.outcome
    }
    #[must_use]
    pub const fn selected_channel_id(&self) -> Option<i64> {
        self.selected_channel_id
    }
    #[must_use]
    pub const fn selected_credential_id(&self) -> Option<i64> {
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
    pub const fn created_at(&self) -> DatabaseTimestamp {
        self.created_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugTraceDetailRecord {
    summary: DebugTraceSummaryRecord,
    attempts: Vec<DebugTraceAttemptRecord>,
}

impl DebugTraceDetailRecord {
    pub(super) fn new(
        summary: DebugTraceSummaryRecord,
        attempts: Vec<DebugTraceAttemptRecord>,
    ) -> Self {
        Self { summary, attempts }
    }
    #[must_use]
    pub const fn summary(&self) -> &DebugTraceSummaryRecord {
        &self.summary
    }
    #[must_use]
    pub fn attempts(&self) -> &[DebugTraceAttemptRecord] {
        &self.attempts
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugTracePageRecord {
    traces: Vec<DebugTraceSummaryRecord>,
    next_cursor: Option<i64>,
}

impl DebugTracePageRecord {
    pub(super) fn new(traces: Vec<DebugTraceSummaryRecord>, next_cursor: Option<i64>) -> Self {
        Self {
            traces,
            next_cursor,
        }
    }
    #[must_use]
    pub fn traces(&self) -> &[DebugTraceSummaryRecord] {
        &self.traces
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DebugTraceWriteError {
    #[error("调试追踪输入无效")]
    InvalidInput,
    #[error("调试追踪持久化状态损坏")]
    Invariant,
}

fn validate_settings(
    sample_per_million: i64,
    retention_hours: i32,
) -> Result<(), DebugTraceWriteError> {
    if !(0..=MAX_DEBUG_TRACE_SAMPLE_PER_MILLION).contains(&sample_per_million)
        || !(1..=MAX_DEBUG_TRACE_RETENTION_HOURS).contains(&retention_hours)
    {
        return Err(DebugTraceWriteError::InvalidInput);
    }
    Ok(())
}

pub(super) fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DEBUG_TRACE_REQUEST_ID_BYTES
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
}

pub(super) fn valid_model(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DEBUG_TRACE_MODEL_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
