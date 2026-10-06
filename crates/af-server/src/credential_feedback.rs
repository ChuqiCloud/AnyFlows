use af_db::{
    CredentialStateChange, CredentialStateEvent, CredentialStateRepository,
    CredentialStateRepositoryError,
};
use af_domain::{AfError, ChannelId, CredentialId, CredentialKind, UpstreamError};
use af_relay::RelayAttemptReport;
use af_scheduler::{
    CredentialFailureContext, CredentialFailureDisposition, IndexedWeightedScheduler,
    PermanentCredentialFailure, TemporaryCredentialFailure, credential_failure_disposition,
};

/// 与 Relay 候选索引一一对应的脱敏凭据身份。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CredentialAttemptTarget {
    channel_id: ChannelId,
    routing_credential_id: CredentialId,
    secret_owner_id: CredentialId,
    shared_health_id: CredentialId,
    credential_kind: CredentialKind,
    oauth_has_refresh_token: bool,
    pool_mode: bool,
}

impl CredentialAttemptTarget {
    /// 校验凭据标识并收敛非敏感 OAuth 恢复能力后构造候选反馈目标。
    #[cfg(test)]
    pub(crate) fn new(
        channel_id: ChannelId,
        credential_id: i64,
        credential_kind: CredentialKind,
        oauth_has_refresh_token: bool,
        pool_mode: bool,
    ) -> Result<Self, AfError> {
        let credential_id = CredentialId::new(credential_id).map_err(|_| AfError::Internal)?;
        Self::with_runtime_identity(
            channel_id,
            credential_id,
            credential_id,
            credential_id,
            credential_kind,
            oauth_has_refresh_token,
            pool_mode,
        )
    }

    /// 使用运行时拆分后的路由、密钥和共享健康身份构造反馈目标。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与脱敏运行时反馈身份一一对应"
    )]
    pub(crate) fn with_runtime_identity(
        channel_id: ChannelId,
        routing_credential_id: CredentialId,
        secret_owner_id: CredentialId,
        shared_health_id: CredentialId,
        credential_kind: CredentialKind,
        oauth_has_refresh_token: bool,
        pool_mode: bool,
    ) -> Result<Self, AfError> {
        let root_identity =
            routing_credential_id == secret_owner_id && secret_owner_id == shared_health_id;
        let shadow_identity = credential_kind == CredentialKind::Oauth
            && routing_credential_id != secret_owner_id
            && secret_owner_id == shared_health_id;
        if !root_identity && !shadow_identity {
            return Err(AfError::Internal);
        }
        Ok(Self {
            channel_id,
            routing_credential_id,
            secret_owner_id,
            shared_health_id,
            credential_kind,
            oauth_has_refresh_token: matches!(credential_kind, CredentialKind::Oauth)
                && oauth_has_refresh_token,
            pool_mode,
        })
    }

    /// 返回与 Relay 候选对应的渠道标识，供成功粘性绑定使用。
    pub(crate) const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回与 Relay 候选对应的凭据标识，供健康反馈与调权使用。
    pub(crate) const fn credential_id(&self) -> CredentialId {
        self.routing_credential_id
    }

    /// 返回持久化认证故障和解密能力所属的母凭据。
    #[cfg(test)]
    pub(crate) const fn secret_owner_id(&self) -> CredentialId {
        self.secret_owner_id
    }

    /// 返回 Redis 共享认证健康所属的母凭据。
    pub(crate) const fn shared_health_id(&self) -> CredentialId {
        self.shared_health_id
    }

    /// 返回渠道是否把渠道级健康交由外部账号池管理。
    pub(crate) const fn pool_mode(&self) -> bool {
        self.pool_mode
    }

    /// 返回凭据状态策略所需的非敏感恢复上下文。
    pub(crate) const fn failure_context(&self) -> CredentialFailureContext {
        CredentialFailureContext::new(self.credential_kind, self.oauth_has_refresh_token)
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
        let target = *targets.get(failure.candidate_index())?;
        let Some(change) = change_for_error(target, failure.error()) else {
            continue;
        };
        events.push(state_event(target, state_owner(target, change), change)?);
        refresh_required = true;
    }
    if let Some(index) = report.successful_candidate_index() {
        let target = *targets.get(index)?;
        events.push(state_event(
            target,
            target.routing_credential_id,
            CredentialStateChange::Succeeded,
        )?);
        if target.secret_owner_id != target.routing_credential_id {
            events.push(state_event(
                target,
                target.secret_owner_id,
                CredentialStateChange::AuthenticationSucceeded,
            )?);
            refresh_required = true;
        }
    }
    Some((events, refresh_required))
}

fn state_event(
    target: CredentialAttemptTarget,
    credential_id: CredentialId,
    change: CredentialStateChange,
) -> Option<CredentialStateEvent> {
    CredentialStateEvent::new(
        target.channel_id,
        credential_id,
        target.credential_kind,
        change,
    )
    .ok()
}

const fn state_owner(
    target: CredentialAttemptTarget,
    change: CredentialStateChange,
) -> CredentialId {
    match change {
        CredentialStateChange::AuthExpired
        | CredentialStateChange::MissingRefreshToken
        | CredentialStateChange::AuthRevoked
        | CredentialStateChange::AccountDisabled
        | CredentialStateChange::AuthenticationSucceeded => target.secret_owner_id,
        CredentialStateChange::Succeeded
        | CredentialStateChange::RateLimited { .. }
        | CredentialStateChange::QuotaExhausted
        | CredentialStateChange::Overloaded { .. } => target.routing_credential_id,
    }
}

const fn change_for_error(
    target: CredentialAttemptTarget,
    error: UpstreamError,
) -> Option<CredentialStateChange> {
    match credential_failure_disposition(error, target.failure_context()) {
        CredentialFailureDisposition::NoChange => None,
        CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::AuthExpired) => {
            Some(CredentialStateChange::AuthExpired)
        }
        CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::RateLimited {
            scope: af_domain::RateLimitScope::Model,
            ..
        }) => None,
        CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::RateLimited {
            retry_after,
            ..
        }) => Some(CredentialStateChange::RateLimited { retry_after }),
        CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::QuotaExhausted) => {
            Some(CredentialStateChange::QuotaExhausted)
        }
        CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::Overloaded {
            retry_after,
        }) => Some(CredentialStateChange::Overloaded { retry_after }),
        CredentialFailureDisposition::Permanent(
            PermanentCredentialFailure::MissingRefreshToken,
        ) => Some(CredentialStateChange::MissingRefreshToken),
        CredentialFailureDisposition::Permanent(PermanentCredentialFailure::AuthRevoked) => {
            Some(CredentialStateChange::AuthRevoked)
        }
        CredentialFailureDisposition::Permanent(PermanentCredentialFailure::AccountDisabled) => {
            Some(CredentialStateChange::AccountDisabled)
        }
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
    use af_domain::{NetworkFailureKind, RateLimitScope, UpstreamRetryAfter, UpstreamServerStatus};

    use super::*;

    #[test]
    fn only_credential_health_errors_produce_state_changes() {
        let target = target(CredentialKind::ApiKey, false);
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
                UpstreamError::AccountDisabled,
                Some(CredentialStateChange::AccountDisabled),
            ),
            (
                UpstreamError::rate_limited(RateLimitScope::Window),
                Some(CredentialStateChange::RateLimited { retry_after: None }),
            ),
            (UpstreamError::rate_limited(RateLimitScope::Model), None),
            (
                UpstreamError::rate_limited_after(
                    RateLimitScope::Credential,
                    UpstreamRetryAfter::from_seconds(90).unwrap(),
                ),
                Some(CredentialStateChange::RateLimited {
                    retry_after: UpstreamRetryAfter::from_seconds(90),
                }),
            ),
            (
                UpstreamError::QuotaExhausted,
                Some(CredentialStateChange::QuotaExhausted),
            ),
            (
                UpstreamError::overloaded(),
                Some(CredentialStateChange::Overloaded { retry_after: None }),
            ),
            (UpstreamError::BadRequest, None),
            (UpstreamError::network(NetworkFailureKind::Connect), None),
            (
                UpstreamError::ServerError {
                    status: UpstreamServerStatus::new(503).unwrap(),
                },
                None,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(change_for_error(target, error), expected);
        }
    }

    #[test]
    fn oauth_auth_expiry_uses_only_the_refresh_capability_bit() {
        assert_eq!(
            change_for_error(
                target(CredentialKind::Oauth, false),
                UpstreamError::AuthExpired,
            ),
            Some(CredentialStateChange::MissingRefreshToken)
        );
        assert_eq!(
            change_for_error(
                target(CredentialKind::Oauth, true),
                UpstreamError::AuthExpired,
            ),
            Some(CredentialStateChange::AuthExpired)
        );
        assert_eq!(
            change_for_error(
                target(CredentialKind::ApiKey, false),
                UpstreamError::AuthExpired,
            ),
            Some(CredentialStateChange::AuthExpired)
        );
    }

    #[test]
    fn spark_persistence_routes_quota_to_shadow_and_authentication_to_parent() {
        let parent_id = CredentialId::new(9).unwrap();
        let shadow_id = CredentialId::new(10).unwrap();
        let target = CredentialAttemptTarget::with_runtime_identity(
            ChannelId::new(7).unwrap(),
            shadow_id,
            parent_id,
            parent_id,
            CredentialKind::Oauth,
            true,
            true,
        )
        .unwrap();

        assert_eq!(
            state_owner(
                target,
                CredentialStateChange::RateLimited { retry_after: None }
            ),
            shadow_id
        );
        assert_eq!(
            state_owner(target, CredentialStateChange::QuotaExhausted),
            shadow_id
        );
        assert_eq!(
            state_owner(target, CredentialStateChange::AuthRevoked),
            parent_id
        );
        assert_eq!(
            state_owner(target, CredentialStateChange::AuthenticationSucceeded),
            parent_id
        );
    }

    #[test]
    fn runtime_feedback_rejects_mismatched_shared_health_identity() {
        assert!(
            CredentialAttemptTarget::with_runtime_identity(
                ChannelId::new(7).unwrap(),
                CredentialId::new(10).unwrap(),
                CredentialId::new(9).unwrap(),
                CredentialId::new(8).unwrap(),
                CredentialKind::Oauth,
                true,
                true,
            )
            .is_err()
        );
    }

    fn target(
        credential_kind: CredentialKind,
        oauth_has_refresh_token: bool,
    ) -> CredentialAttemptTarget {
        CredentialAttemptTarget::new(
            ChannelId::new(7).unwrap(),
            9,
            credential_kind,
            oauth_has_refresh_token,
            false,
        )
        .unwrap()
    }
}
