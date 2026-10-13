//! Terminal request facts for identities whose storage is owned by a distribution.

use std::{future::Future, pin::Pin, time::Duration};

use af_domain::{
    ChannelId, GroupId, Operation, OrganizationServiceAccountRuntimeIdentity, Protocol,
};

use crate::{RequestFailureKind, RequestOutcomeRepositoryError, RequestOutcomeWriteError};

/// Contains authenticated identifiers and closed error categories, never payloads or secrets.
#[derive(Clone)]
pub struct ServiceAccountRequestOutcome {
    identity: OrganizationServiceAccountRuntimeIdentity,
    group_id: GroupId,
    request_id: String,
    protocol: Protocol,
    operation: Operation,
    model: String,
    channel_id: Option<ChannelId>,
    failure_kind: Option<RequestFailureKind>,
    duration_ms: i64,
}

impl ServiceAccountRequestOutcome {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity: OrganizationServiceAccountRuntimeIdentity,
        group_id: GroupId,
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        channel_id: Option<ChannelId>,
        failure_kind: Option<RequestFailureKind>,
        duration: Duration,
    ) -> Result<Self, RequestOutcomeWriteError> {
        if !valid_text(request_id, 128) {
            return Err(RequestOutcomeWriteError::InvalidRequestId);
        }
        if !valid_text(model, 255) {
            return Err(RequestOutcomeWriteError::InvalidModel);
        }
        let duration_ms = i64::try_from(duration.as_millis())
            .map_err(|_| RequestOutcomeWriteError::InvalidDuration)?;
        Ok(Self {
            identity,
            group_id,
            request_id: request_id.to_owned(),
            protocol,
            operation,
            model: model.to_owned(),
            channel_id,
            failure_kind,
            duration_ms,
        })
    }

    pub const fn identity(&self) -> OrganizationServiceAccountRuntimeIdentity {
        self.identity
    }
    pub const fn group_id(&self) -> GroupId {
        self.group_id
    }
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }
    pub const fn operation(&self) -> Operation {
        self.operation
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub const fn channel_id(&self) -> Option<ChannelId> {
        self.channel_id
    }
    pub const fn failure_kind(&self) -> Option<RequestFailureKind> {
        self.failure_kind
    }
    pub const fn duration_ms(&self) -> i64 {
        self.duration_ms
    }
}

/// Implementations must append idempotently and reject conflicting replays.
/// A storage failure must not change the original model response or settlement.
pub trait ServiceAccountOutcomeSink: Send + Sync + 'static {
    fn persist<'a>(&'a self, fact: ServiceAccountRequestOutcome)
    -> ServiceAccountOutcomeFuture<'a>;
}

pub type ServiceAccountOutcomeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), RequestOutcomeRepositoryError>> + Send + 'a>>;

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
