use std::{collections::BTreeSet, fmt};

use af_domain::{
    ClientSimulationBodyPatchResult, ClientSimulationBodyProfile, ClientSimulationProfile,
    ClientSimulationResult,
};
use thiserror::Error;
use zeroize::Zeroize as _;

use crate::{DatabaseTimestamp, EncryptedCredentialEnvelope};

pub const MAX_DEBUG_TRACE_SAMPLE_PER_MILLION: i64 = 1_000_000;
pub const MAX_DEBUG_TRACE_RETENTION_HOURS: i32 = 720;
pub const MAX_DEBUG_TRACE_PAGE_SIZE: usize = 100;
pub const DEFAULT_DEBUG_TRACE_BODY_BYTES: i32 = 16_384;
pub const MIN_DEBUG_TRACE_BODY_BYTES: i32 = 1_024;
pub const MAX_DEBUG_TRACE_BODY_BYTES: i32 = 65_536;
const MAX_DEBUG_TRACE_MODEL_BYTES: usize = 255;
const MAX_DEBUG_TRACE_REQUEST_ID_BYTES: usize = 64;
const MAX_DEBUG_TRACE_ATTEMPTS: usize = 64;
const MAX_DEBUG_TRACE_METHOD_BYTES: usize = 16;
const MAX_DEBUG_TRACE_PATH_BYTES: usize = 8_192;
const MAX_DEBUG_TRACE_SNAPSHOT_BYTES: usize = 256 * 1_024;

/// 字段级诊断快照的闭合存储种类；数值同时作为数据库约束和 AAD 的稳定组成部分。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DebugTraceSnapshotKind {
    DownstreamHeaders,
    DownstreamBody,
    AttemptRequestHeaders,
    AttemptRequestBody,
    AttemptResponseHeaders,
    AttemptResponseBody,
}

impl DebugTraceSnapshotKind {
    pub(super) const fn as_i16(self) -> i16 {
        match self {
            Self::DownstreamHeaders => 1,
            Self::DownstreamBody => 2,
            Self::AttemptRequestHeaders => 3,
            Self::AttemptRequestBody => 4,
            Self::AttemptResponseHeaders => 5,
            Self::AttemptResponseBody => 6,
        }
    }

    pub(super) fn from_i16(value: i16) -> Result<Self, DebugTraceWriteError> {
        match value {
            1 => Ok(Self::DownstreamHeaders),
            2 => Ok(Self::DownstreamBody),
            3 => Ok(Self::AttemptRequestHeaders),
            4 => Ok(Self::AttemptRequestBody),
            5 => Ok(Self::AttemptResponseHeaders),
            6 => Ok(Self::AttemptResponseBody),
            _ => Err(DebugTraceWriteError::Invariant),
        }
    }

    #[must_use]
    pub const fn scope(self) -> DebugTraceSnapshotScope {
        match self {
            Self::DownstreamHeaders
            | Self::AttemptRequestHeaders
            | Self::AttemptResponseHeaders => DebugTraceSnapshotScope::Headers,
            Self::DownstreamBody | Self::AttemptRequestBody | Self::AttemptResponseBody => {
                DebugTraceSnapshotScope::Bodies
            }
        }
    }

    pub(super) const fn requires_attempt(self) -> bool {
        !matches!(self, Self::DownstreamHeaders | Self::DownstreamBody)
    }
}

/// 管理端一次敏感读取只能选择 Header 或正文，禁止隐式扩大范围。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugTraceSnapshotScope {
    Headers,
    Bodies,
}

impl DebugTraceSnapshotScope {
    pub(super) const fn as_i16(self) -> i16 {
        match self {
            Self::Headers => 1,
            Self::Bodies => 2,
        }
    }
}

/// 快照加解密 AAD 使用的真实持久化位置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DebugTraceSnapshotContext {
    trace_id: i64,
    attempt_id: Option<i64>,
    kind: DebugTraceSnapshotKind,
}

impl DebugTraceSnapshotContext {
    pub fn new(
        trace_id: i64,
        attempt_id: Option<i64>,
        kind: DebugTraceSnapshotKind,
    ) -> Result<Self, DebugTraceSnapshotCipherError> {
        if trace_id < 1
            || attempt_id.is_some_and(|value| value < 1)
            || attempt_id.is_some() != kind.requires_attempt()
        {
            return Err(DebugTraceSnapshotCipherError);
        }
        Ok(Self {
            trace_id,
            attempt_id,
            kind,
        })
    }

    #[must_use]
    pub const fn trace_id(self) -> i64 {
        self.trace_id
    }

    #[must_use]
    pub const fn attempt_id(self) -> Option<i64> {
        self.attempt_id
    }

    #[must_use]
    pub const fn kind(self) -> DebugTraceSnapshotKind {
        self.kind
    }
}

/// 密码学边界失败；错误不携带明文、密文、密钥标识或 AAD。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("调试追踪快照密码学操作失败")]
pub struct DebugTraceSnapshotCipherError;

/// 已解密的有界 JSON 快照；默认脱敏并在释放时清零。
pub struct DebugTraceSnapshotPlaintext {
    value: String,
}

impl DebugTraceSnapshotPlaintext {
    /// 仅允许密码学实现把已验证的有界 JSON 交回仓储。
    pub fn new(mut value: String) -> Result<Self, DebugTraceSnapshotCipherError> {
        if !valid_snapshot(Some(&value)) {
            value.zeroize();
            return Err(DebugTraceSnapshotCipherError);
        }
        Ok(Self { value })
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.value
    }
}

impl Drop for DebugTraceSnapshotPlaintext {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

impl fmt::Debug for DebugTraceSnapshotPlaintext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DebugTraceSnapshotPlaintext(<已脱敏>)")
    }
}

/// 诊断快照专用密码学端口；实现必须使用位置绑定 AAD。
pub trait DebugTraceSnapshotCipher: Send + Sync {
    fn encrypt(
        &self,
        context: DebugTraceSnapshotContext,
        plaintext: &str,
    ) -> Result<EncryptedCredentialEnvelope, DebugTraceSnapshotCipherError>;

    fn decrypt(
        &self,
        context: DebugTraceSnapshotContext,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DebugTraceSnapshotPlaintext, DebugTraceSnapshotCipherError>;
}

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
    capture_headers: bool,
    capture_bodies: bool,
    max_body_bytes: i32,
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

    pub(super) fn new(
        enabled: bool,
        sample_per_million: i64,
        retention_hours: i32,
        capture_headers: bool,
        capture_bodies: bool,
        max_body_bytes: i32,
        version: i64,
    ) -> Result<Self, DebugTraceWriteError> {
        validate_settings(sample_per_million, retention_hours, max_body_bytes)?;
        if version < 1 {
            return Err(DebugTraceWriteError::Invariant);
        }
        Ok(Self {
            enabled,
            sample_per_million,
            retention_hours,
            capture_headers,
            capture_bodies,
            max_body_bytes,
            version,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DebugTraceSettingsWrite {
    pub(super) enabled: bool,
    pub(super) sample_per_million: i64,
    pub(super) retention_hours: i32,
    pub(super) capture_headers: bool,
    pub(super) capture_bodies: bool,
    pub(super) max_body_bytes: i32,
}

impl DebugTraceSettingsWrite {
    pub fn new(
        enabled: bool,
        sample_per_million: i64,
        retention_hours: i32,
    ) -> Result<Self, DebugTraceWriteError> {
        validate_settings(
            sample_per_million,
            retention_hours,
            DEFAULT_DEBUG_TRACE_BODY_BYTES,
        )?;
        Ok(Self {
            enabled,
            sample_per_million,
            retention_hours,
            capture_headers: false,
            capture_bodies: false,
            max_body_bytes: DEFAULT_DEBUG_TRACE_BODY_BYTES,
        })
    }

    /// 设置 Header 与正文采集边界；正文开关不会隐式开启 Header。
    pub fn with_diagnostics(
        mut self,
        capture_headers: bool,
        capture_bodies: bool,
        max_body_bytes: i32,
    ) -> Result<Self, DebugTraceWriteError> {
        validate_settings(
            self.sample_per_million,
            self.retention_hours,
            max_body_bytes,
        )?;
        self.capture_headers = capture_headers;
        self.capture_bodies = capture_bodies;
        self.max_body_bytes = max_body_bytes;
        Ok(self)
    }
}

/// 下游请求在 HTTP 信任边界生成的永久脱敏快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugTraceRequestDiagnosticWrite {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) headers_json: Option<String>,
    pub(super) body_json: Option<String>,
}

impl DebugTraceRequestDiagnosticWrite {
    pub fn new(
        method: String,
        path: String,
        headers_json: Option<String>,
        body_json: Option<String>,
    ) -> Result<Self, DebugTraceWriteError> {
        if !valid_method(&method)
            || !valid_path(&path)
            || !valid_snapshot(headers_json.as_deref())
            || !valid_snapshot(body_json.as_deref())
        {
            return Err(DebugTraceWriteError::InvalidInput);
        }
        Ok(Self {
            method,
            path,
            headers_json,
            body_json,
        })
    }
}

/// 单个上游 Attempt 的永久脱敏请求与响应快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugTraceAttemptDiagnosticWrite {
    pub(super) request_method: String,
    pub(super) request_url: String,
    pub(super) request_headers_json: Option<String>,
    pub(super) request_body_json: Option<String>,
    pub(super) response_status: Option<i16>,
    pub(super) response_headers_json: Option<String>,
    pub(super) response_body_json: Option<String>,
    pub(super) response_streamed: bool,
}

impl DebugTraceAttemptDiagnosticWrite {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_method: String,
        request_url: String,
        request_headers_json: Option<String>,
        request_body_json: Option<String>,
        response_status: Option<u16>,
        response_headers_json: Option<String>,
        response_body_json: Option<String>,
        response_streamed: bool,
    ) -> Result<Self, DebugTraceWriteError> {
        let response_status = response_status
            .map(i16::try_from)
            .transpose()
            .map_err(|_| DebugTraceWriteError::InvalidInput)?;
        if !valid_method(&request_method)
            || !valid_path(&request_url)
            || response_status.is_some_and(|status| !(100..=599).contains(&status))
            || !valid_snapshot(request_headers_json.as_deref())
            || !valid_snapshot(request_body_json.as_deref())
            || !valid_snapshot(response_headers_json.as_deref())
            || !valid_snapshot(response_body_json.as_deref())
        {
            return Err(DebugTraceWriteError::InvalidInput);
        }
        Ok(Self {
            request_method,
            request_url,
            request_headers_json,
            request_body_json,
            response_status,
            response_headers_json,
            response_body_json,
            response_streamed,
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
    pub(super) client_simulation_profile: Option<ClientSimulationProfile>,
    pub(super) client_simulation_result: Option<ClientSimulationResult>,
    pub(super) client_simulation_body_profile: Option<ClientSimulationBodyProfile>,
    pub(super) client_simulation_body_result: Option<ClientSimulationBodyPatchResult>,
    pub(super) diagnostic: Option<DebugTraceAttemptDiagnosticWrite>,
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
            client_simulation_profile: None,
            client_simulation_result: None,
            client_simulation_body_profile: None,
            client_simulation_body_result: None,
            diagnostic: None,
        })
    }

    /// 附加闭合仿真档案和结果；档案 ID 已包含版本信息。
    #[must_use]
    pub const fn with_client_simulation(
        mut self,
        profile: ClientSimulationProfile,
        result: ClientSimulationResult,
    ) -> Self {
        self.client_simulation_profile = Some(profile);
        self.client_simulation_result = Some(result);
        self
    }

    /// 附加请求级正文补丁档案和结果；不包含日期或正文内容。
    #[must_use]
    pub const fn with_client_simulation_body(
        mut self,
        profile: ClientSimulationBodyProfile,
        result: ClientSimulationBodyPatchResult,
    ) -> Self {
        self.client_simulation_body_profile = Some(profile);
        self.client_simulation_body_result = Some(result);
        self
    }

    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: DebugTraceAttemptDiagnosticWrite) -> Self {
        self.diagnostic = Some(diagnostic);
        self
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
    pub(super) downstream_diagnostic: Option<DebugTraceRequestDiagnosticWrite>,
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
            downstream_diagnostic: None,
        })
    }

    #[must_use]
    pub fn with_downstream_diagnostic(
        mut self,
        diagnostic: DebugTraceRequestDiagnosticWrite,
    ) -> Self {
        self.downstream_diagnostic = Some(diagnostic);
        self
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
    pub(super) request_id: Option<String>,
}

impl DebugTraceListQuery {
    pub fn new(
        before: Option<i64>,
        limit: usize,
        outcome: Option<DebugTraceOutcome>,
        requested_model: Option<String>,
        request_id: Option<String>,
    ) -> Result<Self, DebugTraceWriteError> {
        let requested_model = requested_model.filter(|model| !model.is_empty());
        if before.is_some_and(|value| value < 1)
            || !(1..=MAX_DEBUG_TRACE_PAGE_SIZE).contains(&limit)
            || requested_model
                .as_deref()
                .is_some_and(|model| !valid_model(model))
            || request_id
                .as_deref()
                .is_some_and(|value| !valid_request_id(value))
        {
            return Err(DebugTraceWriteError::InvalidInput);
        }
        Ok(Self {
            before,
            limit,
            outcome,
            requested_model,
            request_id,
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
    pub(super) client_simulation_profile: Option<ClientSimulationProfile>,
    pub(super) client_simulation_result: Option<ClientSimulationResult>,
    pub(super) client_simulation_body_profile: Option<ClientSimulationBodyProfile>,
    pub(super) client_simulation_body_result: Option<ClientSimulationBodyPatchResult>,
    pub(super) request_method: Option<String>,
    pub(super) request_url: Option<String>,
    pub(super) response_status: Option<i16>,
    pub(super) response_streamed: bool,
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
    #[must_use]
    pub const fn client_simulation_profile(&self) -> Option<ClientSimulationProfile> {
        self.client_simulation_profile
    }
    #[must_use]
    pub const fn client_simulation_result(&self) -> Option<ClientSimulationResult> {
        self.client_simulation_result
    }
    #[must_use]
    pub const fn client_simulation_body_profile(&self) -> Option<ClientSimulationBodyProfile> {
        self.client_simulation_body_profile
    }
    #[must_use]
    pub const fn client_simulation_body_result(&self) -> Option<ClientSimulationBodyPatchResult> {
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
    pub(super) downstream_method: Option<String>,
    pub(super) downstream_path: Option<String>,
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
    pub fn downstream_method(&self) -> Option<&str> {
        self.downstream_method.as_deref()
    }
    #[must_use]
    pub fn downstream_path(&self) -> Option<&str> {
        self.downstream_path.as_deref()
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

/// 单个 Attempt 在指定敏感读取范围内的解密快照。
#[derive(Debug)]
pub struct DebugTraceAttemptSnapshotRecord {
    candidate_index: i16,
    request: Option<DebugTraceSnapshotPlaintext>,
    response: Option<DebugTraceSnapshotPlaintext>,
}

impl DebugTraceAttemptSnapshotRecord {
    pub(super) fn new(
        candidate_index: i16,
        request: Option<DebugTraceSnapshotPlaintext>,
        response: Option<DebugTraceSnapshotPlaintext>,
    ) -> Self {
        Self {
            candidate_index,
            request,
            response,
        }
    }

    #[must_use]
    pub const fn candidate_index(&self) -> i16 {
        self.candidate_index
    }

    #[must_use]
    pub fn request_json(&self) -> Option<&str> {
        self.request
            .as_ref()
            .map(DebugTraceSnapshotPlaintext::expose)
    }

    #[must_use]
    pub fn response_json(&self) -> Option<&str> {
        self.response
            .as_ref()
            .map(DebugTraceSnapshotPlaintext::expose)
    }
}

/// 一次已审计的 Header 或正文读取结果。
#[derive(Debug)]
pub struct DebugTraceSnapshotRecord {
    scope: DebugTraceSnapshotScope,
    downstream: Option<DebugTraceSnapshotPlaintext>,
    attempts: Vec<DebugTraceAttemptSnapshotRecord>,
}

impl DebugTraceSnapshotRecord {
    pub(super) fn new(
        scope: DebugTraceSnapshotScope,
        downstream: Option<DebugTraceSnapshotPlaintext>,
        attempts: Vec<DebugTraceAttemptSnapshotRecord>,
    ) -> Self {
        Self {
            scope,
            downstream,
            attempts,
        }
    }

    #[must_use]
    pub const fn scope(&self) -> DebugTraceSnapshotScope {
        self.scope
    }

    #[must_use]
    pub fn downstream_json(&self) -> Option<&str> {
        self.downstream
            .as_ref()
            .map(DebugTraceSnapshotPlaintext::expose)
    }

    #[must_use]
    pub fn attempts(&self) -> &[DebugTraceAttemptSnapshotRecord] {
        &self.attempts
    }
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
    max_body_bytes: i32,
) -> Result<(), DebugTraceWriteError> {
    if !(0..=MAX_DEBUG_TRACE_SAMPLE_PER_MILLION).contains(&sample_per_million)
        || !(1..=MAX_DEBUG_TRACE_RETENTION_HOURS).contains(&retention_hours)
        || !(MIN_DEBUG_TRACE_BODY_BYTES..=MAX_DEBUG_TRACE_BODY_BYTES).contains(&max_body_bytes)
    {
        return Err(DebugTraceWriteError::InvalidInput);
    }
    Ok(())
}

pub(super) fn valid_method(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DEBUG_TRACE_METHOD_BYTES
        && value.trim() == value
        && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

pub(super) fn valid_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DEBUG_TRACE_PATH_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_snapshot(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= MAX_DEBUG_TRACE_SNAPSHOT_BYTES
            && serde_json::from_str::<serde_json::Value>(value).is_ok()
    })
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
