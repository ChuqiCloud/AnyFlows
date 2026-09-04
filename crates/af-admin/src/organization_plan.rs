use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    OrganizationPlanDisable, OrganizationPlanPageRecord, OrganizationPlanRecord,
    OrganizationPlanRepository, OrganizationPlanRepositoryError, OrganizationPlanWrite,
};
use af_domain::{
    OrganizationPlanFeatures, OrganizationPlanId, OrganizationPlanPrice, OrganizationPlanSnapshot,
    OrganizationPlanStatus, UserId,
};
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 企业套餐管理端默认页大小。
pub const DEFAULT_ORGANIZATION_PLAN_PAGE_SIZE: usize = 25;

/// 管理端创建企业套餐的结构化命令。
pub struct OrganizationPlanCreateCommand {
    name: String,
    description: Option<String>,
    snapshot: OrganizationPlanSnapshot,
    price: OrganizationPlanPrice,
}

impl OrganizationPlanCreateCommand {
    /// 构造完整的能力和价格快照；自定义角色当前由销售边界拒绝。
    #[allow(clippy::too_many_arguments, reason = "套餐销售快照字段必须完整提交")]
    pub fn new(
        name: String,
        description: Option<String>,
        team_management_enabled: bool,
        sso_enabled: bool,
        custom_roles_enabled: bool,
        seat_limit: i64,
        team_limit: i64,
        organization_key_limit: i64,
        audit_retention_days: i64,
        validity_days: i64,
        price_provider: String,
        price_currency: String,
        price_amount_minor: i64,
    ) -> Result<Self, OrganizationPlanServiceError> {
        let features = OrganizationPlanFeatures::new(
            team_management_enabled,
            sso_enabled,
            custom_roles_enabled,
        );
        if custom_roles_enabled {
            return Err(OrganizationPlanServiceError::InvalidRequest);
        }
        let snapshot = OrganizationPlanSnapshot::new(
            features,
            seat_limit,
            team_limit,
            organization_key_limit,
            audit_retention_days,
            validity_days,
        )
        .map_err(|_| OrganizationPlanServiceError::InvalidRequest)?;
        let price = OrganizationPlanPrice::new(price_provider, price_currency, price_amount_minor)
            .map_err(|_| OrganizationPlanServiceError::InvalidRequest)?;
        if !valid_text(&name, 80)
            || description
                .as_deref()
                .is_some_and(|value| !valid_text(value, 240))
        {
            return Err(OrganizationPlanServiceError::InvalidRequest);
        }
        Ok(Self {
            name,
            description,
            snapshot,
            price,
        })
    }
}

impl fmt::Debug for OrganizationPlanCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationPlanCreateCommand(<redacted>)")
    }
}

/// 管理端按版本停用企业套餐的命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationPlanDisableCommand {
    expected_version: i64,
}

impl OrganizationPlanDisableCommand {
    /// 校验版本 CAS 参数。
    pub fn new(expected_version: i64) -> Result<Self, OrganizationPlanServiceError> {
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(OrganizationPlanServiceError::InvalidRequest);
        }
        Ok(Self { expected_version })
    }
}

/// 企业套餐倒序稳定游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationPlanListQuery {
    before: Option<i64>,
    limit: usize,
}

impl OrganizationPlanListQuery {
    /// 校验游标和页大小。
    pub fn new(before: Option<i64>, limit: usize) -> Result<Self, OrganizationPlanServiceError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=af_db::MAX_ORGANIZATION_PLAN_PAGE_SIZE).contains(&limit)
        {
            return Err(OrganizationPlanServiceError::InvalidRequest);
        }
        Ok(Self { before, limit })
    }

    #[must_use]
    pub const fn before(self) -> Option<i64> {
        self.before
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for OrganizationPlanListQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: DEFAULT_ORGANIZATION_PLAN_PAGE_SIZE,
        }
    }
}

/// 可安全交给 HTTP/前端的企业套餐投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationPlanView {
    plan_id: OrganizationPlanId,
    name: String,
    description: Option<String>,
    snapshot: OrganizationPlanSnapshot,
    status: OrganizationPlanStatus,
    provider: String,
    currency: String,
    amount_minor: i64,
    version: i64,
    created_by_user_id: UserId,
    disabled_at: Option<u64>,
    created_at: u64,
    updated_at: u64,
}

impl OrganizationPlanView {
    fn from_record(record: &OrganizationPlanRecord) -> Self {
        let snapshot = record.snapshot();
        Self {
            plan_id: record.plan_id(),
            name: record.name().to_owned(),
            description: record.description().map(str::to_owned),
            snapshot,
            status: record.status(),
            provider: record.price().provider().to_owned(),
            currency: record.price().currency().to_owned(),
            amount_minor: record.price().amount_minor(),
            version: record.version(),
            created_by_user_id: record.created_by_user_id(),
            disabled_at: record.disabled_at(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
        }
    }

    #[must_use]
    pub const fn plan_id(&self) -> OrganizationPlanId {
        self.plan_id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    #[must_use]
    pub const fn snapshot(&self) -> OrganizationPlanSnapshot {
        self.snapshot
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationPlanStatus {
        self.status
    }
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i64 {
        self.amount_minor
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn created_by_user_id(&self) -> UserId {
        self.created_by_user_id
    }
    #[must_use]
    pub const fn disabled_at(&self) -> Option<u64> {
        self.disabled_at
    }
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }
}

/// 当前用户可见的可售企业套餐目录。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationPlanCatalog {
    plans: Vec<OrganizationPlanView>,
}

impl OrganizationPlanCatalog {
    fn from_records(records: Vec<OrganizationPlanRecord>) -> Self {
        Self {
            plans: records
                .iter()
                .map(OrganizationPlanView::from_record)
                .collect(),
        }
    }

    #[must_use]
    pub fn plans(&self) -> &[OrganizationPlanView] {
        &self.plans
    }
}

/// 管理端企业套餐分页结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationPlanPage {
    plans: Vec<OrganizationPlanView>,
    next_cursor: Option<i64>,
}

impl OrganizationPlanPage {
    fn from_record(page: OrganizationPlanPageRecord) -> Self {
        let (records, next_cursor) = page.into_parts();
        Self {
            plans: records
                .iter()
                .map(OrganizationPlanView::from_record)
                .collect(),
            next_cursor,
        }
    }

    #[must_use]
    pub fn plans(&self) -> &[OrganizationPlanView] {
        &self.plans
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPlanServiceError {
    #[error("企业套餐请求无效")]
    InvalidRequest,
    #[error("企业套餐权限不足")]
    Forbidden,
    #[error("企业套餐不存在")]
    NotFound,
    #[error("企业套餐版本冲突")]
    Conflict,
    #[error("企业套餐内部失败")]
    Internal,
}

pub type OrganizationPlanCatalogFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationPlanCatalog, OrganizationPlanServiceError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationPlanPageFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationPlanPage, OrganizationPlanServiceError>> + Send + 'a,
    >,
>;
pub type OrganizationPlanFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationPlanView, OrganizationPlanServiceError>> + Send + 'a,
    >,
>;

/// 企业套餐目录与平台管理应用端口。
pub trait OrganizationPlanService: Send + Sync {
    /// 读取当前仍可售的套餐快照；不创建订单或授权。
    fn list_catalog<'a>(&'a self, principal: SessionPrincipal)
    -> OrganizationPlanCatalogFuture<'a>;
    /// 平台管理员读取所有套餐事实。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: OrganizationPlanListQuery,
    ) -> OrganizationPlanPageFuture<'a>;
    /// 平台管理员创建不可变套餐与价格快照。
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: OrganizationPlanCreateCommand,
    ) -> OrganizationPlanFuture<'a>;
    /// 平台管理员按版本 CAS 停用套餐。
    fn disable<'a>(
        &'a self,
        principal: SessionPrincipal,
        plan_id: OrganizationPlanId,
        command: OrganizationPlanDisableCommand,
    ) -> OrganizationPlanFuture<'a>;
}

/// 使用数据库仓储实现企业套餐目录服务。
pub struct DatabaseOrganizationPlanService {
    repository: Arc<OrganizationPlanRepository>,
}

impl DatabaseOrganizationPlanService {
    /// 绑定启动期创建的企业套餐仓储。
    #[must_use]
    pub fn new(repository: Arc<OrganizationPlanRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationPlanService for DatabaseOrganizationPlanService {
    fn list_catalog<'a>(
        &'a self,
        principal: SessionPrincipal,
    ) -> OrganizationPlanCatalogFuture<'a> {
        Box::pin(async move {
            let _ = principal;
            self.repository
                .list_active_catalog()
                .await
                .map(OrganizationPlanCatalog::from_records)
                .map_err(map_read_error)
        })
    }

    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: OrganizationPlanListQuery,
    ) -> OrganizationPlanPageFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .list(query.before(), query.limit())
                .await
                .map(OrganizationPlanPage::from_record)
                .map_err(map_read_error)
        })
    }

    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: OrganizationPlanCreateCommand,
    ) -> OrganizationPlanFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let write = OrganizationPlanWrite::new(
                random_plan_id()?,
                command.name,
                command.description,
                command.snapshot,
                command.price,
                principal.user_id(),
                unix_now()?,
            )
            .map_err(|_| OrganizationPlanServiceError::InvalidRequest)?;
            let (_, record) = self
                .repository
                .create(write)
                .await
                .map_err(map_write_error)?;
            Ok(OrganizationPlanView::from_record(&record))
        })
    }

    fn disable<'a>(
        &'a self,
        principal: SessionPrincipal,
        plan_id: OrganizationPlanId,
        command: OrganizationPlanDisableCommand,
    ) -> OrganizationPlanFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let disable =
                OrganizationPlanDisable::new(plan_id, command.expected_version, unix_now()?)
                    .map_err(|_| OrganizationPlanServiceError::InvalidRequest)?;
            let (_, record) = self
                .repository
                .disable(disable)
                .await
                .map_err(map_write_error)?;
            Ok(OrganizationPlanView::from_record(&record))
        })
    }
}

impl fmt::Debug for DatabaseOrganizationPlanService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationPlanService(<repository>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), OrganizationPlanServiceError> {
    (principal.role() == SessionRole::Admin)
        .then_some(())
        .ok_or(OrganizationPlanServiceError::Forbidden)
}

fn random_plan_id() -> Result<OrganizationPlanId, OrganizationPlanServiceError> {
    loop {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| OrganizationPlanServiceError::Internal)?;
        if let Ok(id) = OrganizationPlanId::new(bytes) {
            return Ok(id);
        }
    }
}

fn unix_now() -> Result<u64, OrganizationPlanServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| OrganizationPlanServiceError::Internal)
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn map_read_error(error: OrganizationPlanRepositoryError) -> OrganizationPlanServiceError {
    match error {
        OrganizationPlanRepositoryError::InvalidInput => {
            OrganizationPlanServiceError::InvalidRequest
        }
        OrganizationPlanRepositoryError::NotFound => OrganizationPlanServiceError::NotFound,
        OrganizationPlanRepositoryError::Conflict => OrganizationPlanServiceError::Conflict,
        OrganizationPlanRepositoryError::CreatorNotFound
        | OrganizationPlanRepositoryError::Query
        | OrganizationPlanRepositoryError::Timeout
        | OrganizationPlanRepositoryError::Invariant => OrganizationPlanServiceError::Internal,
    }
}

fn map_write_error(error: OrganizationPlanRepositoryError) -> OrganizationPlanServiceError {
    match error {
        OrganizationPlanRepositoryError::InvalidInput => {
            OrganizationPlanServiceError::InvalidRequest
        }
        OrganizationPlanRepositoryError::NotFound => OrganizationPlanServiceError::NotFound,
        OrganizationPlanRepositoryError::Conflict => OrganizationPlanServiceError::Conflict,
        OrganizationPlanRepositoryError::CreatorNotFound => OrganizationPlanServiceError::Forbidden,
        OrganizationPlanRepositoryError::Query
        | OrganizationPlanRepositoryError::Timeout
        | OrganizationPlanRepositoryError::Invariant => OrganizationPlanServiceError::Internal,
    }
}
