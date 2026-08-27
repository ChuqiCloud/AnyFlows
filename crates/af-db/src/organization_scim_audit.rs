use std::{fmt, time::Duration};

use af_domain::{
    OrganizationId, OrganizationScimAuditEventType, OrganizationScimAuditFacts,
    OrganizationScimAuditReason, OrganizationScimAuditResource, OrganizationScimAuditResult,
    OrganizationScimResourceKind, OrganizationScimTokenKey,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    SqlErr, entity::prelude::TimeDateTimeWithTimeZone,
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{DatabasePool, entity::organization_scim_audit_logs};

/// SCIM 脱敏审计事实的幂等写入结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationScimAuditWriteOutcome {
    /// 首次追加成功。
    Applied,
    /// 相同企业、请求指纹和事件类型已经保存了完全相同的事实。
    Existing,
}

/// SCIM 审计列表的稳定游标与精确筛选条件。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationScimAuditQuery {
    before: Option<i64>,
    limit: usize,
    event_type: Option<OrganizationScimAuditEventType>,
    result: Option<OrganizationScimAuditResult>,
    reason: Option<OrganizationScimAuditReason>,
    token_key: Option<OrganizationScimTokenKey>,
    resource_kind: Option<OrganizationScimResourceKind>,
}

impl OrganizationScimAuditQuery {
    /// 校验游标、页大小和闭合筛选枚举；不接受自由文本条件。
    pub fn new(
        before: Option<i64>,
        limit: usize,
        event_type: Option<OrganizationScimAuditEventType>,
        result: Option<OrganizationScimAuditResult>,
        reason: Option<OrganizationScimAuditReason>,
        token_key: Option<OrganizationScimTokenKey>,
        resource_kind: Option<OrganizationScimResourceKind>,
    ) -> Result<Self, OrganizationScimAuditRepositoryError> {
        if before.is_some_and(|value| value <= 0) || !(1..=100).contains(&limit) {
            return Err(OrganizationScimAuditRepositoryError::Invariant);
        }
        Ok(Self {
            before,
            limit,
            event_type,
            result,
            reason,
            token_key,
            resource_kind,
        })
    }

    #[must_use]
    pub const fn before(&self) -> Option<i64> {
        self.before
    }

    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }

    #[must_use]
    pub const fn event_type(&self) -> Option<OrganizationScimAuditEventType> {
        self.event_type
    }

    #[must_use]
    pub const fn result(&self) -> Option<OrganizationScimAuditResult> {
        self.result
    }

    #[must_use]
    pub const fn reason(&self) -> Option<OrganizationScimAuditReason> {
        self.reason
    }

    #[must_use]
    pub fn token_key(&self) -> Option<&OrganizationScimTokenKey> {
        self.token_key.as_ref()
    }

    #[must_use]
    pub const fn resource_kind(&self) -> Option<OrganizationScimResourceKind> {
        self.resource_kind
    }
}

/// 可供管理端读取的脱敏 SCIM 审计记录。
pub struct OrganizationScimAuditRecord {
    id: i64,
    organization_id: OrganizationId,
    token_key: Option<OrganizationScimTokenKey>,
    event_type: OrganizationScimAuditEventType,
    result: OrganizationScimAuditResult,
    reason: Option<OrganizationScimAuditReason>,
    resource_kind: Option<OrganizationScimResourceKind>,
    resource_digest: Option<String>,
    resource_version: Option<i64>,
    request_fingerprint: String,
    retry_after_seconds: Option<i32>,
    created_at: i64,
}

impl OrganizationScimAuditRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub fn token_key(&self) -> Option<&OrganizationScimTokenKey> {
        self.token_key.as_ref()
    }
    #[must_use]
    pub const fn event_type(&self) -> OrganizationScimAuditEventType {
        self.event_type
    }
    #[must_use]
    pub const fn result(&self) -> OrganizationScimAuditResult {
        self.result
    }
    #[must_use]
    pub const fn reason(&self) -> Option<OrganizationScimAuditReason> {
        self.reason
    }
    #[must_use]
    pub const fn resource_kind(&self) -> Option<OrganizationScimResourceKind> {
        self.resource_kind
    }
    #[must_use]
    pub fn resource_digest(&self) -> Option<&str> {
        self.resource_digest.as_deref()
    }
    #[must_use]
    pub const fn resource_version(&self) -> Option<i64> {
        self.resource_version
    }
    #[must_use]
    pub fn request_fingerprint(&self) -> &str {
        &self.request_fingerprint
    }
    #[must_use]
    pub const fn retry_after_seconds(&self) -> Option<i32> {
        self.retry_after_seconds
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
}

impl fmt::Debug for OrganizationScimAuditRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimAuditRecord(<redacted>)")
    }
}

/// SCIM 审计稳定游标页。
pub struct OrganizationScimAuditPageRecord {
    records: Vec<OrganizationScimAuditRecord>,
    next_cursor: Option<i64>,
}

impl OrganizationScimAuditPageRecord {
    /// 消费页面并返回记录和下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<OrganizationScimAuditRecord>, Option<i64>) {
        (self.records, self.next_cursor)
    }
}

/// SCIM 脱敏审计仓储错误，不携带数据库或请求原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimAuditRepositoryError {
    #[error("SCIM 审计事实查询失败")]
    Query,
    #[error("SCIM 审计事实写入超时")]
    Timeout,
    #[error("SCIM 审计事实幂等冲突")]
    Conflict,
    #[error("SCIM 审计事实持久化状态损坏")]
    Invariant,
}

/// 只追加并按企业范围读取 SCIM 脱敏审计事实的仓储。
#[derive(Clone)]
pub struct OrganizationScimAuditRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationScimAuditRepository {
    /// 创建带超时保护的 SCIM 审计仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationScimAuditRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationScimAuditRepositoryError::Invariant);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 追加一条已由领域层校验过的脱敏事实，重复写入只返回 Existing。
    pub async fn record(
        &self,
        facts: &OrganizationScimAuditFacts,
    ) -> Result<OrganizationScimAuditWriteOutcome, OrganizationScimAuditRepositoryError> {
        timeout(self.operation_timeout, self.record_inner(facts))
            .await
            .map_err(|_| OrganizationScimAuditRepositoryError::Timeout)?
    }

    /// 按企业、稳定游标和闭合筛选读取脱敏审计。
    pub async fn list(
        &self,
        organization_id: OrganizationId,
        query: &OrganizationScimAuditQuery,
    ) -> Result<OrganizationScimAuditPageRecord, OrganizationScimAuditRepositoryError> {
        timeout(
            self.operation_timeout,
            self.list_inner(organization_id, query),
        )
        .await
        .map_err(|_| internal_error(OrganizationScimAuditRepositoryError::Timeout))?
    }

    /// 删除企业保留期之前的 SCIM 审计事实，避免查询返回已过期记录。
    pub async fn prune_before(
        &self,
        organization_id: OrganizationId,
        cutoff_unix_seconds: i64,
    ) -> Result<u64, OrganizationScimAuditRepositoryError> {
        let cutoff = TimeDateTimeWithTimeZone::from_unix_timestamp(cutoff_unix_seconds)
            .map_err(|_| internal_error(OrganizationScimAuditRepositoryError::Invariant))?;
        timeout(
            self.operation_timeout,
            organization_scim_audit_logs::Entity::delete_many()
                .filter(
                    organization_scim_audit_logs::Column::OrganizationId.eq(organization_id.get()),
                )
                .filter(organization_scim_audit_logs::Column::CreatedAt.lt(cutoff))
                .exec(self.pool.connection()),
        )
        .await
        .map_err(|_| internal_error(OrganizationScimAuditRepositoryError::Timeout))?
        .map(|result| result.rows_affected)
        .map_err(|_| internal_error(OrganizationScimAuditRepositoryError::Query))
    }

    async fn record_inner(
        &self,
        facts: &OrganizationScimAuditFacts,
    ) -> Result<OrganizationScimAuditWriteOutcome, OrganizationScimAuditRepositoryError> {
        let (resource_kind, resource_digest, resource_version) = facts
            .resource()
            .map(resource_columns)
            .transpose()?
            .map(|(kind, digest, version)| (Some(kind), Some(digest), Some(version)))
            .unwrap_or((None, None, None));
        let retry_after_seconds = facts
            .retry_after_seconds()
            .map(i32::try_from)
            .transpose()
            .map_err(|_| OrganizationScimAuditRepositoryError::Invariant)?;
        let model = organization_scim_audit_logs::ActiveModel {
            organization_id: Set(facts.organization_id().get()),
            token_key: Set(facts.token_key().map(|value| value.as_str().to_owned())),
            event_type: Set(facts.event_type().code().to_owned()),
            result: Set(facts.result().code().to_owned()),
            reason: Set(facts.reason().map(|value| value.code().to_owned())),
            resource_kind: Set(resource_kind),
            resource_digest: Set(resource_digest),
            resource_version: Set(resource_version),
            request_fingerprint: Set(hex_fingerprint(*facts.request_fingerprint().as_bytes())),
            retry_after_seconds: Set(retry_after_seconds),
            created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
            ..Default::default()
        };
        match model.insert(self.pool.connection()).await {
            Ok(_) => Ok(OrganizationScimAuditWriteOutcome::Applied),
            Err(error) if is_unique_violation(&error) => self.classify_existing(facts).await,
            Err(_) => Err(internal_error(OrganizationScimAuditRepositoryError::Query)),
        }
    }

    async fn classify_existing(
        &self,
        facts: &OrganizationScimAuditFacts,
    ) -> Result<OrganizationScimAuditWriteOutcome, OrganizationScimAuditRepositoryError> {
        let fingerprint = hex_fingerprint(*facts.request_fingerprint().as_bytes());
        let existing = organization_scim_audit_logs::Entity::find()
            .filter(
                organization_scim_audit_logs::Column::OrganizationId
                    .eq(facts.organization_id().get()),
            )
            .filter(organization_scim_audit_logs::Column::RequestFingerprint.eq(&fingerprint))
            .filter(organization_scim_audit_logs::Column::EventType.eq(facts.event_type().code()))
            .one(self.pool.connection())
            .await
            .map_err(|_| internal_error(OrganizationScimAuditRepositoryError::Query))?
            .ok_or_else(|| internal_error(OrganizationScimAuditRepositoryError::Invariant))?;
        if matches_facts(&existing, facts) {
            Ok(OrganizationScimAuditWriteOutcome::Existing)
        } else {
            Err(OrganizationScimAuditRepositoryError::Conflict)
        }
    }

    async fn list_inner(
        &self,
        organization_id: OrganizationId,
        query: &OrganizationScimAuditQuery,
    ) -> Result<OrganizationScimAuditPageRecord, OrganizationScimAuditRepositoryError> {
        let mut select = organization_scim_audit_logs::Entity::find()
            .filter(organization_scim_audit_logs::Column::OrganizationId.eq(organization_id.get()))
            .order_by_desc(organization_scim_audit_logs::Column::Id)
            .limit((query.limit() + 1) as u64);
        if let Some(before) = query.before() {
            select = select.filter(organization_scim_audit_logs::Column::Id.lt(before));
        }
        if let Some(event_type) = query.event_type() {
            select = select
                .filter(organization_scim_audit_logs::Column::EventType.eq(event_type.code()));
        }
        if let Some(result) = query.result() {
            select = select.filter(organization_scim_audit_logs::Column::Result.eq(result.code()));
        }
        if let Some(reason) = query.reason() {
            select = select.filter(organization_scim_audit_logs::Column::Reason.eq(reason.code()));
        }
        if let Some(token_key) = query.token_key() {
            select = select
                .filter(organization_scim_audit_logs::Column::TokenKey.eq(token_key.as_str()));
        }
        if let Some(resource_kind) = query.resource_kind() {
            select = select.filter(
                organization_scim_audit_logs::Column::ResourceKind
                    .eq(resource_kind_code(resource_kind)),
            );
        }
        let mut models = select
            .all(self.pool.connection())
            .await
            .map_err(|_| internal_error(OrganizationScimAuditRepositoryError::Query))?;
        let has_more = models.len() > query.limit();
        if has_more {
            models.truncate(query.limit());
        }
        let records = models
            .into_iter()
            .map(to_record)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| records.last().map(OrganizationScimAuditRecord::id))
            .flatten();
        Ok(OrganizationScimAuditPageRecord {
            records,
            next_cursor,
        })
    }
}

impl fmt::Debug for OrganizationScimAuditRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimAuditRepository(<redacted>)")
    }
}

fn resource_columns(
    resource: &OrganizationScimAuditResource,
) -> Result<(i16, String, i64), OrganizationScimAuditRepositoryError> {
    Ok((
        match resource.kind() {
            OrganizationScimResourceKind::Users => 1,
            OrganizationScimResourceKind::Groups => 2,
        },
        resource.digest().persistence_key(),
        i64::try_from(resource.version().get())
            .map_err(|_| OrganizationScimAuditRepositoryError::Invariant)?,
    ))
}

fn resource_kind_code(kind: OrganizationScimResourceKind) -> i16 {
    match kind {
        OrganizationScimResourceKind::Users => 1,
        OrganizationScimResourceKind::Groups => 2,
    }
}

fn to_record(
    model: organization_scim_audit_logs::Model,
) -> Result<OrganizationScimAuditRecord, OrganizationScimAuditRepositoryError> {
    if model.id <= 0 || model.organization_id <= 0 || !valid_fingerprint(&model.request_fingerprint)
    {
        return Err(internal_error(
            OrganizationScimAuditRepositoryError::Invariant,
        ));
    }
    let organization_id = OrganizationId::new(model.organization_id)
        .map_err(|_| internal_error(OrganizationScimAuditRepositoryError::Invariant))?;
    let event_type = event_type_from_code(&model.event_type)?;
    let result = result_from_code(&model.result)?;
    let reason = model.reason.as_deref().map(reason_from_code).transpose()?;
    let token_key = model
        .token_key
        .as_deref()
        .map(OrganizationScimTokenKey::new)
        .transpose()
        .map_err(|_| internal_error(OrganizationScimAuditRepositoryError::Invariant))?;
    let resource_kind = model
        .resource_kind
        .map(resource_kind_from_code)
        .transpose()?;
    if model.resource_digest.is_some() != resource_kind.is_some()
        || model.resource_version.is_some() != resource_kind.is_some()
        || model
            .resource_digest
            .as_deref()
            .is_some_and(|value| !valid_digest(value))
        || model.resource_version.is_some_and(|value| value <= 0)
        || model.retry_after_seconds.is_some_and(|value| value < 0)
    {
        return Err(internal_error(
            OrganizationScimAuditRepositoryError::Invariant,
        ));
    }
    Ok(OrganizationScimAuditRecord {
        id: model.id,
        organization_id,
        token_key,
        event_type,
        result,
        reason,
        resource_kind,
        resource_digest: model.resource_digest,
        resource_version: model.resource_version,
        request_fingerprint: model.request_fingerprint,
        retry_after_seconds: model.retry_after_seconds,
        created_at: model.created_at.unix_timestamp(),
    })
}

fn event_type_from_code(
    value: &str,
) -> Result<OrganizationScimAuditEventType, OrganizationScimAuditRepositoryError> {
    [
        OrganizationScimAuditEventType::TokenIssued,
        OrganizationScimAuditEventType::TokenRevoked,
        OrganizationScimAuditEventType::RequestAccepted,
        OrganizationScimAuditEventType::RequestRejected,
        OrganizationScimAuditEventType::ResourceSynchronized,
        OrganizationScimAuditEventType::LifecycleTransition,
        OrganizationScimAuditEventType::RateLimitRejected,
    ]
    .into_iter()
    .find(|candidate| candidate.code() == value)
    .ok_or_else(|| internal_error(OrganizationScimAuditRepositoryError::Invariant))
}

fn result_from_code(
    value: &str,
) -> Result<OrganizationScimAuditResult, OrganizationScimAuditRepositoryError> {
    [
        OrganizationScimAuditResult::Applied,
        OrganizationScimAuditResult::Noop,
        OrganizationScimAuditResult::Rejected,
        OrganizationScimAuditResult::Failed,
    ]
    .into_iter()
    .find(|candidate| candidate.code() == value)
    .ok_or_else(|| internal_error(OrganizationScimAuditRepositoryError::Invariant))
}

fn reason_from_code(
    value: &str,
) -> Result<OrganizationScimAuditReason, OrganizationScimAuditRepositoryError> {
    [
        OrganizationScimAuditReason::InvalidToken,
        OrganizationScimAuditReason::TokenRevoked,
        OrganizationScimAuditReason::TokenExpired,
        OrganizationScimAuditReason::CrossOrganization,
        OrganizationScimAuditReason::StaleResource,
        OrganizationScimAuditReason::EntitlementUnavailable,
        OrganizationScimAuditReason::SeatCapacityExceeded,
        OrganizationScimAuditReason::RateLimitExceeded,
        OrganizationScimAuditReason::StorageUnavailable,
        OrganizationScimAuditReason::InvalidRequest,
        OrganizationScimAuditReason::InternalFailure,
    ]
    .into_iter()
    .find(|candidate| candidate.code() == value)
    .ok_or_else(|| internal_error(OrganizationScimAuditRepositoryError::Invariant))
}

fn resource_kind_from_code(
    value: i16,
) -> Result<OrganizationScimResourceKind, OrganizationScimAuditRepositoryError> {
    match value {
        1 => Ok(OrganizationScimResourceKind::Users),
        2 => Ok(OrganizationScimResourceKind::Groups),
        _ => Err(internal_error(
            OrganizationScimAuditRepositoryError::Invariant,
        )),
    }
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_digest(value: &str) -> bool {
    valid_fingerprint(value) && value.bytes().all(|byte| !byte.is_ascii_uppercase())
}

fn matches_facts(
    model: &organization_scim_audit_logs::Model,
    facts: &OrganizationScimAuditFacts,
) -> bool {
    let (resource_kind, resource_digest, resource_version) = facts
        .resource()
        .and_then(|resource| resource_columns(resource).ok())
        .map(|(kind, digest, version)| (Some(kind), Some(digest), Some(version)))
        .unwrap_or((None, None, None));
    model.organization_id == facts.organization_id().get()
        && model.token_key.as_deref() == facts.token_key().map(|value| value.as_str())
        && model.event_type == facts.event_type().code()
        && model.result == facts.result().code()
        && model.reason.as_deref() == facts.reason().map(|value| value.code())
        && model.resource_kind == resource_kind
        && model.resource_digest == resource_digest
        && model.resource_version == resource_version
        && model.request_fingerprint == hex_fingerprint(*facts.request_fingerprint().as_bytes())
        && model.retry_after_seconds
            == facts
                .retry_after_seconds()
                .and_then(|value| i32::try_from(value).ok())
}

fn hex_fingerprint(value: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in value {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

fn internal_error(
    error: OrganizationScimAuditRepositoryError,
) -> OrganizationScimAuditRepositoryError {
    let kind = match error {
        OrganizationScimAuditRepositoryError::Query => "organization_scim_audit_query",
        OrganizationScimAuditRepositoryError::Timeout => "organization_scim_audit_timeout",
        OrganizationScimAuditRepositoryError::Conflict => return error,
        OrganizationScimAuditRepositoryError::Invariant => "organization_scim_audit_invariant",
    };
    tracing::error!(target: "af_db::organization_scim_audit", error_kind = kind, "SCIM 审计仓储发生内部错误");
    error
}
