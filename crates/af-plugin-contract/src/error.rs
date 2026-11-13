use serde::{Deserialize, Serialize};

/// Stable, bounded error codes. Raw upstream bodies and secrets are not fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorErrorCode {
    InvalidConfiguration,
    Unsupported,
    AuthenticationRequired,
    Forbidden,
    RateLimited,
    TemporarilyUnavailable,
    Conflict,
    UpstreamFailure,
    PartialSuccess,
    OutcomeUnknown,
    InvalidResponse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    NotApplied,
    Applied,
    Unknown,
}

/// Safe connector failure envelope shared by all runtimes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorError {
    pub code: ConnectorErrorCode,
    pub outcome: OperationOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

impl ConnectorError {
    #[must_use]
    pub const fn new(code: ConnectorErrorCode, outcome: OperationOutcome) -> Self {
        Self {
            code,
            outcome,
            retry_after_ms: None,
        }
    }

    #[must_use]
    pub const fn rate_limited(retry_after_ms: u64) -> Self {
        Self {
            code: ConnectorErrorCode::RateLimited,
            outcome: OperationOutcome::NotApplied,
            retry_after_ms: Some(retry_after_ms),
        }
    }

    #[must_use]
    pub const fn requires_reconciliation(self) -> bool {
        matches!(self.outcome, OperationOutcome::Unknown)
    }
}
