use std::{fmt, future::Future, time::Duration};

use af_domain::{
    OrganizationCreditCurrency, OrganizationCreditLimit, OrganizationCreditTermId,
    OrganizationCreditTermPeriod, OrganizationCreditTermStatus, OrganizationCreditTermsSnapshot,
    OrganizationId, OrganizationMembershipStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{organization_credit_terms, organization_memberships, organizations},
};

/// 企业授信策略查询允许的最大页大小。
pub const MAX_ORGANIZATION_CREDIT_TERMS_PAGE_SIZE: usize = 100;

/// 企业授信策略写入命令；策略版本由企业范围仓储分配。
pub struct OrganizationCreditTermsWrite {
    pub(crate) organization_id: OrganizationId,
    pub(crate) currency: OrganizationCreditCurrency,
    pub(crate) credit_limit: OrganizationCreditLimit,
    pub(crate) period: OrganizationCreditTermPeriod,
    pub(crate) effective_from: TimeDateTimeWithTimeZone,
    pub(crate) effective_until: Option<TimeDateTimeWithTimeZone>,
    pub(crate) expected_version: Option<i64>,
    pub(crate) created_by_user_id: UserId,
}

impl OrganizationCreditTermsWrite {
    /// 组装企业授信策略写入事实。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        currency: OrganizationCreditCurrency,
        credit_limit: OrganizationCreditLimit,
        period: OrganizationCreditTermPeriod,
        effective_from: TimeDateTimeWithTimeZone,
        effective_until: Option<TimeDateTimeWithTimeZone>,
        expected_version: Option<i64>,
        created_by_user_id: UserId,
    ) -> Self {
        Self {
            organization_id,
            currency,
            credit_limit,
            period,
            effective_from,
            effective_until,
            expected_version,
            created_by_user_id,
        }
    }
}

impl fmt::Debug for OrganizationCreditTermsWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationCreditTermsWrite")
            .field("currency", &self.currency.as_str())
            .field("period", &self.period)
            .field("effective_from", &self.effective_from)
            .field("effective_until", &self.effective_until)
            .field("expected_version", &self.expected_version)
            .finish_non_exhaustive()
    }
}

/// 已持久化的企业授信策略安全投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationCreditTermsRecord {
    id: OrganizationCreditTermId,
    snapshot: OrganizationCreditTermsSnapshot,
    created_by_user_id: UserId,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
}

impl OrganizationCreditTermsRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationCreditTermId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.snapshot.organization_id()
    }
    #[must_use]
    pub fn currency(&self) -> &OrganizationCreditCurrency {
        self.snapshot.currency()
    }
    #[must_use]
    pub const fn credit_limit(&self) -> OrganizationCreditLimit {
        self.snapshot.credit_limit()
    }
    #[must_use]
    pub const fn period(&self) -> OrganizationCreditTermPeriod {
        self.snapshot.period()
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationCreditTermStatus {
        self.snapshot.status()
    }
    #[must_use]
    pub const fn effective_from(&self) -> i64 {
        self.snapshot.effective_from()
    }
    #[must_use]
    pub const fn effective_until(&self) -> Option<i64> {
        self.snapshot.effective_until()
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.snapshot.version()
    }
    #[must_use]
    pub const fn created_by_user_id(&self) -> UserId {
        self.created_by_user_id
    }
    #[must_use]
    pub const fn created_at(&self) -> TimeDateTimeWithTimeZone {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> TimeDateTimeWithTimeZone {
        self.updated_at
    }
}

/// 一页企业授信策略历史事实。
pub struct OrganizationCreditTermsPageRecord {
    records: Vec<OrganizationCreditTermsRecord>,
    next_cursor: Option<i64>,
}
impl OrganizationCreditTermsPageRecord {
    /// 消费页面并返回策略记录和下一页游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<OrganizationCreditTermsRecord>, Option<i64>) {
        (self.records, self.next_cursor)
    }
}

/// 企业授信仓储错误；错误不携带金额、币种或主体原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditTermsRepositoryError {
    #[error("企业授信策略输入无效")]
    InvalidInput,
    #[error("企业授信策略不存在")]
    NotFound,
    #[error("企业不可用")]
    OrganizationUnavailable,
    #[error("授信策略创建者不可用")]
    CreatorUnavailable,
    #[error("企业授信策略有效区间冲突")]
    IntervalConflict,
    #[error("企业授信策略版本冲突")]
    Conflict,
    #[error("企业授信策略状态转换无效")]
    InvalidTransition,
    #[error("企业授信策略数据库操作失败")]
    Query,
    #[error("企业授信策略操作超时")]
    Timeout,
    #[error("企业授信策略持久化状态损坏")]
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditTermsRepositoryConfigError {
    #[error("企业授信策略仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 企业范围授信策略仓储；所有写操作先锁企业，再锁策略版本集合。
#[derive(Clone)]
pub struct OrganizationCreditTermsRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationCreditTermsRepository {
    /// 使用共享连接池和非零操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationCreditTermsRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationCreditTermsRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }
    /// 按企业读取全部授信策略；兼容旧调用方，新的管理 API 使用分页版本。
    pub async fn list(
        &self,
        organization_id: OrganizationId,
    ) -> Result<Vec<OrganizationCreditTermsRecord>, OrganizationCreditTermsRepositoryError> {
        self.run(self.list_inner(organization_id)).await
    }
    /// 按稳定自增 ID 从新到旧读取一页授信策略历史事实。
    pub async fn list_page(
        &self,
        organization_id: OrganizationId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<OrganizationCreditTermsPageRecord, OrganizationCreditTermsRepositoryError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_ORGANIZATION_CREDIT_TERMS_PAGE_SIZE).contains(&limit)
        {
            return Err(OrganizationCreditTermsRepositoryError::InvalidInput);
        }
        self.run(self.list_page_inner(organization_id, before, limit))
            .await
    }
    /// 按企业和策略 ID 读取授信策略。
    pub async fn get(
        &self,
        organization_id: OrganizationId,
        id: OrganizationCreditTermId,
    ) -> Result<OrganizationCreditTermsRecord, OrganizationCreditTermsRepositoryError> {
        self.run(self.get_inner(organization_id, id)).await
    }
    /// 创建新的版本化授信策略并拒绝生效区间重叠。
    pub async fn create(
        &self,
        write: OrganizationCreditTermsWrite,
    ) -> Result<OrganizationCreditTermsRecord, OrganizationCreditTermsRepositoryError> {
        self.run(self.create_inner(write)).await
    }
    /// 使用版本 CAS 推进策略状态；Closed 为不可恢复终态。
    pub async fn update_status(
        &self,
        organization_id: OrganizationId,
        id: OrganizationCreditTermId,
        status: OrganizationCreditTermStatus,
        expected_version: i64,
    ) -> Result<OrganizationCreditTermsRecord, OrganizationCreditTermsRepositoryError> {
        self.run(self.update_status_inner(organization_id, id, status, expected_version))
            .await
    }
    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, OrganizationCreditTermsRepositoryError>>,
    ) -> Result<T, OrganizationCreditTermsRepositoryError> {
        timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| OrganizationCreditTermsRepositoryError::Timeout)?
    }
    async fn list_inner(
        &self,
        organization_id: OrganizationId,
    ) -> Result<Vec<OrganizationCreditTermsRecord>, OrganizationCreditTermsRepositoryError> {
        ensure_active_organization(&self.pool, organization_id).await?;
        organization_credit_terms::Entity::find()
            .filter(organization_credit_terms::Column::OrganizationId.eq(organization_id.get()))
            .order_by_desc(organization_credit_terms::Column::Id)
            .all(self.pool.connection())
            .await
            .map_err(query_error)?
            .into_iter()
            .map(record_from_model)
            .collect()
    }
    async fn list_page_inner(
        &self,
        organization_id: OrganizationId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<OrganizationCreditTermsPageRecord, OrganizationCreditTermsRepositoryError> {
        ensure_active_organization(&self.pool, organization_id).await?;
        let mut query = organization_credit_terms::Entity::find()
            .filter(organization_credit_terms::Column::OrganizationId.eq(organization_id.get()))
            .order_by_desc(organization_credit_terms::Column::Id)
            .limit((limit + 1) as u64);
        if let Some(before) = before {
            query = query.filter(organization_credit_terms::Column::Id.lt(before));
        }
        let mut models = query
            .all(self.pool.connection())
            .await
            .map_err(query_error)?;
        let has_more = models.len() > limit;
        if has_more {
            models.truncate(limit);
        }
        let records = models
            .into_iter()
            .map(record_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.id().get()))
            .flatten();
        Ok(OrganizationCreditTermsPageRecord {
            records,
            next_cursor,
        })
    }
    async fn get_inner(
        &self,
        organization_id: OrganizationId,
        id: OrganizationCreditTermId,
    ) -> Result<OrganizationCreditTermsRecord, OrganizationCreditTermsRepositoryError> {
        ensure_active_organization(&self.pool, organization_id).await?;
        organization_credit_terms::Entity::find_by_id(id.get())
            .filter(organization_credit_terms::Column::OrganizationId.eq(organization_id.get()))
            .one(self.pool.connection())
            .await
            .map_err(query_error)?
            .ok_or(OrganizationCreditTermsRepositoryError::NotFound)
            .and_then(record_from_model)
    }
    async fn create_inner(
        &self,
        write: OrganizationCreditTermsWrite,
    ) -> Result<OrganizationCreditTermsRecord, OrganizationCreditTermsRepositoryError> {
        validate_write(&write)?;
        let transaction = self.pool.connection().begin().await.map_err(query_error)?;
        lock_active_organization(&transaction, write.organization_id).await?;
        let rows = lock_scope(&transaction, write.organization_id).await?;
        ensure_active_creator(
            &transaction,
            write.organization_id,
            write.created_by_user_id,
        )
        .await?;
        let current_version = rows.iter().map(|row| row.version).max().unwrap_or(0);
        if write
            .expected_version
            .is_some_and(|version| version != current_version)
        {
            let _ = transaction.rollback().await;
            return Err(OrganizationCreditTermsRepositoryError::Conflict);
        }
        if rows.iter().any(|row| {
            intervals_overlap(
                row.effective_from,
                row.effective_until,
                write.effective_from,
                write.effective_until,
            )
        }) {
            let _ = transaction.rollback().await;
            return Err(OrganizationCreditTermsRepositoryError::IntervalConflict);
        }
        let version = current_version
            .checked_add(1)
            .filter(|value| *value > 0)
            .ok_or_else(invariant)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_credit_terms::ActiveModel {
            organization_id: Set(write.organization_id.get()),
            currency: Set(write.currency.as_str().to_owned()),
            credit_limit: Set(write.credit_limit.units()),
            period_kind: Set(write.period.code()),
            period_days: Set(write.period.net_days_value().map(i64::from)),
            status: Set(OrganizationCreditTermStatus::Active.code()),
            effective_from: Set(write.effective_from),
            effective_until: Set(write.effective_until),
            version: Set(version),
            created_by_user_id: Set(write.created_by_user_id.get()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let model = match model.insert(&transaction).await {
            Ok(model) => model,
            Err(error) => {
                let _ = transaction.rollback().await;
                return Err(query_error(error));
            }
        };
        let record = record_from_model(model)?;
        transaction.commit().await.map_err(query_error)?;
        Ok(record)
    }
    async fn update_status_inner(
        &self,
        organization_id: OrganizationId,
        id: OrganizationCreditTermId,
        status: OrganizationCreditTermStatus,
        expected_version: i64,
    ) -> Result<OrganizationCreditTermsRecord, OrganizationCreditTermsRepositoryError> {
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(OrganizationCreditTermsRepositoryError::InvalidInput);
        }
        let transaction = self.pool.connection().begin().await.map_err(query_error)?;
        lock_active_organization(&transaction, organization_id).await?;
        let current = lock_term(&transaction, organization_id, id).await?;
        let current_status =
            OrganizationCreditTermStatus::try_from(current.status).map_err(|_| invariant())?;
        OrganizationCreditTermsSnapshot::validate_status_transition(current_status, status)
            .map_err(|error| match error {
                af_domain::OrganizationCreditTermsError::InvalidTransition => {
                    OrganizationCreditTermsRepositoryError::InvalidTransition
                }
                _ => OrganizationCreditTermsRepositoryError::Invariant,
            })?;
        if current.version != expected_version {
            let _ = transaction.rollback().await;
            return Err(OrganizationCreditTermsRepositoryError::Conflict);
        }
        let next_version = current
            .version
            .checked_add(1)
            .filter(|value| *value > 0)
            .ok_or_else(invariant)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = organization_credit_terms::Entity::update_many()
            .filter(organization_credit_terms::Column::Id.eq(id.get()))
            .filter(organization_credit_terms::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_credit_terms::Column::Version.eq(expected_version))
            .col_expr(
                organization_credit_terms::Column::Status,
                Expr::value(status.code()),
            )
            .col_expr(
                organization_credit_terms::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_credit_terms::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            let _ = transaction.rollback().await;
            return Err(OrganizationCreditTermsRepositoryError::Conflict);
        }
        let model = organization_credit_terms::Entity::find_by_id(id.get())
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = record_from_model(model)?;
        transaction.commit().await.map_err(query_error)?;
        Ok(record)
    }
}

impl fmt::Debug for OrganizationCreditTermsRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationCreditTermsRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn lock_scope(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<Vec<organization_credit_terms::Model>, OrganizationCreditTermsRepositoryError> {
    let mut query = organization_credit_terms::Entity::find()
        .filter(organization_credit_terms::Column::OrganizationId.eq(organization_id.get()))
        .order_by_asc(organization_credit_terms::Column::Id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.all(transaction).await.map_err(query_error)
}
async fn lock_term(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    id: OrganizationCreditTermId,
) -> Result<organization_credit_terms::Model, OrganizationCreditTermsRepositoryError> {
    let mut query = organization_credit_terms::Entity::find_by_id(id.get())
        .filter(organization_credit_terms::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationCreditTermsRepositoryError::NotFound)
}
async fn lock_active_organization(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<(), OrganizationCreditTermsRepositoryError> {
    let mut query = organizations::Entity::find_by_id(organization_id.get())
        .filter(organizations::Column::Status.eq(1_i16));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .map(|_| ())
        .ok_or(OrganizationCreditTermsRepositoryError::OrganizationUnavailable)
}
async fn ensure_active_organization(
    pool: &DatabasePool,
    organization_id: OrganizationId,
) -> Result<(), OrganizationCreditTermsRepositoryError> {
    if organizations::Entity::find_by_id(organization_id.get())
        .filter(organizations::Column::Status.eq(1_i16))
        .one(pool.connection())
        .await
        .map_err(query_error)?
        .is_none()
    {
        return Err(OrganizationCreditTermsRepositoryError::NotFound);
    }
    Ok(())
}
async fn ensure_active_creator(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<(), OrganizationCreditTermsRepositoryError> {
    organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(user_id.get()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .one(transaction)
        .await
        .map_err(query_error)?
        .map(|_| ())
        .ok_or(OrganizationCreditTermsRepositoryError::CreatorUnavailable)
}
fn validate_write(
    write: &OrganizationCreditTermsWrite,
) -> Result<(), OrganizationCreditTermsRepositoryError> {
    if write.effective_from.unix_timestamp() < 0
        || write
            .effective_until
            .is_some_and(|until| until <= write.effective_from)
        || write.expected_version.is_some_and(|version| version < 0)
    {
        return Err(OrganizationCreditTermsRepositoryError::InvalidInput);
    }
    Ok(())
}
fn intervals_overlap(
    existing_from: TimeDateTimeWithTimeZone,
    existing_until: Option<TimeDateTimeWithTimeZone>,
    new_from: TimeDateTimeWithTimeZone,
    new_until: Option<TimeDateTimeWithTimeZone>,
) -> bool {
    existing_until.is_none_or(|until| until > new_from)
        && new_until.is_none_or(|until| existing_from < until)
}
fn record_from_model(
    model: organization_credit_terms::Model,
) -> Result<OrganizationCreditTermsRecord, OrganizationCreditTermsRepositoryError> {
    let currency = OrganizationCreditCurrency::new(model.currency).map_err(|_| invariant())?;
    let limit = OrganizationCreditLimit::new(model.credit_limit).map_err(|_| invariant())?;
    let period_days = model
        .period_days
        .map(|value| u16::try_from(value).map_err(|_| invariant()))
        .transpose()?;
    let period = OrganizationCreditTermPeriod::from_persistence(model.period_kind, period_days)
        .map_err(|_| invariant())?;
    let status = OrganizationCreditTermStatus::try_from(model.status).map_err(|_| invariant())?;
    if model.updated_at < model.created_at
        || model.created_at.unix_timestamp() < 0
        || model.updated_at.unix_timestamp() < 0
    {
        return Err(invariant());
    }
    let snapshot = OrganizationCreditTermsSnapshot::new(
        OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        currency,
        limit,
        period,
        status,
        model.effective_from.unix_timestamp(),
        model.effective_until.map(|value| value.unix_timestamp()),
        model.version,
    )
    .map_err(|_| invariant())?;
    Ok(OrganizationCreditTermsRecord {
        id: OrganizationCreditTermId::new(model.id).map_err(|_| invariant())?,
        snapshot,
        created_by_user_id: UserId::new(model.created_by_user_id).map_err(|_| invariant())?,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}
fn query_error(_: DbErr) -> OrganizationCreditTermsRepositoryError {
    OrganizationCreditTermsRepositoryError::Query
}
fn invariant() -> OrganizationCreditTermsRepositoryError {
    OrganizationCreditTermsRepositoryError::Invariant
}
