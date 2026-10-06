use std::{fmt, future::Future, pin::Pin};

use af_db::{
    PlatformAuditOutcome, PlatformAuditQuery, PlatformAuditRecord, PlatformAuditRepository,
    PlatformAuditRepositoryError, PlatformAuditWrite,
};
use af_domain::{PlatformPermission, UserId};
use serde_json::Value;
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 平台管理审计默认页大小。
pub const DEFAULT_PLATFORM_AUDIT_PAGE_SIZE: usize = 50;

/// 固定平台角色策略；权限码与角色判断只在该入口组合。
pub struct PlatformPolicy;

impl PlatformPolicy {
    /// 判断会话主体是否拥有指定平台权限。
    #[must_use]
    pub const fn allows(principal: SessionPrincipal, _permission: PlatformPermission) -> bool {
        matches!(principal.role(), SessionRole::Admin)
    }

    /// 要求会话主体拥有指定平台权限。
    pub const fn authorize(
        principal: SessionPrincipal,
        permission: PlatformPermission,
    ) -> Result<(), PlatformAuditError> {
        if Self::allows(principal, permission) {
            Ok(())
        } else {
            Err(PlatformAuditError::Forbidden)
        }
    }
}

/// 平台审计读取范围。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformAuditScope {
    /// 平台管理员完整视图。
    All,
    /// 当前操作者自己的裁剪视图。
    SelfOnly,
}

/// 已校验的平台审计稳定游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformAuditListQuery {
    before: Option<i64>,
    limit: usize,
}

impl PlatformAuditListQuery {
    /// 校验倒序游标和页大小。
    pub fn new(before: Option<i64>, limit: usize) -> Result<Self, PlatformAuditError> {
        PlatformAuditQuery::new(before, limit, None)
            .map_err(|_| PlatformAuditError::InvalidInput)?;
        Ok(Self { before, limit })
    }

    /// 返回倒序游标。
    #[must_use]
    pub const fn before(self) -> Option<i64> {
        self.before
    }

    /// 返回页大小。
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for PlatformAuditListQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: DEFAULT_PLATFORM_AUDIT_PAGE_SIZE,
        }
    }
}

/// 单次平台管理动作的受限审计事实。
pub struct PlatformAuditEntry {
    write: PlatformAuditWrite,
}

impl PlatformAuditEntry {
    /// 组合权限、路由、资源、结果和结构化前后值。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        principal: SessionPrincipal,
        permission: PlatformPermission,
        route: &'static str,
        operation: &'static str,
        resource: &'static str,
        resource_id: Option<String>,
        outcome: PlatformAuditOutcome,
        before_value: Option<Value>,
        after_value: Option<Value>,
        audit_info: Option<Value>,
        request_id: &str,
    ) -> Result<Self, PlatformAuditError> {
        let write = PlatformAuditWrite::new(
            principal.user_id(),
            permission.code().to_owned(),
            route.to_owned(),
            operation.to_owned(),
            resource.to_owned(),
            resource_id,
            outcome,
            serialize_optional_object(before_value)?,
            serialize_optional_object(after_value)?,
            serialize_optional_object(audit_info)?,
            request_id.to_owned(),
        )
        .map_err(|_| PlatformAuditError::InvalidInput)?;
        Ok(Self { write })
    }
}

impl fmt::Debug for PlatformAuditEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlatformAuditEntry(<redacted>)")
    }
}

/// 应用层平台管理审计记录；自有视图会裁掉操作者名、前后值和 `audit_info`。
pub struct PlatformAuditLog {
    record: PlatformAuditRecord,
    redacted: bool,
}

impl PlatformAuditLog {
    /// 返回审计主键。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.record.id()
    }
    /// 返回操作者用户标识。
    #[must_use]
    pub const fn operator_user_id(&self) -> UserId {
        self.record.operator_user_id()
    }
    /// 返回管理员视图中的操作者用户名。
    #[must_use]
    pub fn operator_username(&self) -> Option<&str> {
        (!self.redacted).then(|| self.record.operator_username())
    }
    /// 返回权限码。
    #[must_use]
    pub fn permission_code(&self) -> &str {
        self.record.permission_code()
    }
    /// 返回规范化路由模板。
    #[must_use]
    pub fn route(&self) -> &str {
        self.record.route()
    }
    /// 返回稳定操作码。
    #[must_use]
    pub fn operation(&self) -> &str {
        self.record.operation()
    }
    /// 返回资源类型。
    #[must_use]
    pub fn resource(&self) -> &str {
        self.record.resource()
    }
    /// 返回可选资源标识。
    #[must_use]
    pub fn resource_id(&self) -> Option<&str> {
        self.record.resource_id()
    }
    /// 返回闭合结果。
    #[must_use]
    pub const fn outcome(&self) -> PlatformAuditOutcome {
        self.record.outcome()
    }
    /// 返回管理员视图中的结构化前值。
    #[must_use]
    pub fn before_value(&self) -> Option<&str> {
        (!self.redacted)
            .then(|| self.record.before_value())
            .flatten()
    }
    /// 返回管理员视图中的结构化后值。
    #[must_use]
    pub fn after_value(&self) -> Option<&str> {
        (!self.redacted)
            .then(|| self.record.after_value())
            .flatten()
    }
    /// 返回管理员视图中的结构化审计附加信息。
    #[must_use]
    pub fn audit_info(&self) -> Option<&str> {
        (!self.redacted).then(|| self.record.audit_info()).flatten()
    }
    /// 返回服务端请求标识。
    #[must_use]
    pub fn request_id(&self) -> &str {
        self.record.request_id()
    }
    /// 返回 Unix 秒时间戳。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.record.created_at()
    }
}

/// 平台管理审计稳定游标页。
pub struct PlatformAuditPage {
    records: Vec<PlatformAuditLog>,
    next_cursor: Option<i64>,
}

impl PlatformAuditPage {
    /// 返回当前页审计记录。
    #[must_use]
    pub fn records(&self) -> &[PlatformAuditLog] {
        &self.records
    }
    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

/// 平台权限与审计服务错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlatformAuditError {
    /// 查询、路由或结构化上下文无效。
    #[error("平台审计输入无效")]
    InvalidInput,
    /// 当前会话没有目标平台权限。
    #[error("当前会话无平台权限")]
    Forbidden,
    /// 审计持久化或读取失败。
    #[error("平台审计内部失败")]
    Internal,
}

/// 平台审计写入 Future。
pub type PlatformAuditRecordFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), PlatformAuditError>> + Send + 'a>>;
/// 平台审计列表 Future。
pub type PlatformAuditListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PlatformAuditPage, PlatformAuditError>> + Send + 'a>>;

/// 平台权限与管理审计端口。
pub trait PlatformAuditService: Send + Sync {
    /// 写入已校验的只追加审计事实。
    fn record<'a>(&'a self, entry: PlatformAuditEntry) -> PlatformAuditRecordFuture<'a>;

    /// 按平台全量或当前操作者范围读取审计。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        scope: PlatformAuditScope,
        query: PlatformAuditListQuery,
    ) -> PlatformAuditListFuture<'a>;
}

/// 使用数据库只追加仓储的平台权限与审计服务。
pub struct DatabasePlatformAuditService {
    repository: PlatformAuditRepository,
}

impl DatabasePlatformAuditService {
    /// 绑定平台审计仓储。
    #[must_use]
    pub const fn new(repository: PlatformAuditRepository) -> Self {
        Self { repository }
    }
}

impl PlatformAuditService for DatabasePlatformAuditService {
    fn record<'a>(&'a self, entry: PlatformAuditEntry) -> PlatformAuditRecordFuture<'a> {
        Box::pin(async move {
            self.repository
                .record(&entry.write)
                .await
                .map(|_| ())
                .map_err(map_repository_error)
        })
    }

    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        scope: PlatformAuditScope,
        query: PlatformAuditListQuery,
    ) -> PlatformAuditListFuture<'a> {
        Box::pin(async move {
            let operator_user_id = match scope {
                PlatformAuditScope::All => {
                    PlatformPolicy::authorize(principal, PlatformPermission::PlatformAuditReadAll)?;
                    None
                }
                PlatformAuditScope::SelfOnly => Some(principal.user_id()),
            };
            let query = PlatformAuditQuery::new(query.before(), query.limit(), operator_user_id)
                .map_err(|_| PlatformAuditError::InvalidInput)?;
            let page = self
                .repository
                .list(&query)
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page.into_parts();
            let redacted = scope == PlatformAuditScope::SelfOnly;
            Ok(PlatformAuditPage {
                records: records
                    .into_iter()
                    .map(|record| PlatformAuditLog { record, redacted })
                    .collect(),
                next_cursor,
            })
        })
    }
}

impl fmt::Debug for DatabasePlatformAuditService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabasePlatformAuditService(<redacted>)")
    }
}

fn serialize_optional_object(value: Option<Value>) -> Result<Option<String>, PlatformAuditError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let object = value.as_object().ok_or(PlatformAuditError::InvalidInput)?;
    if object.keys().any(|key| sensitive_audit_key(key)) {
        return Err(PlatformAuditError::InvalidInput);
    }
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|_| PlatformAuditError::Internal)
}

fn sensitive_audit_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "key",
        "password",
        "secret",
        "header",
        "body",
        "cookie",
        "credential",
        "oauth",
    ]
    .iter()
    .any(|marker| key.contains(marker))
}

fn map_repository_error(error: PlatformAuditRepositoryError) -> PlatformAuditError {
    let _ = error;
    PlatformAuditError::Internal
}

#[cfg(test)]
mod tests {
    use af_db::MAX_PLATFORM_AUDIT_PAGE_SIZE;

    use super::*;

    #[test]
    fn platform_policy_is_explicit_and_audit_context_rejects_sensitive_keys() {
        let admin = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::Admin);
        let user = SessionPrincipal::new(UserId::new(2).unwrap(), SessionRole::User);
        assert!(PlatformPolicy::allows(
            admin,
            PlatformPermission::UserDirectoryReadAll
        ));
        assert!(!PlatformPolicy::allows(
            user,
            PlatformPermission::UserDirectoryReadAll
        ));
        assert_eq!(
            serialize_optional_object(Some(serde_json::json!({"request_headers": "hidden"}))),
            Err(PlatformAuditError::InvalidInput)
        );
    }

    #[test]
    fn platform_audit_query_enforces_page_bounds() {
        assert!(PlatformAuditListQuery::default().limit() > 0);
        assert_eq!(
            PlatformAuditListQuery::new(None, MAX_PLATFORM_AUDIT_PAGE_SIZE + 1),
            Err(PlatformAuditError::InvalidInput)
        );
    }
}
