use std::collections::BTreeSet;

use af_db::{ChannelStateRepository, ChannelStateRepositoryError};
use af_domain::{ChannelAutoBanRules, ChannelId, UpstreamError, UpstreamServerStatus};
use af_relay::RelayAttemptReport;
use af_scheduler::{AutoBanEvidence, AutoBanPolicy, FailureAction};

/// 与 Relay 候选索引对应的渠道自动禁用策略；不包含凭据或上游地址。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ChannelAutoBanAttemptTarget {
    channel_id: ChannelId,
    policy: AutoBanPolicy,
    pool_mode: bool,
}

impl ChannelAutoBanAttemptTarget {
    /// 从运行时快照规则构造一次请求级不可变策略。
    pub(crate) fn new(
        channel_id: ChannelId,
        rules: &ChannelAutoBanRules,
        pool_mode: bool,
    ) -> Result<Self, af_domain::AfError> {
        let policy =
            AutoBanPolicy::from_channel_rules(rules).map_err(|_| af_domain::AfError::Internal)?;
        Ok(Self {
            channel_id,
            policy,
            pool_mode,
        })
    }

    /// 返回候选所属渠道。
    pub(crate) const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    fn should_disable(
        &self,
        error: UpstreamError,
        server_status: Option<UpstreamServerStatus>,
        evidence: Option<&str>,
    ) -> bool {
        if self.pool_mode {
            return false;
        }
        let evidence = evidence.and_then(|value| AutoBanEvidence::from_redacted_text(value).ok());
        self.policy
            .should_auto_disable_with_server_status_and_evidence(
                error,
                // Relay 已经完成当前候选的补救与故障转移决策；此处只执行状态副作用。
                FailureAction::Failover,
                server_status,
                evidence.as_ref(),
            )
    }
}

/// 最佳努力执行渠道级自动禁用；状态写入或刷新失败不得覆盖业务响应。
pub(crate) async fn persist_channel_auto_ban_feedback(
    store: Option<&ChannelStateRepository>,
    scheduler: &af_scheduler::IndexedWeightedScheduler,
    targets: &[ChannelAutoBanAttemptTarget],
    report: &RelayAttemptReport,
) {
    let Some(store) = store else {
        return;
    };
    let Some(channel_ids) = matching_channels(targets, report) else {
        tracing::error!(
            target: "af_server::channel_auto_ban",
            error_kind = "channel_auto_ban_invariant",
            "渠道自动禁用反馈候选索引无效"
        );
        return;
    };
    let mut refresh_required = false;
    for channel_id in channel_ids {
        match store.auto_disable(channel_id).await {
            // 只有本请求完成新的状态转换才需要立即刷新；幂等结果不会重复制造全量读取。
            Ok(af_db::ChannelAutoDisableOutcome::Disabled) => refresh_required = true,
            Ok(
                af_db::ChannelAutoDisableOutcome::AlreadyDisabled
                | af_db::ChannelAutoDisableOutcome::NotEligible,
            ) => {}
            Err(error) => record_store_error(error),
        }
    }
    if refresh_required && scheduler.refresh().await.is_err() {
        tracing::error!(
            target: "af_server::channel_auto_ban",
            error_kind = "channel_auto_ban_refresh",
            "渠道自动禁用后刷新运行时快照失败"
        );
    }
}

fn matching_channels(
    targets: &[ChannelAutoBanAttemptTarget],
    report: &RelayAttemptReport,
) -> Option<BTreeSet<ChannelId>> {
    let mut channels = BTreeSet::new();
    for failure in report.failures() {
        let target = targets.get(failure.candidate_index())?;
        if target.should_disable(
            failure.error(),
            failure.server_status(),
            failure.evidence().map(|value| value.as_match_text()),
        ) {
            channels.insert(target.channel_id());
        }
    }
    Some(channels)
}

fn record_store_error(error: ChannelStateRepositoryError) {
    let error_kind = match error {
        ChannelStateRepositoryError::InvalidConfiguration => "channel_auto_ban_configuration",
        ChannelStateRepositoryError::InvalidBatchSize => "channel_auto_ban_batch",
        ChannelStateRepositoryError::Query => "channel_auto_ban_query",
        ChannelStateRepositoryError::Timeout => "channel_auto_ban_timeout",
        ChannelStateRepositoryError::Invariant => "channel_auto_ban_invariant",
        _ => "channel_auto_ban_unknown",
    };
    tracing::warn!(
        target: "af_server::channel_auto_ban",
        error_kind,
        "渠道自动禁用状态写入失败，当前业务结果继续返回"
    );
}

#[cfg(test)]
mod tests {
    use af_domain::{ChannelAutoBanRules, UpstreamError, UpstreamServerStatus};

    use super::*;

    #[test]
    fn target_matches_exact_status_and_keyword_without_exposing_rule_content() {
        const CANARY: &str = "workspace-disabled-canary";
        let rules = ChannelAutoBanRules::new(vec![503], vec![CANARY.to_owned()]).unwrap();
        let target =
            ChannelAutoBanAttemptTarget::new(ChannelId::new(7).unwrap(), &rules, false).unwrap();

        assert!(target.should_disable(server_error(503), None, None));
        assert!(!target.should_disable(server_error(502), None, None));
        assert!(target.should_disable(
            UpstreamError::overloaded(),
            UpstreamServerStatus::new(503),
            None,
        ));
        assert!(target.should_disable(UpstreamError::ProtocolError, None, Some(CANARY)));
        assert!(!target.should_disable(UpstreamError::BadRequest, None, Some(CANARY)));
        assert!(!format!("{target:?}").contains(CANARY));
    }

    #[test]
    fn pool_mode_skips_channel_auto_disable() {
        let rules = ChannelAutoBanRules::new(vec![503], Vec::new()).unwrap();
        let target =
            ChannelAutoBanAttemptTarget::new(ChannelId::new(7).unwrap(), &rules, true).unwrap();
        assert!(!target.should_disable(server_error(503), None, None));
    }

    fn server_error(status: u16) -> UpstreamError {
        UpstreamError::ServerError {
            status: UpstreamServerStatus::new(status).unwrap(),
        }
    }
}
