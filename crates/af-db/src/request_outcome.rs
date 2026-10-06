use std::{fmt, str::FromStr, time::Duration};

use af_domain::{
    ChannelId, GroupId, Operation, OrganizationId, OrganizationTeamId, Protocol, TokenId, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    SqlErr, TransactionTrait,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AnalyticsExportFactKind, AnalyticsExportRepository, DatabasePool,
    entity::{request_outcome_logs, users},
};

const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_REQUEST_ID_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 255;
const MAX_ERROR_CODE_BYTES: usize = 64;
const MAX_ERROR_MESSAGE_BYTES: usize = 255;
const SUCCEEDED_OUTCOME: i16 = 1;
const FAILED_OUTCOME: i16 = 2;

/// 一条请求终态事实关联的下游主体；只保存已经通过鉴权的稳定 ID。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestOutcomeSubject {
    user_id: UserId,
    token_id: TokenId,
    group_id: GroupId,
    organization_id: Option<OrganizationId>,
    organization_team_id: Option<OrganizationTeamId>,
}

impl RequestOutcomeSubject {
    /// 组合用户、令牌、分组和可选企业归属。
    #[must_use]
    pub const fn new(
        user_id: UserId,
        token_id: TokenId,
        group_id: GroupId,
        organization_id: Option<OrganizationId>,
        organization_team_id: Option<OrganizationTeamId>,
    ) -> Self {
        Self {
            user_id,
            token_id,
            group_id,
            organization_id,
            organization_team_id,
        }
    }

    #[must_use]
    pub const fn user_id(self) -> UserId {
        self.user_id
    }

    #[must_use]
    pub const fn token_id(self) -> TokenId {
        self.token_id
    }

    #[must_use]
    pub const fn group_id(self) -> GroupId {
        self.group_id
    }

    #[must_use]
    pub const fn organization_id(self) -> Option<OrganizationId> {
        self.organization_id
    }

    #[must_use]
    pub const fn organization_team_id(self) -> Option<OrganizationTeamId> {
        self.organization_team_id
    }
}

/// 同步模型请求失败的低基数、无原始文本分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RequestFailureKind {
    InvalidRequest,
    ModelNotAllowed,
    InsufficientQuota,
    QuotaLimited,
    ConcurrencyLimited,
    OutcomeUnknown,
    UpstreamRateLimited,
    UpstreamOverloaded,
    UpstreamAuthentication,
    UpstreamQuota,
    UpstreamModel,
    UpstreamProtocol,
    UpstreamServer,
    UpstreamNetwork,
    Internal,
}

impl RequestFailureKind {
    /// 返回数据库和管理 API 共用的稳定分类标识。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::ModelNotAllowed => "model_not_allowed",
            Self::InsufficientQuota => "insufficient_quota",
            Self::QuotaLimited => "quota_limited",
            Self::ConcurrencyLimited => "concurrency_limited",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::UpstreamRateLimited => "upstream_rate_limited",
            Self::UpstreamOverloaded => "upstream_overloaded",
            Self::UpstreamAuthentication => "upstream_authentication",
            Self::UpstreamQuota => "upstream_quota",
            Self::UpstreamModel => "upstream_model",
            Self::UpstreamProtocol => "upstream_protocol",
            Self::UpstreamServer => "upstream_server",
            Self::UpstreamNetwork => "upstream_network",
            Self::Internal => "internal",
        }
    }

    /// 返回普通调用方可以看到的稳定错误码；内部上游分类统一折叠。
    #[must_use]
    pub const fn public_error_code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::ModelNotAllowed | Self::UpstreamModel => "model_not_found",
            Self::InsufficientQuota => "insufficient_quota",
            Self::QuotaLimited | Self::ConcurrencyLimited | Self::UpstreamRateLimited => {
                "rate_limited"
            }
            Self::OutcomeUnknown => "request_outcome_unknown",
            Self::UpstreamOverloaded
            | Self::UpstreamAuthentication
            | Self::UpstreamQuota
            | Self::UpstreamProtocol
            | Self::UpstreamServer
            | Self::UpstreamNetwork => "service_unavailable",
            Self::Internal => "internal_error",
        }
    }

    /// 返回普通调用方可以看到的固定文案；不包含渠道、供应商或上游字样。
    #[must_use]
    pub const fn public_error_message(self) -> &'static str {
        match self {
            Self::InvalidRequest => "Invalid request.",
            Self::ModelNotAllowed | Self::UpstreamModel => {
                "The requested model was not found or is unavailable."
            }
            Self::InsufficientQuota => "Insufficient quota.",
            Self::QuotaLimited | Self::ConcurrencyLimited | Self::UpstreamRateLimited => {
                "Rate limit exceeded. Please retry later."
            }
            Self::OutcomeUnknown => {
                "The request outcome is unknown. Retry with the same idempotency key."
            }
            Self::UpstreamOverloaded
            | Self::UpstreamAuthentication
            | Self::UpstreamQuota
            | Self::UpstreamProtocol
            | Self::UpstreamServer
            | Self::UpstreamNetwork => {
                "The service is temporarily unavailable. Please retry later."
            }
            Self::Internal => "An internal server error occurred.",
        }
    }
}

impl FromStr for RequestFailureKind {
    type Err = RequestOutcomeWriteError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "invalid_request" => Ok(Self::InvalidRequest),
            "model_not_allowed" => Ok(Self::ModelNotAllowed),
            "insufficient_quota" => Ok(Self::InsufficientQuota),
            "quota_limited" => Ok(Self::QuotaLimited),
            "concurrency_limited" => Ok(Self::ConcurrencyLimited),
            "outcome_unknown" => Ok(Self::OutcomeUnknown),
            "upstream_rate_limited" => Ok(Self::UpstreamRateLimited),
            "upstream_overloaded" => Ok(Self::UpstreamOverloaded),
            "upstream_authentication" => Ok(Self::UpstreamAuthentication),
            "upstream_quota" => Ok(Self::UpstreamQuota),
            "upstream_model" => Ok(Self::UpstreamModel),
            "upstream_protocol" => Ok(Self::UpstreamProtocol),
            "upstream_server" => Ok(Self::UpstreamServer),
            "upstream_network" => Ok(Self::UpstreamNetwork),
            "internal" => Ok(Self::Internal),
            _ => Err(RequestOutcomeWriteError::InvalidFailureKind),
        }
    }
}

/// 一条请求终态事实的已校验写入值。
#[derive(Clone, Eq, PartialEq)]
pub struct RequestOutcomeWrite {
    request_id: String,
    protocol: Protocol,
    operation: Operation,
    model: String,
    error_kind: Option<RequestFailureKind>,
    subject: Option<RequestOutcomeSubject>,
    public_error_code: Option<String>,
    public_error_message: Option<String>,
    channel_id: Option<ChannelId>,
    duration_ms: i64,
}

impl RequestOutcomeWrite {
    /// 构造成功请求事实；最终渠道在直连兼容实现中允许为空。
    pub fn succeeded(
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        channel_id: Option<ChannelId>,
        duration: Duration,
    ) -> Result<Self, RequestOutcomeWriteError> {
        Self::new(
            request_id, protocol, operation, model, None, None, None, None, channel_id, duration,
        )
    }

    /// 构造失败请求事实，只接受闭合分类而不接收原始错误文本。
    pub fn failed(
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        error_kind: RequestFailureKind,
        channel_id: Option<ChannelId>,
        duration: Duration,
    ) -> Result<Self, RequestOutcomeWriteError> {
        Self::new(
            request_id,
            protocol,
            operation,
            model,
            Some(error_kind),
            None,
            None,
            None,
            channel_id,
            duration,
        )
    }

    /// 构造带下游主体归属的成功事实。
    pub fn succeeded_for_subject(
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        subject: RequestOutcomeSubject,
        channel_id: Option<ChannelId>,
        duration: Duration,
    ) -> Result<Self, RequestOutcomeWriteError> {
        Self::new(
            request_id,
            protocol,
            operation,
            model,
            None,
            Some(subject),
            None,
            None,
            channel_id,
            duration,
        )
    }

    /// 构造带下游主体和安全公开错误信息的失败事实。
    pub fn failed_for_subject(
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        subject: RequestOutcomeSubject,
        error_kind: RequestFailureKind,
        channel_id: Option<ChannelId>,
        duration: Duration,
    ) -> Result<Self, RequestOutcomeWriteError> {
        Self::new(
            request_id,
            protocol,
            operation,
            model,
            Some(error_kind),
            Some(subject),
            Some(error_kind.public_error_code().to_owned()),
            Some(error_kind.public_error_message().to_owned()),
            channel_id,
            duration,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        error_kind: Option<RequestFailureKind>,
        subject: Option<RequestOutcomeSubject>,
        public_error_code: Option<String>,
        public_error_message: Option<String>,
        channel_id: Option<ChannelId>,
        duration: Duration,
    ) -> Result<Self, RequestOutcomeWriteError> {
        if !valid_text(request_id, MAX_REQUEST_ID_BYTES) {
            return Err(RequestOutcomeWriteError::InvalidRequestId);
        }
        if !valid_text(model, MAX_MODEL_BYTES) {
            return Err(RequestOutcomeWriteError::InvalidModel);
        }
        let duration_ms = i64::try_from(duration.as_millis())
            .map_err(|_| RequestOutcomeWriteError::InvalidDuration)?;
        if public_error_code
            .as_deref()
            .is_some_and(|value| !valid_text(value, MAX_ERROR_CODE_BYTES))
            || public_error_message
                .as_deref()
                .is_some_and(|value| !valid_text(value, MAX_ERROR_MESSAGE_BYTES))
        {
            return Err(RequestOutcomeWriteError::InvalidPublicError);
        }
        Ok(Self {
            request_id: request_id.to_owned(),
            protocol,
            operation,
            model: model.to_owned(),
            error_kind,
            subject,
            public_error_code,
            public_error_message,
            channel_id,
            duration_ms,
        })
    }

    /// 返回当前事实是否表示成功终态。
    #[must_use]
    pub const fn succeeded_outcome(&self) -> bool {
        self.error_kind.is_none()
    }
}

impl fmt::Debug for RequestOutcomeWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestOutcomeWrite")
            .field("protocol", &self.protocol)
            .field("operation", &self.operation)
            .field("error_kind", &self.error_kind)
            .field("has_subject", &self.subject.is_some())
            .field("has_channel", &self.channel_id.is_some())
            .field("duration_ms", &self.duration_ms)
            .finish_non_exhaustive()
    }
}

/// 请求终态写入值校验错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RequestOutcomeWriteError {
    #[error("请求终态请求标识无效")]
    InvalidRequestId,
    #[error("请求终态模型无效")]
    InvalidModel,
    #[error("请求终态耗时无效")]
    InvalidDuration,
    #[error("请求终态失败分类无效")]
    InvalidFailureKind,
    #[error("请求终态公开错误信息无效")]
    InvalidPublicError,
}

/// 幂等追加请求终态事实的结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestOutcomeWriteOutcome {
    Applied,
    Existing,
}

/// 一条失败调用事实的已校验读取值。
pub struct RequestOutcomeRecord {
    id: i64,
    request_id: String,
    protocol: Protocol,
    operation: Operation,
    model: String,
    failure_kind: RequestFailureKind,
    public_error_code: String,
    public_error_message: String,
    user_id: Option<UserId>,
    username: Option<String>,
    token_id: Option<TokenId>,
    group_id: Option<GroupId>,
    organization_id: Option<OrganizationId>,
    organization_team_id: Option<OrganizationTeamId>,
    channel_id: Option<ChannelId>,
    duration_ms: i64,
    created_at: i64,
}

impl RequestOutcomeRecord {
    fn try_from_model(
        model: request_outcome_logs::Model,
        user: Option<users::Model>,
    ) -> Result<Self, RequestOutcomeRepositoryError> {
        if model.id <= 0
            || model.outcome != FAILED_OUTCOME
            || !valid_text(&model.request_id, MAX_REQUEST_ID_BYTES)
            || !valid_text(&model.model, MAX_MODEL_BYTES)
            || model.duration_ms < 0
            || model.channel_id.is_some_and(|value| value <= 0)
            || model.user_id.is_some_and(|value| value <= 0)
            || model.token_id.is_some_and(|value| value <= 0)
            || model.group_id.is_some_and(|value| value <= 0)
            || model.organization_id.is_some_and(|value| value <= 0)
            || model.organization_team_id.is_some_and(|value| value <= 0)
        {
            return Err(internal_error(RequestOutcomeRepositoryError::Invariant));
        }
        let protocol = model
            .protocol
            .parse::<Protocol>()
            .map_err(|_| internal_error(RequestOutcomeRepositoryError::Invariant))?;
        let operation = model
            .operation
            .parse::<Operation>()
            .map_err(|_| internal_error(RequestOutcomeRepositoryError::Invariant))?;
        let failure_kind = model
            .error_kind
            .as_deref()
            .ok_or_else(|| internal_error(RequestOutcomeRepositoryError::Invariant))?
            .parse::<RequestFailureKind>()
            .map_err(|_| internal_error(RequestOutcomeRepositoryError::Invariant))?;
        let (public_error_code, public_error_message) = match (
            model.public_error_code.as_deref(),
            model.public_error_message.as_deref(),
        ) {
            // Rows written before the public error columns were introduced are
            // reconstructed from their closed failure category.
            (None, None) => (
                failure_kind.public_error_code().to_owned(),
                failure_kind.public_error_message().to_owned(),
            ),
            (Some(code), Some(message))
                if valid_text(code, MAX_ERROR_CODE_BYTES)
                    && valid_text(message, MAX_ERROR_MESSAGE_BYTES) =>
            {
                (code.to_owned(), message.to_owned())
            }
            _ => return Err(internal_error(RequestOutcomeRepositoryError::Invariant)),
        };
        let has_user = model.user_id.is_some();
        let has_token = model.token_id.is_some();
        let has_group = model.group_id.is_some();
        let has_organization = model.organization_id.is_some();
        let has_team = model.organization_team_id.is_some();
        if has_user != has_token
            || has_user != has_group
            || (!has_user && (has_organization || has_team))
            || (has_team && !has_organization)
        {
            return Err(internal_error(RequestOutcomeRepositoryError::Invariant));
        }
        let user_id = model
            .user_id
            .map(UserId::new)
            .transpose()
            .map_err(|_| internal_error(RequestOutcomeRepositoryError::Invariant))?;
        if has_user != user.is_some() {
            return Err(internal_error(RequestOutcomeRepositoryError::Invariant));
        }
        let username = user.map(|value| value.username);
        if username.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 64
                || value.trim() != value
                || value.chars().any(char::is_control)
        }) {
            return Err(internal_error(RequestOutcomeRepositoryError::Invariant));
        }
        Ok(Self {
            id: model.id,
            request_id: model.request_id,
            protocol,
            operation,
            model: model.model,
            failure_kind,
            public_error_code,
            public_error_message,
            user_id,
            username,
            token_id: model
                .token_id
                .map(TokenId::new)
                .transpose()
                .map_err(|_| internal_error(RequestOutcomeRepositoryError::Invariant))?,
            group_id: model
                .group_id
                .map(GroupId::new)
                .transpose()
                .map_err(|_| internal_error(RequestOutcomeRepositoryError::Invariant))?,
            organization_id: model
                .organization_id
                .map(OrganizationId::new)
                .transpose()
                .map_err(|_| internal_error(RequestOutcomeRepositoryError::Invariant))?,
            organization_team_id: model
                .organization_team_id
                .map(OrganizationTeamId::new)
                .transpose()
                .map_err(|_| internal_error(RequestOutcomeRepositoryError::Invariant))?,
            channel_id: model
                .channel_id
                .map(ChannelId::new)
                .transpose()
                .map_err(|_| internal_error(RequestOutcomeRepositoryError::Invariant))?,
            duration_ms: model.duration_ms,
            created_at: model.created_at.unix_timestamp(),
        })
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }
    #[must_use]
    pub const fn operation(&self) -> Operation {
        self.operation
    }
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
    #[must_use]
    pub const fn failure_kind(&self) -> RequestFailureKind {
        self.failure_kind
    }
    #[must_use]
    pub fn public_error_code(&self) -> &str {
        &self.public_error_code
    }
    #[must_use]
    pub fn public_error_message(&self) -> &str {
        &self.public_error_message
    }
    #[must_use]
    pub const fn user_id(&self) -> Option<UserId> {
        self.user_id
    }
    #[must_use]
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }
    #[must_use]
    pub const fn token_id(&self) -> Option<TokenId> {
        self.token_id
    }
    #[must_use]
    pub const fn group_id(&self) -> Option<GroupId> {
        self.group_id
    }
    #[must_use]
    pub const fn organization_id(&self) -> Option<OrganizationId> {
        self.organization_id
    }
    #[must_use]
    pub const fn organization_team_id(&self) -> Option<OrganizationTeamId> {
        self.organization_team_id
    }
    #[must_use]
    pub const fn channel_id(&self) -> Option<ChannelId> {
        self.channel_id
    }
    #[must_use]
    pub const fn duration_ms(&self) -> i64 {
        self.duration_ms
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
}

impl fmt::Debug for RequestOutcomeRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RequestOutcomeRecord(<redacted>)")
    }
}

/// 倒序排列的一页失败调用事实。
pub struct RequestOutcomePageRecord {
    logs: Vec<RequestOutcomeRecord>,
    next_cursor: Option<i64>,
}

impl RequestOutcomePageRecord {
    #[must_use]
    pub fn into_parts(self) -> (Vec<RequestOutcomeRecord>, Option<i64>) {
        (self.logs, self.next_cursor)
    }
}

impl fmt::Debug for RequestOutcomePageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RequestOutcomePageRecord(<redacted>)")
    }
}

/// 请求终态事实仓储错误，不携带请求或数据库细节。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RequestOutcomeRepositoryError {
    #[error("请求终态数据库查询失败")]
    Query,
    #[error("请求终态数据库操作超时")]
    Timeout,
    #[error("请求终态幂等事实冲突")]
    Conflict,
    #[error("请求终态持久化状态损坏")]
    Invariant,
}

/// 请求终态事实的只追加、幂等仓储。
#[derive(Clone)]
pub struct RequestOutcomeRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    analytics_export: Option<AnalyticsExportRepository>,
}

impl RequestOutcomeRepository {
    /// 使用共享连接池和固定两秒写入截止时间构造仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
            analytics_export: None,
        }
    }

    /// 开启事实 outbox；请求终态与投递指针会在同一数据库事务提交。
    #[must_use]
    pub fn with_analytics_export(mut self, repository: AnalyticsExportRepository) -> Self {
        self.analytics_export = Some(repository);
        self
    }

    /// 按网关请求标识幂等追加一条终态事实。
    pub async fn record(
        &self,
        write: &RequestOutcomeWrite,
    ) -> Result<RequestOutcomeWriteOutcome, RequestOutcomeRepositoryError> {
        let operation = self
            .record_inner(write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(RequestOutcomeRepositoryError::Timeout)),
        }
    }

    /// 按事实 ID 倒序读取一页失败调用；只读用途不会返回成功终态。
    pub async fn list_failed(
        &self,
        before: Option<i64>,
        limit: usize,
    ) -> Result<RequestOutcomePageRecord, RequestOutcomeRepositoryError> {
        self.list_failed_scoped(None, None, before, limit).await
    }

    /// 读取指定用户自己的失败调用。
    pub async fn list_failed_for_user(
        &self,
        user_id: UserId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<RequestOutcomePageRecord, RequestOutcomeRepositoryError> {
        self.list_failed_scoped(Some(user_id), None, before, limit)
            .await
    }

    /// 按企业和可选成员读取失败调用；企业归属来自请求当时的固化主体。
    pub async fn list_failed_for_organization(
        &self,
        organization_id: OrganizationId,
        user_id: Option<UserId>,
        before: Option<i64>,
        limit: usize,
    ) -> Result<RequestOutcomePageRecord, RequestOutcomeRepositoryError> {
        self.list_failed_scoped(user_id, Some(organization_id), before, limit)
            .await
    }

    async fn list_failed_scoped(
        &self,
        user_id: Option<UserId>,
        organization_id: Option<OrganizationId>,
        before: Option<i64>,
        limit: usize,
    ) -> Result<RequestOutcomePageRecord, RequestOutcomeRepositoryError> {
        if before.is_some_and(|value| value <= 0) || !(1..=100).contains(&limit) {
            return Err(internal_error(RequestOutcomeRepositoryError::Invariant));
        }
        let operation = self
            .query_failed(user_id, organization_id, before, limit)
            .with_subscriber(NoSubscriber::default());
        let mut models = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result?,
            Err(_) => return Err(internal_error(RequestOutcomeRepositoryError::Timeout)),
        };
        let has_more = models.len() > limit;
        if has_more {
            models.truncate(limit);
        }
        let logs = models
            .into_iter()
            .map(|(model, user)| RequestOutcomeRecord::try_from_model(model, user))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| logs.last().map(RequestOutcomeRecord::id))
            .flatten();
        Ok(RequestOutcomePageRecord { logs, next_cursor })
    }

    async fn query_failed(
        &self,
        user_id: Option<UserId>,
        organization_id: Option<OrganizationId>,
        before: Option<i64>,
        limit: usize,
    ) -> Result<
        Vec<(request_outcome_logs::Model, Option<users::Model>)>,
        RequestOutcomeRepositoryError,
    > {
        let mut query = request_outcome_logs::Entity::find()
            .filter(request_outcome_logs::Column::Outcome.eq(FAILED_OUTCOME))
            .order_by_desc(request_outcome_logs::Column::Id)
            .find_also_related(users::Entity)
            .limit((limit + 1) as u64);
        if let Some(user_id) = user_id {
            query = query.filter(request_outcome_logs::Column::UserId.eq(user_id.get()));
        }
        if let Some(organization_id) = organization_id {
            query = query
                .filter(request_outcome_logs::Column::OrganizationId.eq(organization_id.get()));
        }
        if let Some(before) = before {
            query = query.filter(request_outcome_logs::Column::Id.lt(before));
        }
        query
            .all(self.pool.connection())
            .await
            .map_err(|_| internal_error(RequestOutcomeRepositoryError::Query))
    }

    async fn record_inner(
        &self,
        write: &RequestOutcomeWrite,
    ) -> Result<RequestOutcomeWriteOutcome, RequestOutcomeRepositoryError> {
        let model = request_outcome_logs::ActiveModel {
            request_id: Set(write.request_id.clone()),
            protocol: Set(write.protocol.as_str().to_owned()),
            operation: Set(write.operation.as_str().to_owned()),
            model: Set(write.model.clone()),
            outcome: Set(if write.succeeded_outcome() {
                SUCCEEDED_OUTCOME
            } else {
                FAILED_OUTCOME
            }),
            error_kind: Set(write
                .error_kind
                .map(RequestFailureKind::as_str)
                .map(str::to_owned)),
            user_id: Set(write.subject.map(|subject| subject.user_id.get())),
            token_id: Set(write.subject.map(|subject| subject.token_id.get())),
            group_id: Set(write.subject.map(|subject| subject.group_id.get())),
            organization_id: Set(write
                .subject
                .and_then(|subject| subject.organization_id.map(OrganizationId::get))),
            organization_team_id: Set(write
                .subject
                .and_then(|subject| subject.organization_team_id.map(OrganizationTeamId::get))),
            public_error_code: Set(write.public_error_code.clone()),
            public_error_message: Set(write.public_error_message.clone()),
            channel_id: Set(write.channel_id.map(ChannelId::get)),
            duration_ms: Set(write.duration_ms),
            created_at: Set(sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc()),
            ..Default::default()
        };
        if self.analytics_export.is_some() {
            let transaction = self
                .pool
                .connection()
                .begin()
                .await
                .map_err(|_| internal_error(RequestOutcomeRepositoryError::Query))?;
            match model.insert(&transaction).await {
                Ok(saved) => {
                    let result = AnalyticsExportRepository::enqueue_in_transaction(
                        &transaction,
                        AnalyticsExportFactKind::RequestOutcome,
                        saved.id,
                        saved.created_at,
                    )
                    .await;
                    match result {
                        Ok(()) => transaction
                            .commit()
                            .await
                            .map(|_| RequestOutcomeWriteOutcome::Applied)
                            .map_err(|_| internal_error(RequestOutcomeRepositoryError::Query)),
                        Err(_) => {
                            let _ = transaction.rollback().await;
                            Err(internal_error(RequestOutcomeRepositoryError::Query))
                        }
                    }
                }
                Err(error) if is_unique_violation(&error) => {
                    let _ = transaction.rollback().await;
                    self.classify_existing(write).await
                }
                Err(_) => {
                    let _ = transaction.rollback().await;
                    Err(internal_error(RequestOutcomeRepositoryError::Query))
                }
            }
        } else {
            match model.insert(self.pool.connection()).await {
                Ok(_) => Ok(RequestOutcomeWriteOutcome::Applied),
                Err(error) if is_unique_violation(&error) => self.classify_existing(write).await,
                Err(_) => Err(internal_error(RequestOutcomeRepositoryError::Query)),
            }
        }
    }

    async fn classify_existing(
        &self,
        write: &RequestOutcomeWrite,
    ) -> Result<RequestOutcomeWriteOutcome, RequestOutcomeRepositoryError> {
        let existing = request_outcome_logs::Entity::find()
            .filter(request_outcome_logs::Column::RequestId.eq(&write.request_id))
            .one(self.pool.connection())
            .await
            .map_err(|_| internal_error(RequestOutcomeRepositoryError::Query))?
            .ok_or_else(|| internal_error(RequestOutcomeRepositoryError::Invariant))?;
        if matches_write(&existing, write) {
            if let Some(exporter) = self.analytics_export.as_ref() {
                exporter
                    .ensure_pointer(AnalyticsExportFactKind::RequestOutcome, existing.id)
                    .await
                    .map_err(|_| internal_error(RequestOutcomeRepositoryError::Query))?;
            }
            Ok(RequestOutcomeWriteOutcome::Existing)
        } else {
            Err(RequestOutcomeRepositoryError::Conflict)
        }
    }
}

impl fmt::Debug for RequestOutcomeRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestOutcomeRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

fn matches_write(existing: &request_outcome_logs::Model, write: &RequestOutcomeWrite) -> bool {
    existing.request_id == write.request_id
        && existing.protocol == write.protocol.as_str()
        && existing.operation == write.operation.as_str()
        && existing.model == write.model
        && existing.outcome
            == if write.succeeded_outcome() {
                SUCCEEDED_OUTCOME
            } else {
                FAILED_OUTCOME
            }
        && existing.error_kind.as_deref() == write.error_kind.map(RequestFailureKind::as_str)
        && existing.user_id == write.subject.map(|subject| subject.user_id.get())
        && existing.token_id == write.subject.map(|subject| subject.token_id.get())
        && existing.group_id == write.subject.map(|subject| subject.group_id.get())
        && existing.organization_id
            == write
                .subject
                .and_then(|subject| subject.organization_id.map(OrganizationId::get))
        && existing.organization_team_id
            == write
                .subject
                .and_then(|subject| subject.organization_team_id.map(OrganizationTeamId::get))
        && existing.public_error_code == write.public_error_code
        && existing.public_error_message == write.public_error_message
        && existing.channel_id == write.channel_id.map(ChannelId::get)
        && existing.duration_ms == write.duration_ms
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

fn internal_error(error: RequestOutcomeRepositoryError) -> RequestOutcomeRepositoryError {
    let error_kind = match error {
        RequestOutcomeRepositoryError::Query => "request_outcome_query",
        RequestOutcomeRepositoryError::Timeout => "request_outcome_timeout",
        RequestOutcomeRepositoryError::Conflict => return error,
        RequestOutcomeRepositoryError::Invariant => "request_outcome_invariant",
    };
    tracing::error!(
        target: "af_db::request_outcome",
        error_kind,
        "请求终态事实仓储发生内部错误"
    );
    error
}
