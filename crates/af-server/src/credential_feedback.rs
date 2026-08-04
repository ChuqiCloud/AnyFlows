use af_db::{
    CredentialStateChange, CredentialStateEvent, CredentialStateRepository,
    CredentialStateRepositoryError,
};
use af_domain::{AfError, ChannelId, CredentialId, CredentialKind, UpstreamError};
use af_relay::RelayAttemptReport;
use af_scheduler::IndexedWeightedScheduler;

/// 与 Relay 候选索引一一对应的脱敏凭据身份。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CredentialAttemptTarget {
    channel_id: ChannelId,
    credential_id: CredentialId,
    credential_kind: CredentialKind,
}

impl CredentialAttemptTarget {
    /// 校验凭据标识后构造候选反馈目标。
    pub(crate) fn new(
        channel_id: ChannelId,
        credential_id: i64,
        credential_kind: CredentialKind,
    ) -> Result<Self, AfError> {
        Ok(Self {
            channel_id,
            credential_id: CredentialId::new(credential_id).map_err(|_| AfError::Internal)?,
            credential_kind,
        })
    }

    /// 返回与 Relay 候选对应的渠道标识，供成功粘性绑定使用。
    pub(crate) const fn channel_id(self) -> ChannelId {
        self.channel_id
    }
}

/// 最佳努力持久化凭据反馈；失败不得覆盖已经确定的业务结果。
pub(crate) async fn persist_credential_feedback(
    repository: &CredentialStateRepository,
    scheduler: &IndexedWeightedScheduler,
    targets: &[CredentialAttemptTarget],
    report: &RelayAttemptReport,
) {
    let Some((events, refresh_required)) = feedback_events(targets, report) else {
        tracing::error!(
            target: "af_server::credential_feedback",
            error_kind = "credential_feedback_invariant",
            "凭据反馈候选索引无效"
        );
        return;
    };
    if events.is_empty() {
        return;
    }
    if let Err(error) = repository.apply(&events).await {
        record_repository_error(error);
        return;
    }
    if refresh_required && scheduler.refresh().await.is_err() {
        tracing::error!(
            target: "af_server::credential_feedback",
            error_kind = "credential_feedback_refresh",
            "凭据反馈写回后刷新运行时快照失败"
        );
    }
}

fn feedback_events(
    targets: &[CredentialAttemptTarget],
    report: &RelayAttemptReport,
) -> Option<(Vec<CredentialStateEvent>, bool)> {
    let mut events = Vec::with_capacity(
        report
            .failures()
            .len()
            .checked_add(usize::from(report.successful_candidate_index().is_some()))?,
    );
    let mut refresh_required = false;
    for failure in report.failures() {
        let Some(change) = change_for_error(failure.error()) else {
            continue;
        };
        events.push(state_event(
            *targets.get(failure.candidate_index())?,
            change,
        )?);
        refresh_required = true;
    }
    if let Some(index) = report.successful_candidate_index() {
        events.push(state_event(
            *targets.get(index)?,
            CredentialStateChange::Succeeded,
        )?);
    }
    Some((events, refresh_required))
}

fn state_event(
    target: CredentialAttemptTarget,
    change: CredentialStateChange,
) -> Option<CredentialStateEvent> {
    CredentialStateEvent::new(
        target.channel_id,
        target.credential_id,
        target.credential_kind,
        change,
    )
    .ok()
}

const fn change_for_error(error: UpstreamError) -> Option<CredentialStateChange> {
    match error {
        UpstreamError::AuthExpired => Some(CredentialStateChange::AuthExpired),
        UpstreamError::AuthRevoked => Some(CredentialStateChange::AuthRevoked),
        UpstreamError::RateLimited => Some(CredentialStateChange::RateLimited),
        UpstreamError::QuotaExhausted => Some(CredentialStateChange::QuotaExhausted),
        UpstreamError::Overloaded => Some(CredentialStateChange::Overloaded),
        UpstreamError::BadRequest
        | UpstreamError::ModelUnsupported
        | UpstreamError::ProtocolError
        | UpstreamError::Network
        | UpstreamError::ServerError { .. } => None,
    }
}

fn record_repository_error(error: CredentialStateRepositoryError) {
    let error_kind = match error {
        CredentialStateRepositoryError::InvalidConfiguration => {
            "credential_feedback_invalid_configuration"
        }
        CredentialStateRepositoryError::InvalidBatch => "credential_feedback_invalid_batch",
        CredentialStateRepositoryError::InvalidEvent => "credential_feedback_invalid_event",
        CredentialStateRepositoryError::Query => "credential_feedback_query",
        CredentialStateRepositoryError::Timeout => "credential_feedback_timeout",
        CredentialStateRepositoryError::Invariant => "credential_feedback_invariant",
        _ => "credential_feedback_unknown",
    };
    tracing::error!(
        target: "af_server::credential_feedback",
        error_kind,
        "凭据反馈持久化失败"
    );
}

#[cfg(test)]
mod tests {
    use af_domain::UpstreamServerStatus;

    use super::*;

    #[test]
    fn only_credential_health_errors_produce_state_changes() {
        let cases = [
            (
                UpstreamError::AuthExpired,
                Some(CredentialStateChange::AuthExpired),
            ),
            (
                UpstreamError::AuthRevoked,
                Some(CredentialStateChange::AuthRevoked),
            ),
            (
                UpstreamError::RateLimited,
                Some(CredentialStateChange::RateLimited),
            ),
            (
                UpstreamError::QuotaExhausted,
                Some(CredentialStateChange::QuotaExhausted),
            ),
            (
                UpstreamError::Overloaded,
                Some(CredentialStateChange::Overloaded),
            ),
            (UpstreamError::BadRequest, None),
            (UpstreamError::Network, None),
            (
                UpstreamError::ServerError {
                    status: UpstreamServerStatus::new(503).unwrap(),
                },
                None,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(change_for_error(error), expected);
        }
    }
}
