mod repository;
mod types;

pub use repository::{
    DebugTraceRepository, DebugTraceRepositoryConfigError, DebugTraceRepositoryError,
};
pub use types::{
    DEFAULT_DEBUG_TRACE_BODY_BYTES, DebugTraceAttemptDiagnosticWrite, DebugTraceAttemptOutcome,
    DebugTraceAttemptRecord, DebugTraceAttemptWrite, DebugTraceDetailRecord, DebugTraceFailureKind,
    DebugTraceListQuery, DebugTraceOperation, DebugTraceOutcome, DebugTracePageRecord,
    DebugTraceProtocol, DebugTraceRequestDiagnosticWrite, DebugTraceSettingsRecord,
    DebugTraceSettingsWrite, DebugTraceSummaryRecord, DebugTraceWrite, DebugTraceWriteError,
    MAX_DEBUG_TRACE_BODY_BYTES, MAX_DEBUG_TRACE_PAGE_SIZE, MAX_DEBUG_TRACE_RETENTION_HOURS,
    MAX_DEBUG_TRACE_SAMPLE_PER_MILLION, MIN_DEBUG_TRACE_BODY_BYTES,
};
