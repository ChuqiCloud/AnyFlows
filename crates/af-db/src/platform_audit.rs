use std::{fmt, time::Duration};

use af_domain::UserId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    SqlErr,
};
use serde_json::Value;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{platform_audit_logs, users},
};

/// 平台审计单页允许返回的最大记录数。
pub const MAX_PLATFORM_AUDIT_PAGE_SIZE: usize = 100;
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

/// 平台管理操作的闭合结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum PlatformAuditOutcome {
    /// 操作或读取成功完成。
    Succeeded = 1,
    /// 操作被权限策略拒绝。
    Denied = 2,
    /// 操作进入业务层但未成功完成。
    Failed = 3,
}

impl PlatformAuditOutcome {
    const fn code(self) -> i16 {
        self as i16
    }

    fn from_database(value: i16) -> Result<Self, PlatformAuditRepositoryError> {
        match value {
            1 => Ok(Self::Succeeded),
            2 => Ok(Self::Denied),
            3 => Ok(Self::Failed),
            _ => Err(internal_error(PlatformAuditRepositoryError::Invariant)),
        }
    }
}

/// 已校验的平台管理审计写入事实。
pub struct PlatformAuditWrite {
    operator_user_id: UserId,
    permission_code: String,
    route: String,
    operation: String,
    resource: String,
    resource_id: Option<String>,
    outcome: PlatformAuditOutcome,
    before_value: Option<String>,
    after_value: Option<String>,
    audit_info: Option<String>,
    request_id: String,
}

impl PlatformAuditWrite {
    /// 组合操作者、权限、路由、资源和脱敏结构化上下文。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        operator_user_id: UserId,
        permission_code: String,
        route: String,
        operation: String,
        resource: String,
        resource_id: Option<String>,
        outcome: PlatformAuditOutcome,
        before_value: Option<String>,
        after_value: Option<String>,
        audit_info: Option<String>,
        request_id: String,
    ) -> Result<Self, PlatformAuditWriteError> {
        validate_code(&permission_code, 96)?;
        validate_text(&route, 128)?;
        validate_code(&operation, 96)?;
        validate_code(&resource, 64)?;
        if let Some(resource_id) = resource_id.as_deref() {
            validate_text(resource_id, 128)?;
        }
        for value in [
            before_value.as_deref(),
            after_value.as_deref(),
            audit_info.as_deref(),
        ] {
            validate_json_object(value)?;
        }
        validate_text(&request_id, 128)?;
        Ok(Self {
            operator_user_id,
            permission_code,
            route,
            operation,
            resource,
            resource_id,
            outcome,
            before_value,
            after_value,
            audit_info,
            request_id,
        })
    }
}

impl fmt::Debug for PlatformAuditWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlatformAuditWrite(<redacted>)")
    }
}

/// 平台审计输入错误；不会回显审计内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlatformAuditWriteError {
    /// 权限、路由、动作、资源或请求标识不满足稳定格式。
    #[error("平台审计文本字段无效")]
    InvalidText,
    /// 前后值或审计附加信息不是受限 JSON 对象。
    #[error("平台审计结构化字段无效")]
    InvalidJson,
}

/// 平台审计稳定游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformAuditQuery {
    before: Option<i64>,
    limit: usize,
    operator_user_id: Option<UserId>,
}

impl PlatformAuditQuery {
    /// 校验倒序游标、页大小和可选操作者范围。
    pub fn new(
        before: Option<i64>,
        limit: usize,
        operator_user_id: Option<UserId>,
    ) -> Result<Self, PlatformAuditWriteError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_PLATFORM_AUDIT_PAGE_SIZE).contains(&limit)
        {
            return Err(PlatformAuditWriteError::InvalidText);
        }
        Ok(Self {
            before,
            limit,
            operator_user_id,
        })
    }
}

/// 平台审计读取记录。
pub struct PlatformAuditRecord {
    id: i64,
    operator_user_id: UserId,
    operator_username: String,
    permission_code: String,
    route: String,
    operation: String,
    resource: String,
    resource_id: Option<String>,
    outcome: PlatformAuditOutcome,
    before_value: Option<String>,
    after_value: Option<String>,
    audit_info: Option<String>,
    request_id: String,
    created_at: i64,
}

impl PlatformAuditRecord {
    /// 返回审计主键。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    /// 返回操作者用户标识。
    #[must_use]
    pub const fn operator_user_id(&self) -> UserId {
        self.operator_user_id
    }
    /// 返回操作者当前用户名。
    #[must_use]
    pub fn operator_username(&self) -> &str {
        &self.operator_username
    }
    /// 返回稳定权限码。
    #[must_use]
    pub fn permission_code(&self) -> &str {
        &self.permission_code
    }
    /// 返回规范化路由模板。
    #[must_use]
    pub fn route(&self) -> &str {
        &self.route
    }
    /// 返回稳定操作码。
    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }
    /// 返回资源类型。
    #[must_use]
    pub fn resource(&self) -> &str {
        &self.resource
    }
    /// 返回可选的脱敏资源标识。
    #[must_use]
    pub fn resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }
    /// 返回闭合结果。
    #[must_use]
    pub const fn outcome(&self) -> PlatformAuditOutcome {
        self.outcome
    }
    /// 返回结构化前值。
    #[must_use]
    pub fn before_value(&self) -> Option<&str> {
        self.before_value.as_deref()
    }
    /// 返回结构化后值。
    #[must_use]
    pub fn after_value(&self) -> Option<&str> {
        self.after_value.as_deref()
    }
    /// 返回仅管理员可见的审计附加信息。
    #[must_use]
    pub fn audit_info(&self) -> Option<&str> {
        self.audit_info.as_deref()
    }
    /// 返回服务端请求标识。
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    /// 返回 Unix 秒时间戳。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
}

impl fmt::Debug for PlatformAuditRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlatformAuditRecord(<redacted>)")
    }
}

/// 平台审计稳定游标页。
pub struct PlatformAuditPageRecord {
    records: Vec<PlatformAuditRecord>,
    next_cursor: Option<i64>,
}

impl PlatformAuditPageRecord {
    /// 消费页面并返回记录与下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<PlatformAuditRecord>, Option<i64>) {
        (self.records, self.next_cursor)
    }
}

/// 平台审计幂等追加结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformAuditWriteOutcome {
    /// 首次写入。
    Applied,
    /// 同一请求和动作已保存相同事实。
    Existing,
}

/// 平台审计仓储错误；不携带审计文本或主体。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlatformAuditRepositoryError {
    /// 查询或连接失败。
    #[error("平台审计数据库查询失败")]
    Query,
    /// 操作超时。
    #[error("平台审计数据库操作超时")]
    Timeout,
    /// 相同幂等事实内容冲突。
    #[error("平台审计幂等事实冲突")]
    Conflict,
    /// 持久化状态损坏。
    #[error("平台审计持久化状态损坏")]
    Invariant,
}

/// 平台审计只追加与稳定分页仓储。
#[derive(Clone)]
pub struct PlatformAuditRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl PlatformAuditRepository {
    /// 使用默认五秒截止时间构造仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
        }
    }

    /// 幂等追加一条平台管理审计事实。
    pub async fn record(
        &self,
        write: &PlatformAuditWrite,
    ) -> Result<PlatformAuditWriteOutcome, PlatformAuditRepositoryError> {
        let operation = self
            .record_inner(write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(PlatformAuditRepositoryError::Timeout)),
        }
    }

    /// 按稳定游标和可选操作者范围读取平台审计。
    pub async fn list(
        &self,
        query: &PlatformAuditQuery,
    ) -> Result<PlatformAuditPageRecord, PlatformAuditRepositoryError> {
        let operation = self
            .list_inner(query)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(PlatformAuditRepositoryError::Timeout)),
        }
    }

    async fn record_inner(
        &self,
        write: &PlatformAuditWrite,
    ) -> Result<PlatformAuditWriteOutcome, PlatformAuditRepositoryError> {
        let model = platform_audit_logs::ActiveModel {
            operator_user_id: Set(write.operator_user_id.get()),
            permission_code: Set(write.permission_code.clone()),
            route: Set(write.route.clone()),
            operation: Set(write.operation.clone()),
            resource: Set(write.resource.clone()),
            resource_id: Set(write.resource_id.clone()),
            outcome: Set(write.outcome.code()),
            before_value: Set(write.before_value.clone()),
            after_value: Set(write.after_value.clone()),
            audit_info: Set(write.audit_info.clone()),
            request_id: Set(write.request_id.clone()),
            created_at: Set(sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc()),
            ..Default::default()
        };
        match model.insert(self.pool.connection()).await {
            Ok(_) => Ok(PlatformAuditWriteOutcome::Applied),
            Err(error) if is_unique_violation(&error) => self.classify_existing(write).await,
            Err(_) => Err(internal_error(PlatformAuditRepositoryError::Query)),
        }
    }

    async fn classify_existing(
        &self,
        write: &PlatformAuditWrite,
    ) -> Result<PlatformAuditWriteOutcome, PlatformAuditRepositoryError> {
        let existing = platform_audit_logs::Entity::find()
            .filter(platform_audit_logs::Column::RequestId.eq(&write.request_id))
            .filter(platform_audit_logs::Column::Route.eq(&write.route))
            .filter(platform_audit_logs::Column::Operation.eq(&write.operation))
            .one(self.pool.connection())
            .await
            .map_err(|_| internal_error(PlatformAuditRepositoryError::Query))?
            .ok_or_else(|| internal_error(PlatformAuditRepositoryError::Invariant))?;
        if matches_write(&existing, write) {
            Ok(PlatformAuditWriteOutcome::Existing)
        } else {
            Err(PlatformAuditRepositoryError::Conflict)
        }
    }

    async fn list_inner(
        &self,
        query: &PlatformAuditQuery,
    ) -> Result<PlatformAuditPageRecord, PlatformAuditRepositoryError> {
        let mut select = platform_audit_logs::Entity::find()
            .find_also_related(users::Entity)
            .order_by_desc(platform_audit_logs::Column::Id)
            .limit((query.limit + 1) as u64);
        if let Some(before) = query.before {
            select = select.filter(platform_audit_logs::Column::Id.lt(before));
        }
        if let Some(operator_user_id) = query.operator_user_id {
            select = select
                .filter(platform_audit_logs::Column::OperatorUserId.eq(operator_user_id.get()));
        }
        let mut models = select
            .all(self.pool.connection())
            .await
            .map_err(|_| internal_error(PlatformAuditRepositoryError::Query))?;
        let has_more = models.len() > query.limit;
        if has_more {
            models.truncate(query.limit);
        }
        let records = models
            .into_iter()
            .map(joined_record)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| records.last().map(PlatformAuditRecord::id))
            .flatten();
        Ok(PlatformAuditPageRecord {
            records,
            next_cursor,
        })
    }
}

impl fmt::Debug for PlatformAuditRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlatformAuditRepository(<redacted>)")
    }
}

fn joined_record(
    (model, operator): (platform_audit_logs::Model, Option<users::Model>),
) -> Result<PlatformAuditRecord, PlatformAuditRepositoryError> {
    let operator =
        operator.ok_or_else(|| internal_error(PlatformAuditRepositoryError::Invariant))?;
    let outcome = PlatformAuditOutcome::from_database(model.outcome)?;
    if model.id <= 0
        || model.operator_user_id <= 0
        || operator.id != model.operator_user_id
        || operator.username.is_empty()
        || !valid_code(&model.permission_code, 96)
        || !valid_text(&model.route, 128)
        || !valid_code(&model.operation, 96)
        || !valid_code(&model.resource, 64)
        || model
            .resource_id
            .as_deref()
            .is_some_and(|value| !valid_text(value, 128))
        || !valid_json_object(model.before_value.as_deref())
        || !valid_json_object(model.after_value.as_deref())
        || !valid_json_object(model.audit_info.as_deref())
        || !valid_text(&model.request_id, 128)
    {
        return Err(internal_error(PlatformAuditRepositoryError::Invariant));
    }
    Ok(PlatformAuditRecord {
        id: model.id,
        operator_user_id: UserId::new(model.operator_user_id)
            .map_err(|_| internal_error(PlatformAuditRepositoryError::Invariant))?,
        operator_username: operator.username,
        permission_code: model.permission_code,
        route: model.route,
        operation: model.operation,
        resource: model.resource,
        resource_id: model.resource_id,
        outcome,
        before_value: model.before_value,
        after_value: model.after_value,
        audit_info: model.audit_info,
        request_id: model.request_id,
        created_at: model.created_at.unix_timestamp(),
    })
}

fn matches_write(model: &platform_audit_logs::Model, write: &PlatformAuditWrite) -> bool {
    model.operator_user_id == write.operator_user_id.get()
        && model.permission_code == write.permission_code
        && model.route == write.route
        && model.operation == write.operation
        && model.resource == write.resource
        && model.resource_id == write.resource_id
        && model.outcome == write.outcome.code()
        && model.before_value == write.before_value
        && model.after_value == write.after_value
        && model.audit_info == write.audit_info
        && model.request_id == write.request_id
}

fn validate_code(value: &str, max_length: usize) -> Result<(), PlatformAuditWriteError> {
    valid_code(value, max_length)
        .then_some(())
        .ok_or(PlatformAuditWriteError::InvalidText)
}

fn valid_code(value: &str, max_length: usize) -> bool {
    valid_text(value, max_length)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn validate_text(value: &str, max_length: usize) -> Result<(), PlatformAuditWriteError> {
    valid_text(value, max_length)
        .then_some(())
        .ok_or(PlatformAuditWriteError::InvalidText)
}

fn valid_text(value: &str, max_length: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_length
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn validate_json_object(value: Option<&str>) -> Result<(), PlatformAuditWriteError> {
    valid_json_object(value)
        .then_some(())
        .ok_or(PlatformAuditWriteError::InvalidJson)
}

fn valid_json_object(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        value.len() <= 2_048
            && serde_json::from_str::<Value>(value)
                .ok()
                .is_some_and(|value| value.is_object())
    })
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

fn internal_error(error: PlatformAuditRepositoryError) -> PlatformAuditRepositoryError {
    let error_kind = match error {
        PlatformAuditRepositoryError::Query => "platform_audit_query",
        PlatformAuditRepositoryError::Timeout => "platform_audit_timeout",
        PlatformAuditRepositoryError::Conflict => return error,
        PlatformAuditRepositoryError::Invariant => "platform_audit_invariant",
    };
    tracing::error!(
        target: "af_db::platform_audit",
        error_kind,
        "平台管理审计仓储发生内部错误"
    );
    error
}
