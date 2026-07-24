use std::{collections::BTreeSet, fmt, future::Future};

use af_db::{ChannelAutoDisableOutcome, ChannelStateRepository, ChannelStateRepositoryError};
use af_domain::{ChannelId, UpstreamError, UpstreamServerStatus};
use thiserror::Error;

use crate::{FailureAction, FailureContext, FailurePolicy};

/// 单份自动禁用策略允许的结构化规则上限。
pub const MAX_AUTO_BAN_RULES: usize = 64;

/// 可配置的结构化渠道自动禁用规则。
///
/// `BadRequest` 刻意不提供规则变体，客户端输入错误永远不得触发渠道状态变更。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum AutoBanRule {
    /// 上游限流。
    RateLimited,
    /// 上游过载。
    Overloaded,
    /// 上游认证过期；仅在刷新机会耗尽后生效。
    AuthExpired,
    /// 上游凭据已撤销。
    AuthRevoked,
    /// 上游额度耗尽。
    QuotaExhausted,
    /// 上游不支持当前模型。
    ModelUnsupported,
    /// 上游响应或渠道协议不兼容。
    ProtocolError,
    /// 上游网络传输失败。
    Network,
    /// 精确匹配一个已校验的 5xx 状态码。
    ServerStatus(UpstreamServerStatus),
}

/// 自动禁用规则配置错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AutoBanPolicyError {
    /// 输入规则数量超过单份策略的安全上限。
    #[error("自动禁用规则数量超过上限")]
    TooManyRules,
}

/// 有界、无原始错误文本的渠道自动禁用策略。
#[derive(Clone, Default, Eq, PartialEq)]
pub struct AutoBanPolicy {
    rules: BTreeSet<AutoBanRule>,
}

impl AutoBanPolicy {
    /// 校验规则容量并创建不可变策略；重复规则会被安全去重。
    pub fn new(rules: Vec<AutoBanRule>) -> Result<Self, AutoBanPolicyError> {
        if rules.len() > MAX_AUTO_BAN_RULES {
            return Err(AutoBanPolicyError::TooManyRules);
        }
        Ok(Self {
            rules: rules.into_iter().collect(),
        })
    }

    /// 返回去重后的结构化规则数量。
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 判断当前故障是否应在请求动作之外触发渠道自动禁用。
    ///
    /// 同渠道短重试和认证刷新仍可执行时必须先完成补救；只有补救耗尽后的故障转移，
    /// 或显式配置的协议终止错误，才允许进入持久化副作用。
    #[must_use]
    pub fn should_auto_disable(&self, error: UpstreamError, action: FailureAction) -> bool {
        if matches!(
            action,
            FailureAction::RetrySameChannel | FailureAction::RefreshAuthAndRetry
        ) {
            return false;
        }
        structured_rule(error).is_some_and(|rule| self.rules.contains(&rule))
    }
}

impl fmt::Debug for AutoBanPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutoBanPolicy")
            .field("rule_count", &self.rules.len())
            .finish()
    }
}

/// 调度协调器依赖的最小渠道自动禁用端口。
///
/// 生产实现由 [`ChannelStateRepository`] 提供；测试或后续事件化实现可以替换该端口，
/// 不需要让故障策略依赖 SeaORM 类型。
pub trait ChannelAutoDisableStore {
    /// 尝试按渠道持久化自动禁用状态。
    fn auto_disable(
        &self,
        channel_id: ChannelId,
    ) -> impl Future<Output = Result<ChannelAutoDisableOutcome, ChannelStateRepositoryError>> + Send;
}

impl ChannelAutoDisableStore for ChannelStateRepository {
    fn auto_disable(
        &self,
        channel_id: ChannelId,
    ) -> impl Future<Output = Result<ChannelAutoDisableOutcome, ChannelStateRepositoryError>> + Send
    {
        ChannelStateRepository::auto_disable(self, channel_id)
    }
}

/// 一次渠道状态副作用的非阻塞业务结果。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelStateEffectOutcome {
    /// 当前错误或动作不需要自动禁用。
    NotRequested,
    /// 仓储已返回幂等状态转换结果。
    Completed(ChannelAutoDisableOutcome),
    /// 持久化失败；请求动作仍必须按原决定继续执行。
    Failed(ChannelStateRepositoryError),
}

/// 请求动作与渠道状态副作用组成的一次故障协调结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelFailureOutcome {
    action: FailureAction,
    state_effect: ChannelStateEffectOutcome,
}

impl ChannelFailureOutcome {
    /// 返回请求级重试、故障转移或终止动作。
    #[must_use]
    pub const fn action(self) -> FailureAction {
        self.action
    }

    /// 返回独立的渠道状态副作用结果。
    #[must_use]
    pub const fn state_effect(self) -> ChannelStateEffectOutcome {
        self.state_effect
    }
}

/// 将故障动作计算与渠道自动禁用副作用串成一个有界协调步骤。
pub struct ChannelFailureCoordinator<S> {
    failure_policy: FailurePolicy,
    auto_ban_policy: AutoBanPolicy,
    store: S,
}

impl<S> ChannelFailureCoordinator<S> {
    /// 使用显式请求动作策略、自动禁用规则和持久化端口创建协调器。
    #[must_use]
    pub const fn new(
        failure_policy: FailurePolicy,
        auto_ban_policy: AutoBanPolicy,
        store: S,
    ) -> Self {
        Self {
            failure_policy,
            auto_ban_policy,
            store,
        }
    }
}

impl<S> ChannelFailureCoordinator<S>
where
    S: ChannelAutoDisableStore,
{
    /// 先计算不可变请求动作，再按结构化规则尝试自动禁用当前渠道。
    ///
    /// 数据库失败只进入 `state_effect`，不得阻止调用方执行已经确定的 failover 或
    /// terminate，避免健康状态副作用反向拖死请求路径。
    pub async fn handle_failure(
        &self,
        channel_id: ChannelId,
        error: UpstreamError,
        context: FailureContext,
    ) -> ChannelFailureOutcome {
        let action = self.failure_policy.action_for(error, context);
        let state_effect = if self.auto_ban_policy.should_auto_disable(error, action) {
            match self.store.auto_disable(channel_id).await {
                Ok(outcome) => ChannelStateEffectOutcome::Completed(outcome),
                Err(error) => ChannelStateEffectOutcome::Failed(error),
            }
        } else {
            ChannelStateEffectOutcome::NotRequested
        };
        ChannelFailureOutcome {
            action,
            state_effect,
        }
    }
}

impl<S> fmt::Debug for ChannelFailureCoordinator<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelFailureCoordinator")
            .field(
                "transient_server_retry_limit",
                &self.failure_policy.transient_server_retry_limit(),
            )
            .field("auto_ban_rule_count", &self.auto_ban_policy.rule_count())
            .finish_non_exhaustive()
    }
}

const fn structured_rule(error: UpstreamError) -> Option<AutoBanRule> {
    match error {
        UpstreamError::RateLimited => Some(AutoBanRule::RateLimited),
        UpstreamError::Overloaded => Some(AutoBanRule::Overloaded),
        UpstreamError::AuthExpired => Some(AutoBanRule::AuthExpired),
        UpstreamError::AuthRevoked => Some(AutoBanRule::AuthRevoked),
        UpstreamError::QuotaExhausted => Some(AutoBanRule::QuotaExhausted),
        UpstreamError::ModelUnsupported => Some(AutoBanRule::ModelUnsupported),
        UpstreamError::ProtocolError => Some(AutoBanRule::ProtocolError),
        UpstreamError::ServerError { status } => Some(AutoBanRule::ServerStatus(status)),
        UpstreamError::BadRequest => None,
        UpstreamError::Network => Some(AutoBanRule::Network),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn policy_is_empty_by_default_and_bounds_input_before_deduplication() {
        let empty = AutoBanPolicy::default();
        assert_eq!(empty.rule_count(), 0);
        assert!(!empty.should_auto_disable(UpstreamError::AuthRevoked, FailureAction::Failover));

        let duplicated =
            AutoBanPolicy::new(vec![AutoBanRule::AuthRevoked, AutoBanRule::AuthRevoked]).unwrap();
        assert_eq!(duplicated.rule_count(), 1);
        assert_eq!(
            AutoBanPolicy::new(vec![AutoBanRule::Network; MAX_AUTO_BAN_RULES + 1]),
            Err(AutoBanPolicyError::TooManyRules)
        );
    }

    #[test]
    fn structured_rules_are_exact_and_bad_request_cannot_match() {
        let policy = AutoBanPolicy::new(vec![
            AutoBanRule::ProtocolError,
            AutoBanRule::ServerStatus(server_status(503)),
        ])
        .unwrap();

        assert!(policy.should_auto_disable(UpstreamError::ProtocolError, FailureAction::Terminate));
        assert!(policy.should_auto_disable(server_error(503), FailureAction::Failover));
        assert!(!policy.should_auto_disable(server_error(502), FailureAction::Failover));
        assert!(!policy.should_auto_disable(UpstreamError::BadRequest, FailureAction::Terminate));
    }

    #[test]
    fn retry_and_refresh_actions_defer_auto_disable_until_remediation_is_exhausted() {
        let policy = AutoBanPolicy::new(vec![
            AutoBanRule::ServerStatus(server_status(503)),
            AutoBanRule::AuthExpired,
        ])
        .unwrap();
        let failure_policy = FailurePolicy::default();

        let first_server_action =
            failure_policy.action_for(server_error(503), FailureContext::new(0, false));
        assert_eq!(first_server_action, FailureAction::RetrySameChannel);
        assert!(!policy.should_auto_disable(server_error(503), first_server_action));

        let exhausted_server_action =
            failure_policy.action_for(server_error(503), FailureContext::new(1, false));
        assert_eq!(exhausted_server_action, FailureAction::Failover);
        assert!(policy.should_auto_disable(server_error(503), exhausted_server_action));

        let refresh_action =
            failure_policy.action_for(UpstreamError::AuthExpired, FailureContext::new(0, true));
        assert_eq!(refresh_action, FailureAction::RefreshAuthAndRetry);
        assert!(!policy.should_auto_disable(UpstreamError::AuthExpired, refresh_action));
        assert!(policy.should_auto_disable(
            UpstreamError::AuthExpired,
            failure_policy.action_for(UpstreamError::AuthExpired, FailureContext::new(0, false))
        ));
    }

    #[tokio::test]
    async fn coordinator_keeps_request_action_when_state_persistence_fails() {
        let store = FakeStore::new(Err(ChannelStateRepositoryError::Query));
        let coordinator = ChannelFailureCoordinator::new(
            FailurePolicy::default(),
            AutoBanPolicy::new(vec![AutoBanRule::AuthRevoked]).unwrap(),
            store.clone(),
        );
        let channel_id = ChannelId::new(41).unwrap();

        let outcome = coordinator
            .handle_failure(
                channel_id,
                UpstreamError::AuthRevoked,
                FailureContext::default(),
            )
            .await;

        assert_eq!(outcome.action(), FailureAction::Failover);
        assert_eq!(
            outcome.state_effect(),
            ChannelStateEffectOutcome::Failed(ChannelStateRepositoryError::Query)
        );
        assert_eq!(store.calls(), vec![channel_id]);
    }

    #[tokio::test]
    async fn coordinator_persists_only_matching_exhausted_failures() {
        let store = FakeStore::new(Ok(ChannelAutoDisableOutcome::Disabled));
        let coordinator = ChannelFailureCoordinator::new(
            FailurePolicy::default(),
            AutoBanPolicy::new(vec![AutoBanRule::ServerStatus(server_status(503))]).unwrap(),
            store.clone(),
        );
        let channel_id = ChannelId::new(42).unwrap();

        let retry = coordinator
            .handle_failure(channel_id, server_error(503), FailureContext::new(0, false))
            .await;
        assert_eq!(retry.action(), FailureAction::RetrySameChannel);
        assert_eq!(
            retry.state_effect(),
            ChannelStateEffectOutcome::NotRequested
        );

        let failover = coordinator
            .handle_failure(channel_id, server_error(503), FailureContext::new(1, false))
            .await;
        assert_eq!(failover.action(), FailureAction::Failover);
        assert_eq!(
            failover.state_effect(),
            ChannelStateEffectOutcome::Completed(ChannelAutoDisableOutcome::Disabled)
        );
        assert_eq!(store.calls(), vec![channel_id]);
    }

    #[derive(Clone)]
    struct FakeStore {
        result: Result<ChannelAutoDisableOutcome, ChannelStateRepositoryError>,
        calls: Arc<Mutex<Vec<ChannelId>>>,
    }

    impl FakeStore {
        fn new(result: Result<ChannelAutoDisableOutcome, ChannelStateRepositoryError>) -> Self {
            Self {
                result,
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn calls(&self) -> Vec<ChannelId> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl ChannelAutoDisableStore for FakeStore {
        fn auto_disable(
            &self,
            channel_id: ChannelId,
        ) -> impl Future<Output = Result<ChannelAutoDisableOutcome, ChannelStateRepositoryError>> + Send
        {
            let calls = Arc::clone(&self.calls);
            let result = self.result;
            async move {
                calls.lock().unwrap().push(channel_id);
                result
            }
        }
    }

    fn server_error(status: u16) -> UpstreamError {
        UpstreamError::ServerError {
            status: server_status(status),
        }
    }

    fn server_status(status: u16) -> UpstreamServerStatus {
        UpstreamServerStatus::new(status).unwrap()
    }
}
