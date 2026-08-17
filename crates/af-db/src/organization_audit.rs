use std::{fmt, time::Duration};

use af_domain::{MAX_ORGANIZATION_AUDIT_RETENTION_DAYS, OrganizationId, OrganizationRole, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    SqlErr, entity::prelude::TimeDateTimeWithTimeZone,
};
use serde_json::Value;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{organization_audit_logs, organization_entitlements, organizations, users},
};

/// 企业审计单页允许返回的最大记录数。
pub const MAX_ORGANIZATION_AUDIT_PAGE_SIZE: usize = 100;
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

/// 企业管理审计的闭合结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum OrganizationAuditOutcome {
    /// 操作或读取成功完成。
    Succeeded = 1,
    /// 操作因权限或组织边界被拒绝。
    Denied = 2,
    /// 操作进入业务层但未成功完成。
    Failed = 3,
}

impl OrganizationAuditOutcome {
    const fn code(self) -> i16 {
        self as i16
    }

    fn from_database(value: i16) -> Result<Self, OrganizationAuditRepositoryError> {
        match value {
            1 => Ok(Self::Succeeded),
            2 => Ok(Self::Denied),
            3 => Ok(Self::Failed),
            _ => Err(internal_error(OrganizationAuditRepositoryError::Invariant)),
        }
    }
}

/// 已校验的企业管理审计写入事实。
pub struct OrganizationAuditWrite {
    organization_id: OrganizationId,
    actor_user_id: UserId,
    actor_role: Option<OrganizationRole>,
    permission_code: String,
    operation: String,
    target_type: String,
    target_identifier: String,
    outcome: OrganizationAuditOutcome,
    platform_reason: Option<String>,
    request_id: String,
    client_fingerprint: [u8; 32],
    change_summary: Option<String>,
}

impl OrganizationAuditWrite {
    /// 组合企业、操作者、动作、脱敏目标和请求上下文。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        actor_user_id: UserId,
        actor_role: Option<OrganizationRole>,
        permission_code: String,
        operation: String,
        target_type: String,
        target_identifier: String,
        outcome: OrganizationAuditOutcome,
        platform_reason: Option<String>,
        request_id: String,
        client_fingerprint: [u8; 32],
        change_summary: Option<String>,
    ) -> Result<Self, OrganizationAuditWriteError> {
        validate_code(&permission_code, 96)?;
        validate_code(&operation, 96)?;
        validate_code(&target_type, 32)?;
        validate_text(&target_identifier, 128)?;
        validate_text(&request_id, 128)?;
        if let Some(reason) = platform_reason.as_deref() {
            validate_text(reason, 255)?;
        }
        if let Some(summary) = change_summary.as_deref()
            && (summary.len() > 2_048
                || serde_json::from_str::<Value>(summary)
                    .ok()
                    .and_then(|value| value.as_object().map(|_| ()))
                    .is_none())
        {
            return Err(OrganizationAuditWriteError::InvalidChangeSummary);
        }
        if (actor_role.is_none()) != platform_reason.is_some() {
            return Err(OrganizationAuditWriteError::InvalidActorShape);
        }
        Ok(Self {
            organization_id,
            actor_user_id,
            actor_role,
            permission_code,
            operation,
            target_type,
            target_identifier,
            outcome,
            platform_reason,
            request_id,
            client_fingerprint,
            change_summary,
        })
    }
}

impl fmt::Debug for OrganizationAuditWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationAuditWrite(<redacted>)")
    }
}

/// 企业审计写入参数错误；不回显外部文本。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationAuditWriteError {
    /// 权限码、操作码、目标或请求标识无效。
    #[error("企业审计文本字段无效")]
    InvalidText,
    /// 平台代操作与企业角色快照形状不一致。
    #[error("企业审计操作者形状无效")]
    InvalidActorShape,
    /// 变更摘要不是受限 JSON 对象。
    #[error("企业审计变更摘要无效")]
    InvalidChangeSummary,
}

/// 企业审计稳定游标和筛选条件。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationAuditQuery {
    before: Option<i64>,
    limit: usize,
    actor_user_id: Option<UserId>,
    operation: Option<String>,
    target_type: Option<String>,
    outcome: Option<OrganizationAuditOutcome>,
}

impl OrganizationAuditQuery {
    /// 校验游标、页大小和精确筛选文本。
    pub fn new(
        before: Option<i64>,
        limit: usize,
        actor_user_id: Option<UserId>,
        operation: Option<String>,
        target_type: Option<String>,
        outcome: Option<OrganizationAuditOutcome>,
    ) -> Result<Self, OrganizationAuditWriteError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_ORGANIZATION_AUDIT_PAGE_SIZE).contains(&limit)
        {
            return Err(OrganizationAuditWriteError::InvalidText);
        }
        if let Some(operation) = operation.as_deref() {
            validate_code(operation, 96)?;
        }
        if let Some(target_type) = target_type.as_deref() {
            validate_code(target_type, 32)?;
        }
        Ok(Self {
            before,
            limit,
            actor_user_id,
            operation,
            target_type,
            outcome,
        })
    }

    /// 返回倒序日志 ID 游标。
    #[must_use]
    pub const fn before(&self) -> Option<i64> {
        self.before
    }

    /// 返回单页上限。
    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }
}

/// 企业审计读取记录。
pub struct OrganizationAuditRecord {
    id: i64,
    organization_id: OrganizationId,
    actor_user_id: UserId,
    actor_username: String,
    actor_role: Option<OrganizationRole>,
    permission_code: String,
    operation: String,
    target_type: String,
    target_identifier: String,
    outcome: OrganizationAuditOutcome,
    platform_reason: Option<String>,
    request_id: String,
    client_fingerprint: String,
    change_summary: Option<String>,
    created_at: i64,
}

impl OrganizationAuditRecord {
    /// 返回审计主键。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    /// 返回审计企业。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    /// 返回操作者用户。
    #[must_use]
    pub const fn actor_user_id(&self) -> UserId {
        self.actor_user_id
    }
    /// 返回操作者当前用户名。
    #[must_use]
    pub fn actor_username(&self) -> &str {
        &self.actor_username
    }
    /// 返回企业角色快照；平台代操作为空。
    #[must_use]
    pub const fn actor_role(&self) -> Option<OrganizationRole> {
        self.actor_role
    }
    /// 返回本次动作对应的权限码。
    #[must_use]
    pub fn permission_code(&self) -> &str {
        &self.permission_code
    }
    /// 返回稳定操作码。
    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }
    /// 返回脱敏目标类型。
    #[must_use]
    pub fn target_type(&self) -> &str {
        &self.target_type
    }
    /// 返回脱敏目标标识。
    #[must_use]
    pub fn target_identifier(&self) -> &str {
        &self.target_identifier
    }
    /// 返回闭合操作结果。
    #[must_use]
    pub const fn outcome(&self) -> OrganizationAuditOutcome {
        self.outcome
    }
    /// 判断是否为平台代理操作。
    #[must_use]
    pub const fn platform_proxy(&self) -> bool {
        self.actor_role.is_none()
    }
    /// 返回平台代理理由。
    #[must_use]
    pub fn platform_reason(&self) -> Option<&str> {
        self.platform_reason.as_deref()
    }
    /// 返回服务端请求标识。
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    /// 返回不可逆客户端指纹。
    #[must_use]
    pub fn client_fingerprint(&self) -> &str {
        &self.client_fingerprint
    }
    /// 返回结构化脱敏变更摘要。
    #[must_use]
    pub fn change_summary(&self) -> Option<&str> {
        self.change_summary.as_deref()
    }
    /// 返回 Unix 秒时间戳。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
}

impl fmt::Debug for OrganizationAuditRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationAuditRecord(<redacted>)")
    }
}

/// 企业审计稳定游标页。
pub struct OrganizationAuditPageRecord {
    records: Vec<OrganizationAuditRecord>,
    next_cursor: Option<i64>,
}

impl OrganizationAuditPageRecord {
    /// 消费页面并返回记录与下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<OrganizationAuditRecord>, Option<i64>) {
        (self.records, self.next_cursor)
    }
}

/// 企业审计幂等追加结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationAuditWriteOutcome {
    /// 首次写入。
    Applied,
    /// 同一请求和操作已保存相同事实。
    Existing,
}

/// 企业审计仓储错误；不携带审计文本或主体。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationAuditRepositoryError {
    /// 查询或连接失败。
    #[error("企业审计数据库查询失败")]
    Query,
    /// 操作超时。
    #[error("企业审计数据库操作超时")]
    Timeout,
    /// 相同幂等事实内容冲突。
    #[error("企业审计幂等事实冲突")]
    Conflict,
    /// 持久化状态损坏。
    #[error("企业审计持久化状态损坏")]
    Invariant,
}

/// 企业审计只追加与稳定分页仓储。
#[derive(Clone)]
pub struct OrganizationAuditRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationAuditRepository {
    /// 使用默认五秒截止时间构造仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
        }
    }

    /// 幂等追加一条企业管理审计事实。
    pub async fn record(
        &self,
        write: &OrganizationAuditWrite,
    ) -> Result<OrganizationAuditWriteOutcome, OrganizationAuditRepositoryError> {
        let operation = self
            .record_inner(write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(OrganizationAuditRepositoryError::Timeout)),
        }
    }

    /// 按企业、稳定游标和精确筛选读取审计。
    pub async fn list(
        &self,
        organization_id: OrganizationId,
        query: &OrganizationAuditQuery,
    ) -> Result<OrganizationAuditPageRecord, OrganizationAuditRepositoryError> {
        let operation = self
            .list_inner(organization_id, query)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(OrganizationAuditRepositoryError::Timeout)),
        }
    }

    /// 删除单个企业超出授权保留期的管理审计；账务和用量事实不受影响。
    pub async fn prune_before(
        &self,
        organization_id: OrganizationId,
        cutoff_unix_seconds: i64,
    ) -> Result<u64, OrganizationAuditRepositoryError> {
        let cutoff = TimeDateTimeWithTimeZone::from_unix_timestamp(cutoff_unix_seconds)
            .map_err(|_| internal_error(OrganizationAuditRepositoryError::Invariant))?;
        let operation = organization_audit_logs::Entity::delete_many()
            .filter(organization_audit_logs::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_audit_logs::Column::CreatedAt.lt(cutoff))
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(Ok(result)) => Ok(result.rows_affected),
            Ok(Err(_)) => Err(internal_error(OrganizationAuditRepositoryError::Query)),
            Err(_) => Err(internal_error(OrganizationAuditRepositoryError::Timeout)),
        }
    }

    /// 读取平台跨企业访问所需的授权保留天数，并确认企业与授权事实存在。
    pub async fn retention_days(
        &self,
        organization_id: OrganizationId,
    ) -> Result<i64, OrganizationAuditRepositoryError> {
        let operation = async {
            let organization_exists = organizations::Entity::find_by_id(organization_id.get())
                .select_only()
                .column(organizations::Column::Id)
                .into_tuple::<i64>()
                .one(self.pool.connection())
                .await
                .map_err(|_| internal_error(OrganizationAuditRepositoryError::Query))?
                .is_some();
            if !organization_exists {
                return Err(OrganizationAuditRepositoryError::Invariant);
            }
            let retention_days = organization_entitlements::Entity::find()
                .filter(organization_entitlements::Column::OrganizationId.eq(organization_id.get()))
                .select_only()
                .column(organization_entitlements::Column::AuditRetentionDays)
                .into_tuple::<i64>()
                .one(self.pool.connection())
                .await
                .map_err(|_| internal_error(OrganizationAuditRepositoryError::Query))?
                .ok_or_else(|| internal_error(OrganizationAuditRepositoryError::Invariant))?;
            if !(1..=MAX_ORGANIZATION_AUDIT_RETENTION_DAYS).contains(&retention_days) {
                return Err(internal_error(OrganizationAuditRepositoryError::Invariant));
            }
            Ok(retention_days)
        }
        .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(OrganizationAuditRepositoryError::Timeout)),
        }
    }

    async fn record_inner(
        &self,
        write: &OrganizationAuditWrite,
    ) -> Result<OrganizationAuditWriteOutcome, OrganizationAuditRepositoryError> {
        let model = organization_audit_logs::ActiveModel {
            organization_id: Set(write.organization_id.get()),
            actor_user_id: Set(write.actor_user_id.get()),
            actor_role: Set(write.actor_role.map(OrganizationRole::code)),
            permission_code: Set(write.permission_code.clone()),
            operation: Set(write.operation.clone()),
            target_type: Set(write.target_type.clone()),
            target_identifier: Set(write.target_identifier.clone()),
            outcome: Set(write.outcome.code()),
            platform_proxy: Set(write.actor_role.is_none()),
            platform_reason: Set(write.platform_reason.clone()),
            request_id: Set(write.request_id.clone()),
            client_fingerprint: Set(hex_fingerprint(write.client_fingerprint)),
            change_summary: Set(write.change_summary.clone()),
            created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
            ..Default::default()
        };
        match model.insert(self.pool.connection()).await {
            Ok(_) => Ok(OrganizationAuditWriteOutcome::Applied),
            Err(error) if is_unique_violation(&error) => self.classify_existing(write).await,
            Err(_) => Err(internal_error(OrganizationAuditRepositoryError::Query)),
        }
    }

    async fn classify_existing(
        &self,
        write: &OrganizationAuditWrite,
    ) -> Result<OrganizationAuditWriteOutcome, OrganizationAuditRepositoryError> {
        let existing = organization_audit_logs::Entity::find()
            .filter(organization_audit_logs::Column::OrganizationId.eq(write.organization_id.get()))
            .filter(organization_audit_logs::Column::RequestId.eq(&write.request_id))
            .filter(organization_audit_logs::Column::Operation.eq(&write.operation))
            .one(self.pool.connection())
            .await
            .map_err(|_| internal_error(OrganizationAuditRepositoryError::Query))?
            .ok_or_else(|| internal_error(OrganizationAuditRepositoryError::Invariant))?;
        if matches_write(&existing, write) {
            Ok(OrganizationAuditWriteOutcome::Existing)
        } else {
            Err(OrganizationAuditRepositoryError::Conflict)
        }
    }

    async fn list_inner(
        &self,
        organization_id: OrganizationId,
        query: &OrganizationAuditQuery,
    ) -> Result<OrganizationAuditPageRecord, OrganizationAuditRepositoryError> {
        let mut select = organization_audit_logs::Entity::find()
            .find_also_related(users::Entity)
            .filter(organization_audit_logs::Column::OrganizationId.eq(organization_id.get()))
            .order_by_desc(organization_audit_logs::Column::Id)
            .limit((query.limit + 1) as u64);
        if let Some(before) = query.before {
            select = select.filter(organization_audit_logs::Column::Id.lt(before));
        }
        if let Some(actor_user_id) = query.actor_user_id {
            select =
                select.filter(organization_audit_logs::Column::ActorUserId.eq(actor_user_id.get()));
        }
        if let Some(operation) = query.operation.as_deref() {
            select = select.filter(organization_audit_logs::Column::Operation.eq(operation));
        }
        if let Some(target_type) = query.target_type.as_deref() {
            select = select.filter(organization_audit_logs::Column::TargetType.eq(target_type));
        }
        if let Some(outcome) = query.outcome {
            select = select.filter(organization_audit_logs::Column::Outcome.eq(outcome.code()));
        }
        let mut models = select
            .all(self.pool.connection())
            .await
            .map_err(|_| internal_error(OrganizationAuditRepositoryError::Query))?;
        let has_more = models.len() > query.limit;
        if has_more {
            models.truncate(query.limit);
        }
        let records = models
            .into_iter()
            .map(joined_record)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| records.last().map(OrganizationAuditRecord::id))
            .flatten();
        Ok(OrganizationAuditPageRecord {
            records,
            next_cursor,
        })
    }
}

impl fmt::Debug for OrganizationAuditRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationAuditRepository(<redacted>)")
    }
}

fn joined_record(
    (model, actor): (organization_audit_logs::Model, Option<users::Model>),
) -> Result<OrganizationAuditRecord, OrganizationAuditRepositoryError> {
    let actor = actor.ok_or_else(|| internal_error(OrganizationAuditRepositoryError::Invariant))?;
    let actor_role = model
        .actor_role
        .map(OrganizationRole::try_from)
        .transpose()
        .map_err(|_| internal_error(OrganizationAuditRepositoryError::Invariant))?;
    let outcome = OrganizationAuditOutcome::from_database(model.outcome)?;
    if model.id <= 0
        || model.organization_id <= 0
        || model.actor_user_id <= 0
        || actor.id != model.actor_user_id
        || actor.username.is_empty()
        || !valid_code(&model.permission_code, 96)
        || !valid_code(&model.operation, 96)
        || !valid_code(&model.target_type, 32)
        || !valid_text(&model.target_identifier, 128)
        || !valid_text(&model.request_id, 128)
        || !valid_fingerprint(&model.client_fingerprint)
        || ((actor_role.is_none()) != model.platform_reason.is_some())
        || model.platform_proxy != actor_role.is_none()
        || model
            .platform_reason
            .as_deref()
            .is_some_and(|value| !valid_text(value, 255))
        || model.change_summary.as_deref().is_some_and(|value| {
            value.len() > 2_048
                || serde_json::from_str::<Value>(value)
                    .ok()
                    .and_then(|value| value.as_object().map(|_| ()))
                    .is_none()
        })
    {
        return Err(internal_error(OrganizationAuditRepositoryError::Invariant));
    }
    Ok(OrganizationAuditRecord {
        id: model.id,
        organization_id: OrganizationId::new(model.organization_id)
            .map_err(|_| internal_error(OrganizationAuditRepositoryError::Invariant))?,
        actor_user_id: UserId::new(model.actor_user_id)
            .map_err(|_| internal_error(OrganizationAuditRepositoryError::Invariant))?,
        actor_username: actor.username,
        actor_role,
        permission_code: model.permission_code,
        operation: model.operation,
        target_type: model.target_type,
        target_identifier: model.target_identifier,
        outcome,
        platform_reason: model.platform_reason,
        request_id: model.request_id,
        client_fingerprint: model.client_fingerprint,
        change_summary: model.change_summary,
        created_at: model.created_at.unix_timestamp(),
    })
}

fn matches_write(model: &organization_audit_logs::Model, write: &OrganizationAuditWrite) -> bool {
    model.organization_id == write.organization_id.get()
        && model.actor_user_id == write.actor_user_id.get()
        && model.actor_role == write.actor_role.map(OrganizationRole::code)
        && model.permission_code == write.permission_code
        && model.operation == write.operation
        && model.target_type == write.target_type
        && model.target_identifier == write.target_identifier
        && model.outcome == write.outcome.code()
        && model.platform_proxy == write.actor_role.is_none()
        && model.platform_reason == write.platform_reason
        && model.request_id == write.request_id
        && model.client_fingerprint == hex_fingerprint(write.client_fingerprint)
        && model.change_summary == write.change_summary
}

fn validate_code(value: &str, max_length: usize) -> Result<(), OrganizationAuditWriteError> {
    valid_code(value, max_length)
        .then_some(())
        .ok_or(OrganizationAuditWriteError::InvalidText)
}

fn valid_code(value: &str, max_length: usize) -> bool {
    valid_text(value, max_length)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn validate_text(value: &str, max_length: usize) -> Result<(), OrganizationAuditWriteError> {
    valid_text(value, max_length)
        .then_some(())
        .ok_or(OrganizationAuditWriteError::InvalidText)
}

fn valid_text(value: &str, max_length: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_length
        && value.trim() == value
        && !value.chars().any(char::is_control)
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

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

fn internal_error(error: OrganizationAuditRepositoryError) -> OrganizationAuditRepositoryError {
    let error_kind = match error {
        OrganizationAuditRepositoryError::Query => "organization_audit_query",
        OrganizationAuditRepositoryError::Timeout => "organization_audit_timeout",
        OrganizationAuditRepositoryError::Conflict => return error,
        OrganizationAuditRepositoryError::Invariant => "organization_audit_invariant",
    };
    tracing::error!(
        target: "af_db::organization_audit",
        error_kind,
        "企业管理审计仓储发生内部错误"
    );
    error
}
