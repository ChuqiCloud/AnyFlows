use std::{fmt, time::Duration};

use af_domain::{
    MAX_ORGANIZATION_PLAN_DESCRIPTION_BYTES, MAX_ORGANIZATION_PLAN_NAME_BYTES,
    OrganizationPlanFeatures, OrganizationPlanId, OrganizationPlanPrice, OrganizationPlanSnapshot,
    OrganizationPlanStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{SensitiveString, organization_plan_prices, organization_plans, users},
};

pub const MAX_ORGANIZATION_PLAN_PAGE_SIZE: usize = 100;

/// 平台创建企业套餐时固化的完整销售事实。
pub struct OrganizationPlanWrite {
    pub(crate) plan_id: OrganizationPlanId,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) snapshot: OrganizationPlanSnapshot,
    pub(crate) price: OrganizationPlanPrice,
    pub(crate) created_by_user_id: UserId,
    pub(crate) created_at: u64,
}

impl OrganizationPlanWrite {
    /// 校验文本、能力快照和支付价格；套餐标识由调用方生成并贯穿幂等重试。
    #[allow(clippy::too_many_arguments, reason = "套餐销售事实必须完整固化")]
    pub fn new(
        plan_id: OrganizationPlanId,
        name: String,
        description: Option<String>,
        snapshot: OrganizationPlanSnapshot,
        price: OrganizationPlanPrice,
        created_by_user_id: UserId,
        created_at: u64,
    ) -> Result<Self, OrganizationPlanRepositoryError> {
        if !valid_text(&name, 80)
            || description
                .as_deref()
                .is_some_and(|value| !valid_text(value, 240))
            || snapshot.features().custom_roles_enabled()
        {
            return Err(OrganizationPlanRepositoryError::InvalidInput);
        }
        to_database_time(created_at)?;
        Ok(Self {
            plan_id,
            name,
            description,
            snapshot,
            price,
            created_by_user_id,
            created_at,
        })
    }
}

impl fmt::Debug for OrganizationPlanWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationPlanWrite")
            .field("name", &self.name)
            .field("has_description", &self.description.is_some())
            .field("created_by_user_id", &self.created_by_user_id)
            .finish_non_exhaustive()
    }
}

/// 按套餐版本 CAS 停止销售的命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationPlanDisable {
    pub(crate) plan_id: OrganizationPlanId,
    pub(crate) expected_version: i64,
    pub(crate) disabled_at: u64,
}

impl OrganizationPlanDisable {
    /// 校验停用版本和时间边界。
    pub fn new(
        plan_id: OrganizationPlanId,
        expected_version: i64,
        disabled_at: u64,
    ) -> Result<Self, OrganizationPlanRepositoryError> {
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(OrganizationPlanRepositoryError::InvalidInput);
        }
        to_database_time(disabled_at)?;
        Ok(Self {
            plan_id,
            expected_version,
            disabled_at,
        })
    }
}

/// 已持久化的企业套餐及其不可变能力快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationPlanRecord {
    database_id: i64,
    plan_id: OrganizationPlanId,
    name: String,
    description: Option<String>,
    snapshot: OrganizationPlanSnapshot,
    status: OrganizationPlanStatus,
    price: OrganizationPlanPriceRecord,
    version: i64,
    created_by_user_id: UserId,
    disabled_at: Option<u64>,
    created_at: u64,
    updated_at: u64,
}

impl OrganizationPlanRecord {
    #[must_use]
    pub const fn database_id(&self) -> i64 {
        self.database_id
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
    pub const fn price(&self) -> &OrganizationPlanPriceRecord {
        &self.price
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

/// 已持久化的货币最小单位价格快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationPlanPriceRecord {
    database_id: i64,
    plan_database_id: i64,
    price: OrganizationPlanPrice,
    created_at: u64,
}

impl OrganizationPlanPriceRecord {
    #[must_use]
    pub const fn database_id(&self) -> i64 {
        self.database_id
    }
    #[must_use]
    pub const fn plan_database_id(&self) -> i64 {
        self.plan_database_id
    }
    #[must_use]
    pub fn provider(&self) -> &str {
        self.price.provider()
    }
    #[must_use]
    pub fn currency(&self) -> &str {
        self.price.currency()
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i64 {
        self.price.amount_minor()
    }
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
}

/// 管理端套餐列表分页事实。
pub struct OrganizationPlanPageRecord {
    plans: Vec<OrganizationPlanRecord>,
    next_cursor: Option<i64>,
}

impl OrganizationPlanPageRecord {
    #[must_use]
    pub fn into_parts(self) -> (Vec<OrganizationPlanRecord>, Option<i64>) {
        (self.plans, self.next_cursor)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPlanRepositoryError {
    #[error("企业套餐输入无效")]
    InvalidInput,
    #[error("企业套餐不存在")]
    NotFound,
    #[error("企业套餐创建者不存在")]
    CreatorNotFound,
    #[error("企业套餐事实冲突")]
    Conflict,
    #[error("企业套餐数据库操作失败")]
    Query,
    #[error("企业套餐操作超时")]
    Timeout,
    #[error("企业套餐持久化状态损坏")]
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPlanRepositoryConfigError {
    #[error("企业套餐仓储超时必须大于零")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationPlanCreateOutcome {
    Created,
    Existing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationPlanDisableOutcome {
    Applied,
    Existing,
}

/// 企业套餐目录仓储；价格快照只插入一次，历史行不提供更新入口。
#[derive(Clone)]
pub struct OrganizationPlanRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationPlanRepository {
    /// 使用共享数据库连接池构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationPlanRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationPlanRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 按稳定套餐标识读取完整套餐和价格快照。
    pub async fn get(
        &self,
        plan_id: OrganizationPlanId,
    ) -> Result<Option<OrganizationPlanRecord>, OrganizationPlanRepositoryError> {
        let operation = async {
            let Some((plan, price)) = organization_plans::Entity::find()
                .filter(
                    organization_plans::Column::PlanKey
                        .eq(SensitiveString::from(plan_id.persistence_key())),
                )
                .find_also_related(organization_plan_prices::Entity)
                .one(self.pool.connection())
                .await
                .map_err(|_| OrganizationPlanRepositoryError::Query)?
            else {
                return Ok(None);
            };
            let price = price.ok_or(OrganizationPlanRepositoryError::Invariant)?;
            plan_record(plan, price).map(Some)
        }
        .with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Timeout)?
    }

    /// 读取当前可售套餐；只返回 Active 且有完整价格快照的行。
    pub async fn list_active_catalog(
        &self,
    ) -> Result<Vec<OrganizationPlanRecord>, OrganizationPlanRepositoryError> {
        let operation = async {
            let (rows, _) = load_plan_rows(
                self.pool.connection(),
                true,
                None,
                MAX_ORGANIZATION_PLAN_PAGE_SIZE,
            )
            .await?;
            Ok(rows)
        }
        .with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Timeout)?
    }

    /// 管理端按主键倒序读取全部套餐，支持稳定游标。
    pub async fn list(
        &self,
        before: Option<i64>,
        limit: usize,
    ) -> Result<OrganizationPlanPageRecord, OrganizationPlanRepositoryError> {
        if limit == 0 || limit > MAX_ORGANIZATION_PLAN_PAGE_SIZE || before.is_some_and(|v| v <= 0) {
            return Err(OrganizationPlanRepositoryError::InvalidInput);
        }
        let operation = async move {
            let (rows, has_next) =
                load_plan_rows(self.pool.connection(), false, before, limit).await?;
            let next_cursor = has_next
                .then(|| rows.last().map(|record| record.database_id()))
                .flatten();
            Ok(OrganizationPlanPageRecord {
                plans: rows,
                next_cursor,
            })
        }
        .with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Timeout)?
    }

    /// 幂等创建套餐；同键不同事实返回冲突，绝不覆盖旧价格。
    pub async fn create(
        &self,
        write: OrganizationPlanWrite,
    ) -> Result<
        (OrganizationPlanCreateOutcome, OrganizationPlanRecord),
        OrganizationPlanRepositoryError,
    > {
        let operation =
            async move { self.create_inner(write).await }.with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Timeout)?
    }

    /// 使用版本 CAS 停用套餐；停用后仍可读取历史快照。
    pub async fn disable(
        &self,
        command: OrganizationPlanDisable,
    ) -> Result<
        (OrganizationPlanDisableOutcome, OrganizationPlanRecord),
        OrganizationPlanRepositoryError,
    > {
        let operation = async move { self.disable_inner(command).await }
            .with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Timeout)?
    }

    async fn create_inner(
        &self,
        write: OrganizationPlanWrite,
    ) -> Result<
        (OrganizationPlanCreateOutcome, OrganizationPlanRecord),
        OrganizationPlanRepositoryError,
    > {
        if let Some(existing) = self.get(write.plan_id).await? {
            return classify_existing(existing, &write);
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Query)?;
        if users::Entity::find_by_id(write.created_by_user_id.get())
            .one(&transaction)
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Query)?
            .is_none()
        {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanRepositoryError::Query)?;
            return Err(OrganizationPlanRepositoryError::CreatorNotFound);
        }
        if let Some(existing) = load_plan_by_key(&transaction, write.plan_id).await? {
            let price = load_price(&transaction, existing.id).await?;
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanRepositoryError::Query)?;
            let record = plan_record(
                existing,
                price.ok_or(OrganizationPlanRepositoryError::Invariant)?,
            )?;
            return classify_existing(record, &write);
        }
        let created_at = to_database_time(write.created_at)?;
        let snapshot = write.snapshot;
        let inserted = organization_plans::ActiveModel {
            plan_key: Set(SensitiveString::from(write.plan_id.persistence_key())),
            name: Set(write.name.clone()),
            description: Set(write.description.clone()),
            status: Set(OrganizationPlanStatus::Active.code()),
            team_management_enabled: Set(snapshot.features().team_management_enabled()),
            seat_limit: Set(snapshot.seat_limit()),
            team_limit: Set(snapshot.team_limit()),
            organization_key_limit: Set(snapshot.organization_key_limit()),
            audit_retention_days: Set(snapshot.audit_retention_days()),
            sso_enabled: Set(snapshot.features().sso_enabled()),
            custom_roles_enabled: Set(snapshot.features().custom_roles_enabled()),
            validity_days: Set(snapshot.validity_days()),
            created_by_user_id: Set(write.created_by_user_id.get()),
            version: Set(1),
            disabled_at: Set(None),
            created_at: Set(created_at),
            updated_at: Set(created_at),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(|error| {
            if is_unique_conflict(&error) {
                OrganizationPlanRepositoryError::Conflict
            } else {
                OrganizationPlanRepositoryError::Query
            }
        })?;
        let price = organization_plan_prices::ActiveModel {
            plan_id: Set(inserted.id),
            provider: Set(write.price.provider().to_owned()),
            currency: Set(write.price.currency().to_owned()),
            amount_minor: Set(write.price.amount_minor()),
            created_at: Set(created_at),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(|_| OrganizationPlanRepositoryError::Query)?;
        let record = plan_record(inserted, price)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Query)?;
        Ok((OrganizationPlanCreateOutcome::Created, record))
    }

    async fn disable_inner(
        &self,
        command: OrganizationPlanDisable,
    ) -> Result<
        (OrganizationPlanDisableOutcome, OrganizationPlanRecord),
        OrganizationPlanRepositoryError,
    > {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Query)?;
        let Some(model) = lock_plan(&transaction, command.plan_id).await? else {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanRepositoryError::Query)?;
            return Err(OrganizationPlanRepositoryError::NotFound);
        };
        let price = load_price(&transaction, model.id)
            .await?
            .ok_or(OrganizationPlanRepositoryError::Invariant)?;
        let current = plan_record(model.clone(), price.clone())?;
        if current.status() == OrganizationPlanStatus::Disabled {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanRepositoryError::Query)?;
            if current.version() == command.expected_version + 1
                && current.disabled_at() == Some(command.disabled_at)
            {
                return Ok((OrganizationPlanDisableOutcome::Existing, current));
            }
            return Err(OrganizationPlanRepositoryError::Conflict);
        }
        if current.version() != command.expected_version
            || command.disabled_at < current.created_at()
        {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanRepositoryError::Query)?;
            return Err(OrganizationPlanRepositoryError::Conflict);
        }
        let disabled_at = to_database_time(command.disabled_at)?;
        let next_version = command
            .expected_version
            .checked_add(1)
            .ok_or(OrganizationPlanRepositoryError::Invariant)?;
        let updated_at = current.updated_at().max(command.disabled_at);
        let updated_at = to_database_time(updated_at)?;
        let result = organization_plans::Entity::update_many()
            .filter(organization_plans::Column::Id.eq(model.id))
            .filter(organization_plans::Column::Version.eq(command.expected_version))
            .filter(organization_plans::Column::Status.eq(OrganizationPlanStatus::Active.code()))
            .col_expr(
                organization_plans::Column::Status,
                Expr::value(OrganizationPlanStatus::Disabled.code()),
            )
            .col_expr(
                organization_plans::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_plans::Column::DisabledAt,
                Expr::value(Some(disabled_at)),
            )
            .col_expr(
                organization_plans::Column::UpdatedAt,
                Expr::value(updated_at),
            )
            .exec(&transaction)
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Query)?;
        if result.rows_affected != 1 {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanRepositoryError::Query)?;
            return Err(OrganizationPlanRepositoryError::Conflict);
        }
        let updated = organization_plans::Entity::find_by_id(model.id)
            .one(&transaction)
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Query)?
            .ok_or(OrganizationPlanRepositoryError::Invariant)?;
        let record = plan_record(updated, price)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationPlanRepositoryError::Query)?;
        Ok((OrganizationPlanDisableOutcome::Applied, record))
    }
}

async fn load_plan_rows<C: ConnectionTrait>(
    connection: &C,
    active_only: bool,
    before: Option<i64>,
    limit: usize,
) -> Result<(Vec<OrganizationPlanRecord>, bool), OrganizationPlanRepositoryError> {
    let query_limit = limit
        .checked_add(usize::from(!active_only))
        .ok_or(OrganizationPlanRepositoryError::Invariant)?;
    let mut query = organization_plans::Entity::find()
        .find_also_related(organization_plan_prices::Entity)
        .order_by_desc(organization_plans::Column::Id)
        .limit(u64::try_from(query_limit).map_err(|_| OrganizationPlanRepositoryError::Invariant)?);
    if active_only {
        query = query
            .filter(organization_plans::Column::Status.eq(OrganizationPlanStatus::Active.code()));
    }
    if let Some(before) = before {
        query = query.filter(organization_plans::Column::Id.lt(before));
    }
    let rows = query
        .all(connection)
        .await
        .map_err(|_| OrganizationPlanRepositoryError::Query)?;
    let has_next = rows.len() > limit;
    let mut records = rows
        .into_iter()
        .map(|(plan, price)| {
            plan_record(
                plan,
                price.ok_or(OrganizationPlanRepositoryError::Invariant)?,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if has_next {
        records.pop();
    }
    Ok((records, has_next))
}

async fn load_plan_by_key<C: ConnectionTrait>(
    connection: &C,
    plan_id: OrganizationPlanId,
) -> Result<Option<organization_plans::Model>, OrganizationPlanRepositoryError> {
    organization_plans::Entity::find()
        .filter(
            organization_plans::Column::PlanKey
                .eq(SensitiveString::from(plan_id.persistence_key())),
        )
        .one(connection)
        .await
        .map_err(|_| OrganizationPlanRepositoryError::Query)
}

async fn load_price<C: ConnectionTrait>(
    connection: &C,
    plan_id: i64,
) -> Result<Option<organization_plan_prices::Model>, OrganizationPlanRepositoryError> {
    organization_plan_prices::Entity::find()
        .filter(organization_plan_prices::Column::PlanId.eq(plan_id))
        .one(connection)
        .await
        .map_err(|_| OrganizationPlanRepositoryError::Query)
}

async fn lock_plan(
    transaction: &DatabaseTransaction,
    plan_id: OrganizationPlanId,
) -> Result<Option<organization_plans::Model>, OrganizationPlanRepositoryError> {
    let mut query = organization_plans::Entity::find().filter(
        organization_plans::Column::PlanKey.eq(SensitiveString::from(plan_id.persistence_key())),
    );
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationPlanRepositoryError::Query)
}

fn classify_existing(
    record: OrganizationPlanRecord,
    write: &OrganizationPlanWrite,
) -> Result<(OrganizationPlanCreateOutcome, OrganizationPlanRecord), OrganizationPlanRepositoryError>
{
    if record.name() == write.name
        && record.description() == write.description.as_deref()
        && record.snapshot() == write.snapshot
        && record.price().provider() == write.price.provider()
        && record.price().currency() == write.price.currency()
        && record.price().amount_minor() == write.price.amount_minor()
        && record.created_by_user_id() == write.created_by_user_id
        && record.created_at() == write.created_at
    {
        Ok((OrganizationPlanCreateOutcome::Existing, record))
    } else {
        Err(OrganizationPlanRepositoryError::Conflict)
    }
}

fn plan_record(
    model: organization_plans::Model,
    price_model: organization_plan_prices::Model,
) -> Result<OrganizationPlanRecord, OrganizationPlanRepositoryError> {
    let plan_id = OrganizationPlanId::from_persistence_key(model.plan_key.as_str())
        .map_err(|_| OrganizationPlanRepositoryError::Invariant)?;
    let status = OrganizationPlanStatus::try_from(model.status)
        .map_err(|_| OrganizationPlanRepositoryError::Invariant)?;
    let features = OrganizationPlanFeatures::new(
        model.team_management_enabled,
        model.sso_enabled,
        model.custom_roles_enabled,
    );
    let snapshot = OrganizationPlanSnapshot::new(
        features,
        model.seat_limit,
        model.team_limit,
        model.organization_key_limit,
        model.audit_retention_days,
        model.validity_days,
    )
    .map_err(|_| OrganizationPlanRepositoryError::Invariant)?;
    let price_id = price_model.id;
    let price_plan_database_id = price_model.plan_id;
    let price_created_at = unix_seconds(price_model.created_at)?;
    let price = OrganizationPlanPrice::new(
        price_model.provider,
        price_model.currency,
        price_model.amount_minor,
    )
    .map_err(|_| OrganizationPlanRepositoryError::Invariant)?;
    let disabled_at = optional_unix_seconds(model.disabled_at)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    let created_by_user_id = UserId::new(model.created_by_user_id)
        .map_err(|_| OrganizationPlanRepositoryError::Invariant)?;
    if model.id <= 0
        || !valid_text(&model.name, MAX_ORGANIZATION_PLAN_NAME_BYTES)
        || model
            .description
            .as_deref()
            .is_some_and(|value| !valid_text(value, MAX_ORGANIZATION_PLAN_DESCRIPTION_BYTES))
        || model.version <= 0
        || price_id <= 0
        || price_plan_database_id != model.id
        || price_created_at < created_at
        || updated_at < created_at
        || disabled_at.is_some_and(|value| value < created_at || value > updated_at)
        || (status == OrganizationPlanStatus::Active && disabled_at.is_some())
        || (status == OrganizationPlanStatus::Disabled && disabled_at.is_none())
    {
        return Err(OrganizationPlanRepositoryError::Invariant);
    }
    Ok(OrganizationPlanRecord {
        database_id: model.id,
        plan_id,
        name: model.name,
        description: model.description,
        snapshot,
        status,
        price: OrganizationPlanPriceRecord {
            database_id: price_id,
            plan_database_id: price_plan_database_id,
            price,
            created_at: price_created_at,
        },
        version: model.version,
        created_by_user_id,
        disabled_at,
        created_at,
        updated_at,
    })
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn to_database_time(
    value: u64,
) -> Result<TimeDateTimeWithTimeZone, OrganizationPlanRepositoryError> {
    let value = i64::try_from(value).map_err(|_| OrganizationPlanRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| OrganizationPlanRepositoryError::Invariant)
}

fn unix_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, OrganizationPlanRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| OrganizationPlanRepositoryError::Invariant)
}

fn optional_unix_seconds(
    value: Option<TimeDateTimeWithTimeZone>,
) -> Result<Option<u64>, OrganizationPlanRepositoryError> {
    value.map(unix_seconds).transpose()
}

fn is_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_organization_plans_plan_key")
        || rendered.contains("organization_plans.plan_key")
}
