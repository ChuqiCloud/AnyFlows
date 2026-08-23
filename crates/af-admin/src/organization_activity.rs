use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    AdminUsageLogPageRecord, AdminUsageLogRepository, OrganizationAuditOutcome,
    OrganizationAuditPageRecord, OrganizationAuditQuery, OrganizationAuditRecord,
    OrganizationAuditRepository, OrganizationAuditRepositoryError, OrganizationAuditWrite,
};
use af_domain::{
    OrganizationId, OrganizationPermission, OrganizationRole, PlatformPermission, UserId,
};
use serde_json::Value;
use thiserror::Error;

use crate::{
    AdminUsageLog, AdminUsageLogListQuery, AdminUsageLogReadError, OrganizationAuthorizationError,
    OrganizationAuthorizationService, OrganizationContext, SessionPrincipal, SessionRole,
    UsageLogOperation, UsageLogProtocol,
    auth_challenge_security::{AuthChallengeSecurityKey, AuthChallengeSecurityKeyError},
};

const AUDIT_CLIENT_FINGERPRINT_DOMAIN: &[u8] = b"AnyFlows organization audit client v1";
const MAX_CLIENT_HINT_BYTES: usize = 1_024;
const MAX_PLATFORM_REASON_BYTES: usize = 255;
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

/// 企业调用日志角色裁剪视图。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationUsageLogScope {
    /// Owner、Admin 与 Billing 的企业完整计费明细。
    Organization,
    /// 当前成员自己的企业调用明细。
    SelfOnly,
    /// Viewer 可读取的企业脱敏聚合。
    Summary,
}

/// 企业调用日志脱敏聚合项；不包含成员、Key、请求或渠道标识。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationUsageLogSummary {
    model: Option<String>,
    protocol: Option<UsageLogProtocol>,
    operation: Option<UsageLogOperation>,
    request_count: i64,
    input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    quota: i64,
    duration_ms: i64,
}

impl OrganizationUsageLogSummary {
    /// 返回模型名；旧事实可能为空。
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }
    /// 返回入口协议。
    #[must_use]
    pub const fn protocol(&self) -> Option<UsageLogProtocol> {
        self.protocol
    }
    /// 返回规范化操作。
    #[must_use]
    pub const fn operation(&self) -> Option<UsageLogOperation> {
        self.operation
    }
    /// 返回聚合请求数。
    #[must_use]
    pub const fn request_count(&self) -> i64 {
        self.request_count
    }
    /// 返回输入 token 合计。
    #[must_use]
    pub const fn input_tokens(&self) -> i64 {
        self.input_tokens
    }
    /// 返回输出 token 合计。
    #[must_use]
    pub const fn output_tokens(&self) -> i64 {
        self.output_tokens
    }
    /// 返回推理 token 合计。
    #[must_use]
    pub const fn reasoning_tokens(&self) -> i64 {
        self.reasoning_tokens
    }
    /// 返回额度消耗合计。
    #[must_use]
    pub const fn quota(&self) -> i64 {
        self.quota
    }
    /// 返回总耗时合计；缺少旧观测的记录按零计入。
    #[must_use]
    pub const fn duration_ms(&self) -> i64 {
        self.duration_ms
    }
}

/// 企业调用日志页面内容。
pub enum OrganizationUsageLogRows {
    /// 角色允许读取的逐次调用明细。
    Detailed(Vec<AdminUsageLog>),
    /// 不包含成员或凭据归属的企业聚合。
    Summary(Vec<OrganizationUsageLogSummary>),
}

/// 企业调用日志稳定游标页。
pub struct OrganizationUsageLogPage {
    scope: OrganizationUsageLogScope,
    rows: OrganizationUsageLogRows,
    next_cursor: Option<i64>,
}

impl OrganizationUsageLogPage {
    /// 返回实际应用的角色裁剪范围。
    #[must_use]
    pub const fn scope(&self) -> OrganizationUsageLogScope {
        self.scope
    }
    /// 返回明细或脱敏聚合。
    #[must_use]
    pub const fn rows(&self) -> &OrganizationUsageLogRows {
        &self.rows
    }
    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for OrganizationUsageLogPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationUsageLogPage(<redacted>)")
    }
}

/// HTTP 边界提供的服务端请求标识与非敏感客户端提示。
pub struct OrganizationAuditRequestContext {
    request_id: String,
    client_hint: Vec<u8>,
}

impl OrganizationAuditRequestContext {
    /// 构造审计请求上下文；原始 Header 只作为指纹输入，不会进入持久化层。
    pub fn new(
        request_id: &str,
        client_hint: Option<&str>,
    ) -> Result<Self, OrganizationActivityError> {
        if request_id.is_empty()
            || request_id.len() > 128
            || request_id.trim() != request_id
            || request_id.chars().any(char::is_control)
        {
            return Err(OrganizationActivityError::InvalidInput);
        }
        let client_hint = client_hint.map_or_else(
            || b"<absent>".to_vec(),
            |value| {
                if value.len() <= MAX_CLIENT_HINT_BYTES && !value.chars().any(char::is_control) {
                    value.as_bytes().to_vec()
                } else {
                    b"<invalid>".to_vec()
                }
            },
        );
        Ok(Self {
            request_id: request_id.to_owned(),
            client_hint,
        })
    }
}

impl fmt::Debug for OrganizationAuditRequestContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationAuditRequestContext(<redacted>)")
    }
}

/// 已通过敏感键名拒绝与大小校验的结构化变更摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationAuditChangeSummary(String);

impl OrganizationAuditChangeSummary {
    /// 只接受对象形状，并拒绝可能承载凭据、正文、Header、IP 或邮箱的键名。
    pub fn new(value: Value) -> Result<Self, OrganizationActivityError> {
        let object = value
            .as_object()
            .ok_or(OrganizationActivityError::InvalidInput)?;
        if object.keys().any(|key| sensitive_summary_key(key)) {
            return Err(OrganizationActivityError::InvalidInput);
        }
        let encoded =
            serde_json::to_string(&value).map_err(|_| OrganizationActivityError::Internal)?;
        if encoded.len() > 2_048 {
            return Err(OrganizationActivityError::InvalidInput);
        }
        Ok(Self(encoded))
    }

    fn into_string(self) -> String {
        self.0
    }
}

/// 企业管理审计列表查询。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationAuditListQuery(OrganizationAuditQuery);

impl OrganizationAuditListQuery {
    /// 校验稳定游标和精确筛选。
    pub fn new(
        before: Option<i64>,
        limit: usize,
        actor_user_id: Option<UserId>,
        operation: Option<String>,
        target_type: Option<String>,
        outcome: Option<OrganizationAuditOutcome>,
    ) -> Result<Self, OrganizationActivityError> {
        OrganizationAuditQuery::new(
            before,
            limit,
            actor_user_id,
            operation,
            target_type,
            outcome,
        )
        .map(Self)
        .map_err(|_| OrganizationActivityError::InvalidInput)
    }

    /// 返回页大小。
    #[must_use]
    pub const fn limit(&self) -> usize {
        self.0.limit()
    }
}

/// 应用层企业管理审计记录；Viewer 视图会裁掉敏感目标与平台代理理由。
pub struct OrganizationAuditLog {
    record: OrganizationAuditRecord,
    redacted: bool,
}

impl OrganizationAuditLog {
    /// 返回审计主键。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.record.id()
    }
    /// 返回操作者用户。
    #[must_use]
    pub const fn actor_user_id(&self) -> UserId {
        self.record.actor_user_id()
    }
    /// 返回操作者用户名。
    #[must_use]
    pub fn actor_username(&self) -> &str {
        self.record.actor_username()
    }
    /// 返回企业角色快照；平台代操作为空。
    #[must_use]
    pub const fn actor_role(&self) -> Option<OrganizationRole> {
        self.record.actor_role()
    }
    /// 返回动作权限码。
    #[must_use]
    pub fn permission_code(&self) -> &str {
        self.record.permission_code()
    }
    /// 返回稳定操作码。
    #[must_use]
    pub fn operation(&self) -> &str {
        self.record.operation()
    }
    /// 返回目标类型。
    #[must_use]
    pub fn target_type(&self) -> &str {
        self.record.target_type()
    }
    /// 返回脱敏目标标识；Viewer 固定为空。
    #[must_use]
    pub fn target_identifier(&self) -> Option<&str> {
        (!self.redacted).then(|| self.record.target_identifier())
    }
    /// 返回闭合结果。
    #[must_use]
    pub const fn outcome(&self) -> OrganizationAuditOutcome {
        self.record.outcome()
    }
    /// 判断是否为平台代理操作。
    #[must_use]
    pub const fn platform_proxy(&self) -> bool {
        self.record.platform_proxy()
    }
    /// 返回平台代理理由；Viewer 固定为空。
    #[must_use]
    pub fn platform_reason(&self) -> Option<&str> {
        (!self.redacted)
            .then(|| self.record.platform_reason())
            .flatten()
    }
    /// 返回服务端请求标识。
    #[must_use]
    pub fn request_id(&self) -> &str {
        self.record.request_id()
    }
    /// 返回不可逆客户端指纹；Viewer 固定为空。
    #[must_use]
    pub fn client_fingerprint(&self) -> Option<&str> {
        (!self.redacted).then(|| self.record.client_fingerprint())
    }
    /// 返回结构化脱敏变更摘要；Viewer 固定为空。
    #[must_use]
    pub fn change_summary(&self) -> Option<&str> {
        (!self.redacted)
            .then(|| self.record.change_summary())
            .flatten()
    }
    /// 返回 Unix 秒时间戳。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.record.created_at()
    }
}

impl fmt::Debug for OrganizationAuditLog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationAuditLog(<redacted>)")
    }
}

/// 企业管理审计稳定游标页。
pub struct OrganizationAuditPage {
    records: Vec<OrganizationAuditLog>,
    next_cursor: Option<i64>,
}

impl OrganizationAuditPage {
    /// 返回本页审计记录。
    #[must_use]
    pub fn records(&self) -> &[OrganizationAuditLog] {
        &self.records
    }
    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for OrganizationAuditPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationAuditPage(<redacted>)")
    }
}

/// 企业活动服务错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationActivityError {
    /// 查询、理由或审计上下文输入无效。
    #[error("企业活动请求无效")]
    InvalidInput,
    /// 当前企业角色或平台角色无权访问。
    #[error("企业活动访问权限不足")]
    Forbidden,
    /// 企业路径或成员关系不存在。
    #[error("企业活动资源不存在")]
    NotFound,
    /// 数据库、授权或持久化不变量失败。
    #[error("企业活动内部失败")]
    Internal,
}

pub type OrganizationUsageLogFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationUsageLogPage, OrganizationActivityError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationAuditListFuture<'a> = Pin<
    Box<dyn Future<Output = Result<OrganizationAuditPage, OrganizationActivityError>> + Send + 'a>,
>;
pub type OrganizationAuditRecordFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), OrganizationActivityError>> + Send + 'a>>;

/// 企业调用日志与管理审计应用端口。
pub trait OrganizationActivityService: Send + Sync {
    /// 按企业角色读取调用日志或 Viewer 脱敏汇总。
    fn list_usage<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        scope: OrganizationUsageLogScope,
        query: AdminUsageLogListQuery,
    ) -> OrganizationUsageLogFuture<'a>;

    /// 平台管理员显式说明理由后跨企业读取调用日志。
    fn list_usage_as_platform<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        query: AdminUsageLogListQuery,
        platform_reason: String,
        request: OrganizationAuditRequestContext,
    ) -> OrganizationUsageLogFuture<'a>;

    /// 按企业角色读取管理审计，并执行授权保留策略。
    fn list_audit<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        query: OrganizationAuditListQuery,
        request: OrganizationAuditRequestContext,
    ) -> OrganizationAuditListFuture<'a>;

    /// 平台管理员显式说明理由后跨企业读取管理审计。
    fn list_audit_as_platform<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        query: OrganizationAuditListQuery,
        platform_reason: String,
        request: OrganizationAuditRequestContext,
    ) -> OrganizationAuditListFuture<'a>;

    /// 为已经完成企业授权的管理动作追加结果审计。
    #[allow(clippy::too_many_arguments)]
    fn record_member_action<'a>(
        &'a self,
        context: OrganizationContext,
        request: OrganizationAuditRequestContext,
        permission_code: &'static str,
        operation: &'static str,
        target_type: &'static str,
        target_identifier: String,
        outcome: OrganizationAuditOutcome,
        summary: Option<OrganizationAuditChangeSummary>,
    ) -> OrganizationAuditRecordFuture<'a>;
}

/// 数据库实现的企业调用日志与管理审计服务。
pub struct DatabaseOrganizationActivityService {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    usage_repository: AdminUsageLogRepository,
    audit_repository: OrganizationAuditRepository,
    security_key: AuthChallengeSecurityKey,
}

impl DatabaseOrganizationActivityService {
    /// 绑定企业授权、调用日志、审计仓储与应用级派生密钥。
    pub fn new(
        authorization_service: Arc<dyn OrganizationAuthorizationService>,
        usage_repository: AdminUsageLogRepository,
        audit_repository: OrganizationAuditRepository,
        security_key: Option<&str>,
    ) -> Result<Self, OrganizationActivityServiceConfigError> {
        Ok(Self {
            authorization_service,
            usage_repository,
            audit_repository,
            security_key: AuthChallengeSecurityKey::new(security_key)
                .map_err(map_security_key_error)?,
        })
    }

    async fn member_context(
        &self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
    ) -> Result<OrganizationContext, OrganizationActivityError> {
        self.authorization_service
            .context(
                principal,
                organization_id,
                OrganizationPermission::OverviewRead,
            )
            .await
            .map_err(map_authorization_error)
    }

    async fn read_usage_page(
        &self,
        organization_id: OrganizationId,
        user_id: Option<UserId>,
        query: AdminUsageLogListQuery,
    ) -> Result<AdminUsageLogPageRecord, OrganizationActivityError> {
        match user_id {
            Some(user_id) => {
                self.usage_repository
                    .list_for_organization_user(
                        organization_id,
                        user_id,
                        query.before(),
                        query.limit(),
                    )
                    .await
            }
            None => {
                self.usage_repository
                    .list_for_organization(organization_id, query.before(), query.limit())
                    .await
            }
        }
        .map_err(|_| OrganizationActivityError::Internal)
    }

    fn map_usage_page(
        scope: OrganizationUsageLogScope,
        page: AdminUsageLogPageRecord,
    ) -> Result<OrganizationUsageLogPage, OrganizationActivityError> {
        let (records, next_cursor) = page.into_parts();
        let logs = records
            .into_iter()
            .map(AdminUsageLog::from_record)
            .collect::<Result<Vec<_>, AdminUsageLogReadError>>()
            .map_err(|_| OrganizationActivityError::Internal)?;
        let rows = if scope == OrganizationUsageLogScope::Summary {
            OrganizationUsageLogRows::Summary(summarize_usage(&logs)?)
        } else {
            OrganizationUsageLogRows::Detailed(logs)
        };
        Ok(OrganizationUsageLogPage {
            scope,
            rows,
            next_cursor,
        })
    }

    async fn prune(
        &self,
        organization_id: OrganizationId,
        retention_days: i64,
    ) -> Result<(), OrganizationActivityError> {
        let now = current_unix_timestamp()?;
        let retention_seconds = retention_days
            .checked_mul(SECONDS_PER_DAY)
            .ok_or(OrganizationActivityError::Internal)?;
        let cutoff = now
            .checked_sub(retention_seconds)
            .ok_or(OrganizationActivityError::Internal)?;
        self.audit_repository
            .prune_before(organization_id, cutoff)
            .await
            .map(|_| ())
            .map_err(map_audit_repository_error)
    }

    #[allow(clippy::too_many_arguments)]
    async fn record(
        &self,
        organization_id: OrganizationId,
        actor_user_id: UserId,
        actor_role: Option<OrganizationRole>,
        retention_days: i64,
        request: OrganizationAuditRequestContext,
        permission_code: &'static str,
        operation: &'static str,
        target_type: &'static str,
        target_identifier: String,
        outcome: OrganizationAuditOutcome,
        platform_reason: Option<String>,
        summary: Option<OrganizationAuditChangeSummary>,
    ) -> Result<(), OrganizationActivityError> {
        self.prune(organization_id, retention_days).await?;
        let actor_bytes = actor_user_id.get().to_be_bytes();
        let client_fingerprint = self.security_key.derive(
            AUDIT_CLIENT_FINGERPRINT_DOMAIN,
            &[&actor_bytes, &request.client_hint],
        );
        let write = OrganizationAuditWrite::new(
            organization_id,
            actor_user_id,
            actor_role,
            permission_code.to_owned(),
            operation.to_owned(),
            target_type.to_owned(),
            target_identifier,
            outcome,
            platform_reason,
            request.request_id,
            client_fingerprint,
            summary.map(OrganizationAuditChangeSummary::into_string),
        )
        .map_err(|_| OrganizationActivityError::InvalidInput)?;
        self.audit_repository
            .record(&write)
            .await
            .map(|_| ())
            .map_err(map_audit_repository_error)
    }
}

impl OrganizationActivityService for DatabaseOrganizationActivityService {
    fn list_usage<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        scope: OrganizationUsageLogScope,
        query: AdminUsageLogListQuery,
    ) -> OrganizationUsageLogFuture<'a> {
        Box::pin(async move {
            let context = self.member_context(principal, organization_id).await?;
            match scope {
                OrganizationUsageLogScope::Organization
                    if !context.allows(OrganizationPermission::UsageLogsReadAll) =>
                {
                    return Err(OrganizationActivityError::Forbidden);
                }
                OrganizationUsageLogScope::SelfOnly
                    if !context.allows(OrganizationPermission::UsageLogsReadSelf) =>
                {
                    return Err(OrganizationActivityError::Forbidden);
                }
                OrganizationUsageLogScope::Summary
                    if context.role() != OrganizationRole::Viewer
                        && !context.allows(OrganizationPermission::UsageLogsReadAll) =>
                {
                    return Err(OrganizationActivityError::Forbidden);
                }
                _ => {}
            }
            let user_id =
                (scope == OrganizationUsageLogScope::SelfOnly).then_some(context.user_id());
            let page = self
                .read_usage_page(organization_id, user_id, query)
                .await?;
            Self::map_usage_page(scope, page)
        })
    }

    fn list_usage_as_platform<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        query: AdminUsageLogListQuery,
        platform_reason: String,
        request: OrganizationAuditRequestContext,
    ) -> OrganizationUsageLogFuture<'a> {
        Box::pin(async move {
            require_platform(principal, &platform_reason)?;
            let retention_days = self
                .audit_repository
                .retention_days(organization_id)
                .await
                .map_err(map_audit_repository_error)?;
            let page = self.read_usage_page(organization_id, None, query).await?;
            self.record(
                organization_id,
                principal.user_id(),
                None,
                retention_days,
                request,
                PlatformPermission::OrganizationUsageLogsReadAll.code(),
                "platform.organization_usage_logs.read",
                "organization",
                organization_id.get().to_string(),
                OrganizationAuditOutcome::Succeeded,
                Some(platform_reason),
                None,
            )
            .await?;
            Self::map_usage_page(OrganizationUsageLogScope::Organization, page)
        })
    }

    fn list_audit<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        query: OrganizationAuditListQuery,
        request: OrganizationAuditRequestContext,
    ) -> OrganizationAuditListFuture<'a> {
        Box::pin(async move {
            let context = self.member_context(principal, organization_id).await?;
            if !context.allows(OrganizationPermission::AuditRead) {
                return Err(OrganizationActivityError::Forbidden);
            }
            self.prune(
                organization_id,
                context.entitlement().audit_retention_days(),
            )
            .await?;
            let page = self
                .audit_repository
                .list(organization_id, &query.0)
                .await
                .map_err(map_audit_repository_error)?;
            self.record(
                organization_id,
                context.user_id(),
                Some(context.role()),
                context.entitlement().audit_retention_days(),
                request,
                OrganizationPermission::AuditRead.code(),
                "organization.audit_logs.read",
                "organization",
                organization_id.get().to_string(),
                OrganizationAuditOutcome::Succeeded,
                None,
                None,
            )
            .await?;
            Ok(map_audit_page(
                page,
                context.role() == OrganizationRole::Viewer,
            ))
        })
    }

    fn list_audit_as_platform<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        query: OrganizationAuditListQuery,
        platform_reason: String,
        request: OrganizationAuditRequestContext,
    ) -> OrganizationAuditListFuture<'a> {
        Box::pin(async move {
            require_platform(principal, &platform_reason)?;
            let retention_days = self
                .audit_repository
                .retention_days(organization_id)
                .await
                .map_err(map_audit_repository_error)?;
            self.prune(organization_id, retention_days).await?;
            let page = self
                .audit_repository
                .list(organization_id, &query.0)
                .await
                .map_err(map_audit_repository_error)?;
            self.record(
                organization_id,
                principal.user_id(),
                None,
                retention_days,
                request,
                PlatformPermission::OrganizationAuditReadAll.code(),
                "platform.organization_audit.read",
                "organization",
                organization_id.get().to_string(),
                OrganizationAuditOutcome::Succeeded,
                Some(platform_reason),
                None,
            )
            .await?;
            Ok(map_audit_page(page, false))
        })
    }

    fn record_member_action<'a>(
        &'a self,
        context: OrganizationContext,
        request: OrganizationAuditRequestContext,
        permission_code: &'static str,
        operation: &'static str,
        target_type: &'static str,
        target_identifier: String,
        outcome: OrganizationAuditOutcome,
        summary: Option<OrganizationAuditChangeSummary>,
    ) -> OrganizationAuditRecordFuture<'a> {
        Box::pin(async move {
            self.record(
                context.organization_id(),
                context.user_id(),
                Some(context.role()),
                context.entitlement().audit_retention_days(),
                request,
                permission_code,
                operation,
                target_type,
                target_identifier,
                outcome,
                None,
                summary,
            )
            .await
        })
    }
}

impl fmt::Debug for DatabaseOrganizationActivityService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationActivityService(<redacted>)")
    }
}

/// 企业活动服务启动配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationActivityServiceConfigError {
    /// 缺少应用级派生密钥。
    #[error("企业审计安全派生密钥缺失")]
    MissingSecurityKey,
    /// 应用级派生密钥格式无效。
    #[error("企业审计安全派生密钥无效")]
    InvalidSecurityKey,
}

fn summarize_usage(
    logs: &[AdminUsageLog],
) -> Result<Vec<OrganizationUsageLogSummary>, OrganizationActivityError> {
    let mut summaries = BTreeMap::<
        (
            Option<String>,
            Option<UsageLogProtocol>,
            Option<UsageLogOperation>,
        ),
        OrganizationUsageLogSummary,
    >::new();
    for log in logs {
        let key = (
            log.model().map(str::to_owned),
            log.protocol(),
            log.operation(),
        );
        let entry = summaries
            .entry(key.clone())
            .or_insert_with(|| OrganizationUsageLogSummary {
                model: key.0,
                protocol: key.1,
                operation: key.2,
                request_count: 0,
                input_tokens: 0,
                output_tokens: 0,
                reasoning_tokens: 0,
                quota: 0,
                duration_ms: 0,
            });
        let usage = log.usage();
        entry.request_count = entry
            .request_count
            .checked_add(1)
            .ok_or(OrganizationActivityError::Internal)?;
        entry.input_tokens = entry
            .input_tokens
            .checked_add(usage.input_tokens())
            .ok_or(OrganizationActivityError::Internal)?;
        entry.output_tokens = entry
            .output_tokens
            .checked_add(usage.output_tokens())
            .ok_or(OrganizationActivityError::Internal)?;
        entry.reasoning_tokens = entry
            .reasoning_tokens
            .checked_add(usage.reasoning_tokens())
            .ok_or(OrganizationActivityError::Internal)?;
        entry.quota = entry
            .quota
            .checked_add(log.quota())
            .ok_or(OrganizationActivityError::Internal)?;
        entry.duration_ms = entry
            .duration_ms
            .checked_add(log.duration_ms().unwrap_or_default())
            .ok_or(OrganizationActivityError::Internal)?;
    }
    Ok(summaries.into_values().collect())
}

fn map_audit_page(page: OrganizationAuditPageRecord, redacted: bool) -> OrganizationAuditPage {
    let (records, next_cursor) = page.into_parts();
    OrganizationAuditPage {
        records: records
            .into_iter()
            .map(|record| OrganizationAuditLog { record, redacted })
            .collect(),
        next_cursor,
    }
}

fn sensitive_summary_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "key",
        "password",
        "secret",
        "header",
        "body",
        "ip",
        "address",
        "credential",
        "oauth",
        "email",
    ]
    .iter()
    .any(|marker| key.contains(marker))
}

fn require_platform(
    principal: SessionPrincipal,
    reason: &str,
) -> Result<(), OrganizationActivityError> {
    if principal.role() != SessionRole::Admin {
        return Err(OrganizationActivityError::Forbidden);
    }
    if reason.is_empty()
        || reason.len() > MAX_PLATFORM_REASON_BYTES
        || reason.trim() != reason
        || reason.chars().any(char::is_control)
    {
        return Err(OrganizationActivityError::InvalidInput);
    }
    Ok(())
}

fn current_unix_timestamp() -> Result<i64, OrganizationActivityError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_secs()).ok())
        .ok_or(OrganizationActivityError::Internal)
}

fn map_authorization_error(error: OrganizationAuthorizationError) -> OrganizationActivityError {
    match error {
        OrganizationAuthorizationError::NotFound => OrganizationActivityError::NotFound,
        OrganizationAuthorizationError::Forbidden => OrganizationActivityError::Forbidden,
        OrganizationAuthorizationError::Internal => OrganizationActivityError::Internal,
    }
}

fn map_audit_repository_error(
    error: OrganizationAuditRepositoryError,
) -> OrganizationActivityError {
    let _ = error;
    OrganizationActivityError::Internal
}

fn map_security_key_error(
    error: AuthChallengeSecurityKeyError,
) -> OrganizationActivityServiceConfigError {
    match error {
        AuthChallengeSecurityKeyError::Missing => {
            OrganizationActivityServiceConfigError::MissingSecurityKey
        }
        AuthChallengeSecurityKeyError::Invalid => {
            OrganizationActivityServiceConfigError::InvalidSecurityKey
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_summary_rejects_sensitive_keys() {
        assert!(OrganizationAuditChangeSummary::new(serde_json::json!({"version": 2})).is_ok());
        for key in ["api_key", "request_headers", "client_ip", "email"] {
            assert_eq!(
                OrganizationAuditChangeSummary::new(serde_json::json!({(key): "redacted"})),
                Err(OrganizationActivityError::InvalidInput)
            );
        }
    }

    #[test]
    fn platform_reason_and_request_context_are_bounded() {
        let user = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_platform(user, "support"),
            Err(OrganizationActivityError::Forbidden)
        );
        let admin = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::Admin);
        assert_eq!(
            require_platform(admin, ""),
            Err(OrganizationActivityError::InvalidInput)
        );
        assert!(OrganizationAuditRequestContext::new("request", Some("browser")).is_ok());
    }
}
