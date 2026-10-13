use std::{sync::Arc, time::Instant};

use af_db::{
    RequestFailureKind, RequestOutcomeRepository, RequestOutcomeRepositoryError,
    RequestOutcomeSubject, RequestOutcomeWrite, ServiceAccountOutcomeSink,
    ServiceAccountRequestOutcome,
};
use af_domain::{AfError, ChannelId, GatewayPrincipal, Operation, Protocol, UpstreamError};

/// 调度执行成功后返回的公开值与最终渠道归属。
pub struct RoutedExecution<T> {
    value: T,
    channel_id: ChannelId,
}

impl<T> RoutedExecution<T> {
    /// 绑定已经由候选报告确认的最终成功渠道。
    #[must_use]
    pub const fn new(value: T, channel_id: ChannelId) -> Self {
        Self { value, channel_id }
    }

    /// 返回最终成功渠道。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 消费路由结果并返回公开服务值。
    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }
}

/// 单个同步模型请求的低敏感度终态上下文。
pub struct RequestOutcomeContext {
    request_id: String,
    protocol: Protocol,
    operation: Operation,
    model: String,
    subject: Option<RequestOutcomeSubject>,
    service_subject: Option<(
        af_domain::OrganizationServiceAccountRuntimeIdentity,
        af_domain::GroupId,
    )>,
    started_at: Instant,
}

impl RequestOutcomeContext {
    /// 固化请求标识和低基数路由维度，不接收主体、凭据或原始协议内容。
    #[must_use]
    pub fn new(request_id: &str, protocol: Protocol, operation: Operation, model: &str) -> Self {
        Self {
            request_id: request_id.to_owned(),
            protocol,
            operation,
            model: model.to_owned(),
            subject: None,
            service_subject: None,
            started_at: Instant::now(),
        }
    }

    /// 构造带下游主体归属的请求终态上下文。
    #[must_use]
    pub fn for_principal(
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        principal: GatewayPrincipal,
    ) -> Self {
        let organization = principal.organization_principal();
        Self {
            request_id: request_id.to_owned(),
            protocol,
            operation,
            model: model.to_owned(),
            subject: Some(RequestOutcomeSubject::new(
                principal.user_id(),
                principal.token_id(),
                principal.group_id(),
                organization.map(|value| value.organization_id()),
                organization.and_then(|value| value.team_id()),
            )),
            service_subject: None,
            started_at: Instant::now(),
        }
    }

    /// Service identities use the distribution sink exclusively, including on failure.
    #[must_use]
    pub fn for_service_account(
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        identity: af_domain::OrganizationServiceAccountRuntimeIdentity,
        group_id: af_domain::GroupId,
    ) -> Self {
        let mut context = Self::new(request_id, protocol, operation, model);
        context.service_subject = Some((identity, group_id));
        context
    }

    /// 持久化服务调用终态并剥离内部渠道包装。
    pub async fn finish<T>(
        self,
        runtime: Option<&RequestOutcomeRuntime>,
        result: Result<RoutedExecution<T>, AfError>,
    ) -> Result<T, AfError> {
        match result {
            Ok(routed) => {
                let channel_id = routed.channel_id();
                let value = routed.into_value();
                self.record_success(runtime, channel_id).await;
                Ok(value)
            }
            Err(error) => {
                self.record_failure(runtime, &error, None).await;
                Err(error)
            }
        }
    }

    /// 记录成功终态；观测存储故障不得覆盖已经完成的计费和上游结果。
    pub async fn record_success(
        &self,
        runtime: Option<&RequestOutcomeRuntime>,
        channel_id: ChannelId,
    ) {
        let Some(runtime) = runtime else {
            return;
        };
        if self.service_subject.is_some() {
            self.record_service(runtime, Some(channel_id), None).await;
            return;
        }
        let write = match self.subject {
            Some(subject) => RequestOutcomeWrite::succeeded_for_subject(
                &self.request_id,
                self.protocol,
                self.operation,
                &self.model,
                subject,
                Some(channel_id),
                self.started_at.elapsed(),
            ),
            None => RequestOutcomeWrite::succeeded(
                &self.request_id,
                self.protocol,
                self.operation,
                &self.model,
                Some(channel_id),
                self.started_at.elapsed(),
            ),
        };
        runtime.record(write).await;
    }

    /// 记录失败终态，原始错误只在内存中映射为闭合分类。
    pub async fn record_failure(
        &self,
        runtime: Option<&RequestOutcomeRuntime>,
        error: &AfError,
        channel_id: Option<ChannelId>,
    ) {
        let Some(runtime) = runtime else {
            return;
        };
        let failure_kind = classify_error(error);
        if self.service_subject.is_some() {
            self.record_service(runtime, channel_id, Some(failure_kind))
                .await;
            return;
        }
        let write = match self.subject {
            Some(subject) => RequestOutcomeWrite::failed_for_subject(
                &self.request_id,
                self.protocol,
                self.operation,
                &self.model,
                subject,
                failure_kind,
                channel_id,
                self.started_at.elapsed(),
            ),
            None => RequestOutcomeWrite::failed(
                &self.request_id,
                self.protocol,
                self.operation,
                &self.model,
                failure_kind,
                channel_id,
                self.started_at.elapsed(),
            ),
        };
        runtime.record(write).await;
    }

    async fn record_service(
        &self,
        runtime: &RequestOutcomeRuntime,
        channel_id: Option<ChannelId>,
        failure_kind: Option<RequestFailureKind>,
    ) {
        let Some((identity, group_id)) = self.service_subject else {
            return;
        };
        let fact = ServiceAccountRequestOutcome::new(
            identity,
            group_id,
            &self.request_id,
            self.protocol,
            self.operation,
            &self.model,
            channel_id,
            failure_kind,
            self.started_at.elapsed(),
        );
        let Ok(fact) = fact else {
            record_runtime_error("service_outcome_invalid_fact");
            return;
        };
        let Some(sink) = runtime.service_accounts.as_ref() else {
            record_runtime_error("service_outcome_sink_missing");
            return;
        };
        for attempt in 0..3 {
            match sink.persist(fact.clone()).await {
                Ok(()) => return,
                Err(
                    RequestOutcomeRepositoryError::Query | RequestOutcomeRepositoryError::Timeout,
                ) if attempt < 2 => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Err(error) => {
                    record_repository_error(error);
                    return;
                }
            }
        }
    }
}

/// 请求终态事实的进程级持久化适配器。
#[derive(Clone)]
pub struct RequestOutcomeRuntime {
    repository: RequestOutcomeRepository,
    service_accounts: Option<Arc<dyn ServiceAccountOutcomeSink>>,
}

impl RequestOutcomeRuntime {
    /// 绑定只追加请求终态仓储。
    #[must_use]
    pub const fn new(repository: RequestOutcomeRepository) -> Self {
        Self {
            repository,
            service_accounts: None,
        }
    }

    #[must_use]
    pub fn with_service_accounts(
        mut self,
        sink: Option<Arc<dyn ServiceAccountOutcomeSink>>,
    ) -> Self {
        self.service_accounts = sink;
        self
    }

    async fn record(&self, write: Result<RequestOutcomeWrite, af_db::RequestOutcomeWriteError>) {
        let write = match write {
            Ok(write) => write,
            Err(_) => {
                record_runtime_error("request_outcome_invalid_fact");
                return;
            }
        };
        if let Err(error) = self.repository.record(&write).await {
            record_repository_error(error);
        }
    }
}

impl std::fmt::Debug for RequestOutcomeRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RequestOutcomeRuntime")
            .field("repository", &self.repository)
            .field("has_service_sink", &self.service_accounts.is_some())
            .finish()
    }
}

fn classify_error(error: &AfError) -> RequestFailureKind {
    match error {
        AfError::InvalidRequest | AfError::InvalidApiKey => RequestFailureKind::InvalidRequest,
        AfError::ModelNotAllowed => RequestFailureKind::ModelNotAllowed,
        AfError::InsufficientQuota => RequestFailureKind::InsufficientQuota,
        AfError::QuotaWindowLimited { .. } => RequestFailureKind::QuotaLimited,
        AfError::ConcurrencyLimited => RequestFailureKind::ConcurrencyLimited,
        AfError::RequestOutcomeUnknown => RequestFailureKind::OutcomeUnknown,
        AfError::Upstream(error) => classify_upstream_error(*error),
        AfError::TaskNotFound | AfError::IdempotencyConflict | AfError::Internal => {
            RequestFailureKind::Internal
        }
    }
}

const fn classify_upstream_error(error: UpstreamError) -> RequestFailureKind {
    match error {
        UpstreamError::RateLimited { .. } => RequestFailureKind::UpstreamRateLimited,
        UpstreamError::Overloaded { .. } => RequestFailureKind::UpstreamOverloaded,
        UpstreamError::AuthExpired
        | UpstreamError::AuthRevoked
        | UpstreamError::AccountDisabled => RequestFailureKind::UpstreamAuthentication,
        UpstreamError::QuotaExhausted => RequestFailureKind::UpstreamQuota,
        UpstreamError::ModelUnsupported => RequestFailureKind::UpstreamModel,
        UpstreamError::ProtocolError | UpstreamError::BadRequest => {
            RequestFailureKind::UpstreamProtocol
        }
        UpstreamError::ServerError { .. } => RequestFailureKind::UpstreamServer,
        UpstreamError::Network { .. } => RequestFailureKind::UpstreamNetwork,
    }
}

fn record_repository_error(error: RequestOutcomeRepositoryError) {
    let error_kind = match error {
        RequestOutcomeRepositoryError::Query => "request_outcome_record_query",
        RequestOutcomeRepositoryError::Timeout => "request_outcome_record_timeout",
        RequestOutcomeRepositoryError::Conflict => "request_outcome_record_conflict",
        RequestOutcomeRepositoryError::Invariant => "request_outcome_record_invariant",
    };
    record_runtime_error(error_kind);
}

fn record_runtime_error(error_kind: &'static str) {
    tracing::error!(
        target: "af_server::request_outcome",
        error_kind,
        "请求终态观测未能持久化，原请求结果保持不变"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_domain::{NetworkFailureKind, RateLimitScope};

    #[test]
    fn public_errors_map_to_closed_low_cardinality_categories() {
        for (error, expected) in [
            (AfError::InvalidRequest, RequestFailureKind::InvalidRequest),
            (
                AfError::Upstream(UpstreamError::rate_limited(RateLimitScope::Window)),
                RequestFailureKind::UpstreamRateLimited,
            ),
            (
                AfError::Upstream(UpstreamError::network(NetworkFailureKind::Connect)),
                RequestFailureKind::UpstreamNetwork,
            ),
            (AfError::Internal, RequestFailureKind::Internal),
        ] {
            assert_eq!(classify_error(&error), expected);
        }
    }

    #[test]
    fn routed_execution_exposes_only_value_and_channel() {
        let channel_id = ChannelId::new(7).unwrap();
        let routed = RoutedExecution::new("ok", channel_id);
        assert_eq!(routed.channel_id(), channel_id);
        assert_eq!(routed.into_value(), "ok");
    }
}
