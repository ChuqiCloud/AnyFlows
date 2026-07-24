use af_domain::{UpstreamError, UpstreamServerStatus};

/// M1 默认只允许一次同渠道 5xx 短重试，避免上游持续故障时放大延迟。
pub const DEFAULT_TRANSIENT_SERVER_RETRY_LIMIT: u8 = 1;

/// 调度层对一次已归一化上游故障作出的请求级动作。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureAction {
    /// 客户端请求或协议边界不可重试，立即结束当前请求。
    Terminate,
    /// 放弃当前渠道，交给请求级重试计划选择下一候选。
    Failover,
    /// 在切换渠道前，对当前渠道执行一次受预算约束的短重试。
    RetrySameChannel,
    /// 刷新当前 OAuth 凭据后再重试当前渠道。
    RefreshAuthAndRetry,
}

/// 计算故障动作所需的请求级上下文。
///
/// `same_channel_retries_started` 只统计初次发送之后已经启动的同渠道短重试；
/// `auth_refresh_retry_available` 仅在凭据可刷新且本请求尚未尝试刷新时为真。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FailureContext {
    same_channel_retries_started: u8,
    auth_refresh_retry_available: bool,
}

impl FailureContext {
    /// 创建不携带原始错误文本或凭据内容的故障动作上下文。
    #[must_use]
    pub const fn new(same_channel_retries_started: u8, auth_refresh_retry_available: bool) -> Self {
        Self {
            same_channel_retries_started,
            auth_refresh_retry_available,
        }
    }

    /// 返回当前渠道已经启动的短重试次数。
    #[must_use]
    pub const fn same_channel_retries_started(self) -> u8 {
        self.same_channel_retries_started
    }

    /// 返回本请求是否仍可执行一次认证刷新重试。
    #[must_use]
    pub const fn auth_refresh_retry_available(self) -> bool {
        self.auth_refresh_retry_available
    }
}

/// 将闭合的上游错误映射为有限、无副作用的请求级动作。
///
/// 本策略只决定请求下一步，不执行冷却、健康惩罚、凭据禁用或 OAuth 刷新；这些
/// 副作用由后续调度编排层消费动作后完成。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FailurePolicy {
    transient_server_retry_limit: u8,
}

impl FailurePolicy {
    /// 使用指定的同渠道短重试上限创建策略；零表示 5xx 直接故障转移。
    #[must_use]
    pub const fn new(transient_server_retry_limit: u8) -> Self {
        Self {
            transient_server_retry_limit,
        }
    }

    /// 返回当前策略允许的同渠道 5xx 短重试次数。
    #[must_use]
    pub const fn transient_server_retry_limit(self) -> u8 {
        self.transient_server_retry_limit
    }

    /// 根据结构化错误和请求级上下文决定下一步动作。
    #[must_use]
    pub const fn action_for(self, error: UpstreamError, context: FailureContext) -> FailureAction {
        match error {
            UpstreamError::BadRequest | UpstreamError::ProtocolError => FailureAction::Terminate,
            UpstreamError::AuthExpired if context.auth_refresh_retry_available => {
                FailureAction::RefreshAuthAndRetry
            }
            UpstreamError::ServerError { status }
                if is_transient_server_status(status)
                    && context.same_channel_retries_started < self.transient_server_retry_limit =>
            {
                FailureAction::RetrySameChannel
            }
            UpstreamError::RateLimited
            | UpstreamError::Overloaded
            | UpstreamError::AuthExpired
            | UpstreamError::AuthRevoked
            | UpstreamError::QuotaExhausted
            | UpstreamError::ModelUnsupported
            | UpstreamError::Network
            | UpstreamError::ServerError { .. } => FailureAction::Failover,
        }
    }
}

impl Default for FailurePolicy {
    fn default() -> Self {
        Self::new(DEFAULT_TRANSIENT_SERVER_RETRY_LIMIT)
    }
}

const fn is_transient_server_status(status: UpstreamServerStatus) -> bool {
    matches!(status.get(), 500 | 502 | 503 | 504)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_protocol_errors_terminate() {
        let policy = FailurePolicy::default();

        for error in [UpstreamError::BadRequest, UpstreamError::ProtocolError] {
            assert_eq!(
                policy.action_for(error, FailureContext::default()),
                FailureAction::Terminate
            );
        }
    }

    #[test]
    fn channel_scoped_failures_fail_over() {
        let policy = FailurePolicy::default();

        for error in [
            UpstreamError::RateLimited,
            UpstreamError::Overloaded,
            UpstreamError::AuthRevoked,
            UpstreamError::QuotaExhausted,
            UpstreamError::ModelUnsupported,
            UpstreamError::Network,
        ] {
            assert_eq!(
                policy.action_for(error, FailureContext::default()),
                FailureAction::Failover
            );
        }
    }

    #[test]
    fn transient_server_errors_retry_only_within_budget() {
        let policy = FailurePolicy::new(2);

        for status in [500, 502, 503, 504] {
            let error = server_error(status);
            assert_eq!(
                policy.action_for(error, FailureContext::new(0, false)),
                FailureAction::RetrySameChannel
            );
            assert_eq!(
                policy.action_for(error, FailureContext::new(1, false)),
                FailureAction::RetrySameChannel
            );
            assert_eq!(
                policy.action_for(error, FailureContext::new(2, false)),
                FailureAction::Failover
            );
        }
    }

    #[test]
    fn zero_budget_and_non_transient_server_errors_fail_over() {
        let no_retry = FailurePolicy::new(0);
        assert_eq!(
            no_retry.action_for(server_error(503), FailureContext::default()),
            FailureAction::Failover
        );

        let policy = FailurePolicy::default();
        for status in [501, 505, 507, 529, 599] {
            assert_eq!(
                policy.action_for(server_error(status), FailureContext::default()),
                FailureAction::Failover
            );
        }
    }

    #[test]
    fn auth_expiry_refreshes_only_when_request_still_has_one_shot_refresh() {
        let policy = FailurePolicy::default();

        assert_eq!(
            policy.action_for(UpstreamError::AuthExpired, FailureContext::new(0, true)),
            FailureAction::RefreshAuthAndRetry
        );
        assert_eq!(
            policy.action_for(UpstreamError::AuthExpired, FailureContext::new(0, false)),
            FailureAction::Failover
        );
    }

    #[test]
    fn default_policy_allows_exactly_one_transient_server_retry() {
        let policy = FailurePolicy::default();
        let error = server_error(503);

        assert_eq!(
            policy.transient_server_retry_limit(),
            DEFAULT_TRANSIENT_SERVER_RETRY_LIMIT
        );
        assert_eq!(
            policy.action_for(error, FailureContext::new(0, false)),
            FailureAction::RetrySameChannel
        );
        assert_eq!(
            policy.action_for(error, FailureContext::new(1, false)),
            FailureAction::Failover
        );
    }

    #[test]
    fn context_accessors_preserve_only_bounded_control_state() {
        let context = FailureContext::new(7, true);

        assert_eq!(context.same_channel_retries_started(), 7);
        assert!(context.auth_refresh_retry_available());
        assert_eq!(
            format!("{context:?}"),
            "FailureContext { same_channel_retries_started: 7, auth_refresh_retry_available: true }"
        );
    }

    fn server_error(status: u16) -> UpstreamError {
        UpstreamError::ServerError {
            status: UpstreamServerStatus::new(status).unwrap(),
        }
    }
}
