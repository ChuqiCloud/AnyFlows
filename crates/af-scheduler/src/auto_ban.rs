use std::{collections::BTreeSet, fmt, future::Future};

use af_db::{ChannelAutoDisableOutcome, ChannelStateRepository, ChannelStateRepositoryError};
use af_domain::{
    ChannelAutoBanRules, ChannelId, MAX_CHANNEL_AUTO_BAN_RULES, UpstreamError, UpstreamServerStatus,
};
use thiserror::Error;

use crate::{
    AutoBanEvidence, FailureAction, FailureContext, FailurePolicy,
    auto_ban_keyword::{AutoBanKeywordBuildError, AutoBanKeywordMatcher},
};

/// 单份自动禁用策略允许的结构化规则上限。
pub const MAX_AUTO_BAN_RULES: usize = MAX_CHANNEL_AUTO_BAN_RULES;

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
    /// 上游账号、组织或工作区已停用。
    AccountDisabled,
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
    /// 关键词为空、含控制字符或超过单条长度上限。
    #[error("自动禁用关键词无效")]
    InvalidKeyword,
    /// 全部关键词超过单份策略允许的总字节上限。
    #[error("自动禁用关键词总长度超过上限")]
    TooManyKeywordBytes,
    /// 有界关键词集合无法构造匹配器。
    #[error("自动禁用关键词匹配器构造失败")]
    KeywordMatcherBuild,
}

/// 有界、无原始错误文本的渠道自动禁用策略。
#[derive(Clone, Default, Eq, PartialEq)]
pub struct AutoBanPolicy {
    rules: BTreeSet<AutoBanRule>,
    keyword_matcher: Option<AutoBanKeywordMatcher>,
}

impl AutoBanPolicy {
    /// 从跨持久化边界的渠道规则创建不可变运行时策略。
    pub fn from_channel_rules(rules: &ChannelAutoBanRules) -> Result<Self, AutoBanPolicyError> {
        Self::with_keywords(
            rules
                .server_statuses()
                .iter()
                .copied()
                .map(AutoBanRule::ServerStatus)
                .collect(),
            rules.keywords().to_vec(),
        )
    }

    /// 校验规则容量并创建不可变策略；重复规则会被安全去重。
    pub fn new(rules: Vec<AutoBanRule>) -> Result<Self, AutoBanPolicyError> {
        Self::with_keywords(rules, Vec::new())
    }

    /// 校验结构化规则与关键词容量并创建不可变策略。
    ///
    /// 关键词仅作为结构化分类未命中时的兜底，且只能匹配调用方显式构造的
    /// [`AutoBanEvidence`]；输入计数在去重前检查，避免重复配置绕过容量限制。
    pub fn with_keywords(
        rules: Vec<AutoBanRule>,
        keywords: Vec<String>,
    ) -> Result<Self, AutoBanPolicyError> {
        if rules
            .len()
            .checked_add(keywords.len())
            .is_none_or(|count| count > MAX_AUTO_BAN_RULES)
        {
            return Err(AutoBanPolicyError::TooManyRules);
        }
        let keyword_matcher =
            AutoBanKeywordMatcher::build(keywords).map_err(|error| match error {
                AutoBanKeywordBuildError::InvalidKeyword => AutoBanPolicyError::InvalidKeyword,
                AutoBanKeywordBuildError::TooManyBytes => AutoBanPolicyError::TooManyKeywordBytes,
                AutoBanKeywordBuildError::MatcherBuild => AutoBanPolicyError::KeywordMatcherBuild,
            })?;
        Ok(Self {
            rules: rules.into_iter().collect(),
            keyword_matcher,
        })
    }

    /// 返回去重后的结构化规则数量。
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.structured_rule_count() + self.keyword_count()
    }

    /// 返回去重后的结构化规则数量。
    #[must_use]
    pub fn structured_rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 返回去重后的关键词规则数量。
    #[must_use]
    pub fn keyword_count(&self) -> usize {
        self.keyword_matcher
            .as_ref()
            .map_or(0, AutoBanKeywordMatcher::len)
    }

    /// 判断当前故障是否应在请求动作之外触发渠道自动禁用。
    ///
    /// 同渠道短重试和认证刷新仍可执行时必须先完成补救；只有补救耗尽后的故障转移，
    /// 或显式配置的协议终止错误，才允许进入持久化副作用。
    #[must_use]
    pub fn should_auto_disable(&self, error: UpstreamError, action: FailureAction) -> bool {
        self.should_auto_disable_with_evidence(error, action, None)
    }

    /// 先匹配闭合结构化规则，再对受限脱敏证据执行关键词兜底。
    ///
    /// `BadRequest` 属于客户端输入边界，即使证据命中关键词也不得改变渠道状态。
    #[must_use]
    pub fn should_auto_disable_with_evidence(
        &self,
        error: UpstreamError,
        action: FailureAction,
        evidence: Option<&AutoBanEvidence>,
    ) -> bool {
        self.should_auto_disable_with_server_status_and_evidence(error, action, None, evidence)
    }

    /// 同时使用原始 5xx 状态、闭合分类与受限证据判断是否自动停用。
    ///
    /// 上游分类可能把 503/529 收敛为 `Overloaded`，因此精确状态码必须独立保留；
    /// `BadRequest` 仍优先失败关闭，不能因异常服务端状态或关键词改变渠道状态。
    #[must_use]
    pub fn should_auto_disable_with_server_status_and_evidence(
        &self,
        error: UpstreamError,
        action: FailureAction,
        server_status: Option<UpstreamServerStatus>,
        evidence: Option<&AutoBanEvidence>,
    ) -> bool {
        if matches!(
            action,
            FailureAction::RetrySameChannel | FailureAction::RefreshAuthAndRetry
        ) {
            return false;
        }
        if error == UpstreamError::BadRequest {
            return false;
        }
        if server_status
            .is_some_and(|status| self.rules.contains(&AutoBanRule::ServerStatus(status)))
            || structured_rule(error).is_some_and(|rule| self.rules.contains(&rule))
        {
            return true;
        }
        evidence.is_some_and(|evidence| {
            self.keyword_matcher
                .as_ref()
                .is_some_and(|matcher| matcher.is_match(evidence))
        })
    }
}

impl fmt::Debug for AutoBanPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutoBanPolicy")
            .field("structured_rule_count", &self.structured_rule_count())
            .field("keyword_rule_count", &self.keyword_count())
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
        self.handle_failure_with_evidence(channel_id, error, context, None)
            .await
    }

    /// 计算请求动作，并在补救耗尽后使用结构化规则或受限证据执行状态副作用。
    pub async fn handle_failure_with_evidence(
        &self,
        channel_id: ChannelId,
        error: UpstreamError,
        context: FailureContext,
        evidence: Option<&AutoBanEvidence>,
    ) -> ChannelFailureOutcome {
        let action = self.failure_policy.action_for(error, context);
        let state_effect = if self
            .auto_ban_policy
            .should_auto_disable_with_evidence(error, action, evidence)
        {
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
            .field(
                "auto_ban_structured_rule_count",
                &self.auto_ban_policy.structured_rule_count(),
            )
            .field(
                "auto_ban_keyword_rule_count",
                &self.auto_ban_policy.keyword_count(),
            )
            .finish_non_exhaustive()
    }
}

const fn structured_rule(error: UpstreamError) -> Option<AutoBanRule> {
    match error {
        UpstreamError::RateLimited { .. } => Some(AutoBanRule::RateLimited),
        UpstreamError::Overloaded { .. } => Some(AutoBanRule::Overloaded),
        UpstreamError::AuthExpired => Some(AutoBanRule::AuthExpired),
        UpstreamError::AuthRevoked => Some(AutoBanRule::AuthRevoked),
        UpstreamError::AccountDisabled => Some(AutoBanRule::AccountDisabled),
        UpstreamError::QuotaExhausted => Some(AutoBanRule::QuotaExhausted),
        UpstreamError::ModelUnsupported => Some(AutoBanRule::ModelUnsupported),
        UpstreamError::ProtocolError => Some(AutoBanRule::ProtocolError),
        UpstreamError::ServerError { status } => Some(AutoBanRule::ServerStatus(status)),
        UpstreamError::BadRequest => None,
        UpstreamError::Network { .. } => Some(AutoBanRule::Network),
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
        assert_eq!(
            AutoBanPolicy::with_keywords(
                Vec::new(),
                vec!["revoked".to_owned(); MAX_AUTO_BAN_RULES + 1]
            ),
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

    #[test]
    fn keyword_fallback_requires_explicit_evidence_and_never_matches_bad_request() {
        let policy = AutoBanPolicy::with_keywords(
            Vec::new(),
            vec![
                "workspace disabled".to_owned(),
                "quota exhausted".to_owned(),
            ],
        )
        .unwrap();
        let matching =
            AutoBanEvidence::from_redacted_text("error_code: workspace disabled").unwrap();
        let unrelated = AutoBanEvidence::from_redacted_text("temporary network issue").unwrap();

        assert_eq!(policy.keyword_count(), 2);
        assert!(!policy.should_auto_disable(UpstreamError::AuthRevoked, FailureAction::Failover));
        assert!(policy.should_auto_disable_with_evidence(
            UpstreamError::AuthRevoked,
            FailureAction::Failover,
            Some(&matching)
        ));
        assert!(!policy.should_auto_disable_with_evidence(
            UpstreamError::AuthRevoked,
            FailureAction::Failover,
            Some(&unrelated)
        ));
        assert!(!policy.should_auto_disable_with_evidence(
            UpstreamError::BadRequest,
            FailureAction::Terminate,
            Some(&matching)
        ));
        assert!(!policy.should_auto_disable_with_evidence(
            UpstreamError::AuthExpired,
            FailureAction::RefreshAuthAndRetry,
            Some(&matching)
        ));
        assert!(!policy.should_auto_disable_with_evidence(
            server_error(503),
            FailureAction::RetrySameChannel,
            Some(&matching)
        ));
    }

    #[test]
    fn structured_rules_take_effect_without_keyword_evidence() {
        let policy = AutoBanPolicy::with_keywords(
            vec![AutoBanRule::AuthRevoked],
            vec!["workspace disabled".to_owned()],
        )
        .unwrap();

        assert!(policy.should_auto_disable_with_evidence(
            UpstreamError::AuthRevoked,
            FailureAction::Failover,
            None
        ));
        assert_eq!(policy.rule_count(), 2);
        assert_eq!(
            format!("{policy:?}"),
            "AutoBanPolicy { structured_rule_count: 1, keyword_rule_count: 1 }"
        );
    }

    #[test]
    fn exact_http_status_survives_more_specific_error_classification() {
        let status = server_status(529);
        let policy = AutoBanPolicy::new(vec![AutoBanRule::ServerStatus(status)]).unwrap();

        assert!(policy.should_auto_disable_with_server_status_and_evidence(
            UpstreamError::overloaded(),
            FailureAction::Failover,
            Some(status),
            None,
        ));
        assert!(!policy.should_auto_disable_with_server_status_and_evidence(
            UpstreamError::overloaded(),
            FailureAction::Failover,
            Some(server_status(503)),
            None,
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

    #[tokio::test]
    async fn coordinator_uses_keyword_evidence_without_changing_request_action() {
        let store = FakeStore::new(Ok(ChannelAutoDisableOutcome::Disabled));
        let coordinator = ChannelFailureCoordinator::new(
            FailurePolicy::default(),
            AutoBanPolicy::with_keywords(Vec::new(), vec!["account revoked".to_owned()]).unwrap(),
            store.clone(),
        );
        let channel_id = ChannelId::new(43).unwrap();
        let evidence = AutoBanEvidence::from_redacted_text("code: account revoked").unwrap();

        let outcome = coordinator
            .handle_failure_with_evidence(
                channel_id,
                UpstreamError::AuthRevoked,
                FailureContext::default(),
                Some(&evidence),
            )
            .await;

        assert_eq!(outcome.action(), FailureAction::Failover);
        assert_eq!(
            outcome.state_effect(),
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
