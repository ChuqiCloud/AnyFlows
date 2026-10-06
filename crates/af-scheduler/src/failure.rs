use af_domain::{
    CredentialKind, RateLimitScope, UpstreamError, UpstreamRetryAfter, UpstreamServerStatus,
};

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

/// 上游故障对凭据持久状态的闭合影响。
///
/// 本类型只表达业务语义，不执行数据库写入。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialFailureDisposition {
    /// 当前错误不应改变凭据持久状态。
    NoChange,
    /// 凭据仍可能自行恢复，只进入有界冷却。
    Temporary(TemporaryCredentialFailure),
    /// 结构化信号已经确认凭据或账号无法继续使用。
    Permanent(PermanentCredentialFailure),
}

/// 凭据失败决策所需的非敏感恢复上下文。
///
/// 构造器会忽略非 OAuth 凭据传入的 refresh token 标记，避免把 API Key 的未知认证
/// 错误误判成“缺少 refresh token”。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialFailureContext {
    credential_kind: CredentialKind,
    oauth_has_refresh_token: bool,
}

impl CredentialFailureContext {
    /// 从凭据类型与解密阶段提取的布尔能力创建上下文，不保留任何令牌内容。
    #[must_use]
    pub const fn new(credential_kind: CredentialKind, oauth_has_refresh_token: bool) -> Self {
        Self {
            credential_kind,
            oauth_has_refresh_token: matches!(credential_kind, CredentialKind::Oauth)
                && oauth_has_refresh_token,
        }
    }

    /// 返回数据库声明的凭据类型。
    #[must_use]
    pub const fn credential_kind(self) -> CredentialKind {
        self.credential_kind
    }

    /// 返回当前 OAuth 凭据是否具备 refresh token；非 OAuth 凭据始终返回假。
    #[must_use]
    pub const fn oauth_has_refresh_token(self) -> bool {
        self.oauth_has_refresh_token
    }

    const fn is_oauth_without_refresh_token(self) -> bool {
        matches!(self.credential_kind, CredentialKind::Oauth) && !self.oauth_has_refresh_token
    }
}

/// 可恢复凭据故障的稳定原因和受控等待提示。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemporaryCredentialFailure {
    /// 认证可能只是 access token 过期。
    AuthExpired,
    /// 上游限流；模型级信号不得扩大成整凭据冷却。
    RateLimited {
        scope: RateLimitScope,
        retry_after: Option<UpstreamRetryAfter>,
    },
    /// 余额或额度可能在充值、重置后恢复。
    QuotaExhausted,
    /// 上游过载，按短窗口冷却。
    Overloaded {
        retry_after: Option<UpstreamRetryAfter>,
    },
}

/// 只有确定性死亡信号才能进入的永久凭据状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermanentCredentialFailure {
    /// OAuth 认证已失效且没有 refresh token，无法自动恢复。
    MissingRefreshToken,
    /// API Key 或 token 已被明确吊销。
    AuthRevoked,
    /// 上游账号、组织或工作区已被明确停用。
    AccountDisabled,
}

/// 将闭合上游错误映射为凭据持久状态语义。
#[must_use]
pub const fn credential_failure_disposition(
    error: UpstreamError,
    context: CredentialFailureContext,
) -> CredentialFailureDisposition {
    match error {
        UpstreamError::AuthExpired if context.is_oauth_without_refresh_token() => {
            CredentialFailureDisposition::Permanent(PermanentCredentialFailure::MissingRefreshToken)
        }
        UpstreamError::AuthExpired => {
            CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::AuthExpired)
        }
        UpstreamError::RateLimited { scope, retry_after } => {
            CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::RateLimited {
                scope,
                retry_after,
            })
        }
        UpstreamError::QuotaExhausted => {
            CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::QuotaExhausted)
        }
        UpstreamError::Overloaded { retry_after } => {
            CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::Overloaded {
                retry_after,
            })
        }
        UpstreamError::AuthRevoked => {
            CredentialFailureDisposition::Permanent(PermanentCredentialFailure::AuthRevoked)
        }
        UpstreamError::AccountDisabled => {
            CredentialFailureDisposition::Permanent(PermanentCredentialFailure::AccountDisabled)
        }
        UpstreamError::BadRequest
        | UpstreamError::ModelUnsupported
        | UpstreamError::ProtocolError
        | UpstreamError::Network { .. }
        | UpstreamError::ServerError { .. } => CredentialFailureDisposition::NoChange,
    }
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
            UpstreamError::RateLimited { .. }
            | UpstreamError::Overloaded { .. }
            | UpstreamError::AuthExpired
            | UpstreamError::AuthRevoked
            | UpstreamError::AccountDisabled
            | UpstreamError::QuotaExhausted
            | UpstreamError::ModelUnsupported
            | UpstreamError::Network { .. }
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
            UpstreamError::rate_limited(RateLimitScope::Unknown),
            UpstreamError::overloaded(),
            UpstreamError::AuthRevoked,
            UpstreamError::AccountDisabled,
            UpstreamError::QuotaExhausted,
            UpstreamError::ModelUnsupported,
            UpstreamError::network(af_domain::NetworkFailureKind::Connect),
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

    #[test]
    fn credential_disposition_separates_recovery_from_permanent_death() {
        let retry_after = UpstreamRetryAfter::from_seconds(90).unwrap();
        let api_key = CredentialFailureContext::new(CredentialKind::ApiKey, false);
        let renewable_oauth = CredentialFailureContext::new(CredentialKind::Oauth, true);
        assert_eq!(
            credential_failure_disposition(
                UpstreamError::rate_limited_after(RateLimitScope::Window, retry_after),
                api_key,
            ),
            CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::RateLimited {
                scope: RateLimitScope::Window,
                retry_after: Some(retry_after),
            })
        );
        assert_eq!(
            credential_failure_disposition(UpstreamError::AuthExpired, renewable_oauth),
            CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::AuthExpired)
        );
        assert_eq!(
            credential_failure_disposition(UpstreamError::AuthRevoked, api_key),
            CredentialFailureDisposition::Permanent(PermanentCredentialFailure::AuthRevoked)
        );
        assert_eq!(
            credential_failure_disposition(UpstreamError::AccountDisabled, api_key),
            CredentialFailureDisposition::Permanent(PermanentCredentialFailure::AccountDisabled)
        );
        assert_eq!(
            credential_failure_disposition(UpstreamError::BadRequest, api_key),
            CredentialFailureDisposition::NoChange
        );
    }

    #[test]
    fn only_oauth_without_refresh_token_turns_auth_expiry_permanent() {
        let missing = CredentialFailureContext::new(CredentialKind::Oauth, false);
        assert_eq!(
            credential_failure_disposition(UpstreamError::AuthExpired, missing),
            CredentialFailureDisposition::Permanent(
                PermanentCredentialFailure::MissingRefreshToken
            )
        );

        let api_key = CredentialFailureContext::new(CredentialKind::ApiKey, true);
        assert_eq!(api_key.credential_kind(), CredentialKind::ApiKey);
        assert!(!api_key.oauth_has_refresh_token());
        assert_eq!(
            credential_failure_disposition(UpstreamError::AuthExpired, api_key),
            CredentialFailureDisposition::Temporary(TemporaryCredentialFailure::AuthExpired)
        );
    }

    fn server_error(status: u16) -> UpstreamError {
        UpstreamError::ServerError {
            status: UpstreamServerStatus::new(status).unwrap(),
        }
    }
}
