use std::{fmt, future::Future, time::Duration};

use af_domain::{
    OrganizationContractPriceId, OrganizationId, OrganizationMembershipStatus, Protocol, UserId,
};
use rust_decimal::Decimal;
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
    entity::{organization_contract_prices, organization_memberships, organizations},
};

/// 企业合同价创建输入；价格单位固定为美元/百万 Token。
pub struct OrganizationContractPriceWrite {
    pub(crate) organization_id: OrganizationId,
    pub(crate) model: String,
    pub(crate) protocol: Protocol,
    pub(crate) prices: [Decimal; 5],
    pub(crate) effective_from: TimeDateTimeWithTimeZone,
    pub(crate) effective_until: Option<TimeDateTimeWithTimeZone>,
    pub(crate) expected_version: Option<i64>,
    pub(crate) created_by_user_id: UserId,
}

impl OrganizationContractPriceWrite {
    /// 组装合同价写入事实，公开标识和时间戳由仓储生成。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        model: String,
        protocol: Protocol,
        prices: [Decimal; 5],
        effective_from: TimeDateTimeWithTimeZone,
        effective_until: Option<TimeDateTimeWithTimeZone>,
        expected_version: Option<i64>,
        created_by_user_id: UserId,
    ) -> Self {
        Self {
            organization_id,
            model,
            protocol,
            prices,
            effective_from,
            effective_until,
            expected_version,
            created_by_user_id,
        }
    }
}

impl fmt::Debug for OrganizationContractPriceWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationContractPriceWrite")
            .field("protocol", &self.protocol)
            .field("effective_from", &self.effective_from)
            .field("effective_until", &self.effective_until)
            .field("expected_version", &self.expected_version)
            .finish_non_exhaustive()
    }
}

/// 已持久化的合同价安全投影。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationContractPriceRecord {
    id: OrganizationContractPriceId,
    organization_id: OrganizationId,
    model: String,
    protocol: Protocol,
    prices: [Decimal; 5],
    effective_from: TimeDateTimeWithTimeZone,
    effective_until: Option<TimeDateTimeWithTimeZone>,
    version: i64,
    created_by_user_id: UserId,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
}

impl OrganizationContractPriceRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationContractPriceId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }
    #[must_use]
    pub const fn prices(&self) -> [Decimal; 5] {
        self.prices
    }
    #[must_use]
    pub const fn effective_from(&self) -> TimeDateTimeWithTimeZone {
        self.effective_from
    }
    #[must_use]
    pub const fn effective_until(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.effective_until
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
    pub const fn created_at(&self) -> TimeDateTimeWithTimeZone {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> TimeDateTimeWithTimeZone {
        self.updated_at
    }
}

/// 合同价仓储错误；不携带模型名、价格或其他外部原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationContractPriceRepositoryError {
    #[error("企业合同价输入无效")]
    InvalidInput,
    #[error("企业合同价不存在")]
    NotFound,
    #[error("企业不可用")]
    OrganizationUnavailable,
    #[error("合同价创建者不可用")]
    CreatorUnavailable,
    #[error("合同价有效区间冲突")]
    IntervalConflict,
    #[error("合同价版本冲突")]
    Conflict,
    #[error("合同价状态转换无效")]
    InvalidTransition,
    #[error("企业合同价数据库操作失败")]
    Query,
    #[error("企业合同价操作超时")]
    Timeout,
    #[error("企业合同价持久化状态损坏")]
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationContractPriceRepositoryConfigError {
    #[error("企业合同价仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 企业范围合同价仓储；所有写操作先锁企业，再锁同模型协议区间。
#[derive(Clone)]
pub struct OrganizationContractPriceRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationContractPriceRepository {
    /// 使用共享连接池和非零操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationContractPriceRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationContractPriceRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 创建企业合同价版本，并在企业锁内复验有效区间不重叠。
    pub async fn create(
        &self,
        write: OrganizationContractPriceWrite,
    ) -> Result<OrganizationContractPriceRecord, OrganizationContractPriceRepositoryError> {
        self.run(self.create_inner(write)).await
    }

    /// 按企业范围列出全部合同价历史事实。
    pub async fn list(
        &self,
        organization_id: OrganizationId,
    ) -> Result<Vec<OrganizationContractPriceRecord>, OrganizationContractPriceRepositoryError>
    {
        self.run(self.list_inner(organization_id)).await
    }

    /// 按企业与内部标识读取合同价，跨企业标识统一视为不存在。
    pub async fn get(
        &self,
        organization_id: OrganizationId,
        id: OrganizationContractPriceId,
    ) -> Result<OrganizationContractPriceRecord, OrganizationContractPriceRepositoryError> {
        self.run(self.get_inner(organization_id, id)).await
    }

    /// 按记录版本 CAS 关闭合同价区间。
    pub async fn close(
        &self,
        organization_id: OrganizationId,
        id: OrganizationContractPriceId,
        effective_until: TimeDateTimeWithTimeZone,
        expected_version: i64,
    ) -> Result<OrganizationContractPriceRecord, OrganizationContractPriceRepositoryError> {
        self.run(self.close_inner(organization_id, id, effective_until, expected_version))
            .await
    }

    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, OrganizationContractPriceRepositoryError>>,
    ) -> Result<T, OrganizationContractPriceRepositoryError> {
        timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| OrganizationContractPriceRepositoryError::Timeout)?
    }

    async fn create_inner(
        &self,
        write: OrganizationContractPriceWrite,
    ) -> Result<OrganizationContractPriceRecord, OrganizationContractPriceRepositoryError> {
        validate_write(&write)?;
        let transaction = begin(&self.pool).await?;
        lock_active_organization(&transaction, write.organization_id).await?;
        ensure_active_creator(
            &transaction,
            write.organization_id,
            write.created_by_user_id,
        )
        .await?;
        let scoped = lock_scope(
            &transaction,
            write.organization_id,
            &write.model,
            write.protocol,
        )
        .await?;
        let current_version = scoped.iter().map(|row| row.version).max().unwrap_or(0);
        if write
            .expected_version
            .is_some_and(|version| version != current_version)
        {
            rollback(transaction).await?;
            return Err(OrganizationContractPriceRepositoryError::Conflict);
        }
        if scoped.iter().any(|row| {
            intervals_overlap(
                row.effective_from,
                row.effective_until,
                write.effective_from,
                write.effective_until,
            )
        }) {
            rollback(transaction).await?;
            return Err(OrganizationContractPriceRepositoryError::IntervalConflict);
        }
        let version = current_version
            .checked_add(1)
            .filter(|value| *value > 0)
            .ok_or_else(invariant)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_contract_prices::ActiveModel {
            organization_id: Set(write.organization_id.get()),
            model: Set(write.model.into()),
            protocol: Set(write.protocol.as_str().to_owned()),
            input_price: Set(write.prices[0].into()),
            output_price: Set(write.prices[1].into()),
            cache_read_price: Set(write.prices[2].into()),
            cache_creation_5m_price: Set(write.prices[3].into()),
            cache_creation_1h_price: Set(write.prices[4].into()),
            effective_from: Set(write.effective_from),
            effective_until: Set(write.effective_until),
            version: Set(version),
            created_by_user_id: Set(write.created_by_user_id.get()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(query_error)?;
        let record = record_from_model(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn list_inner(
        &self,
        organization_id: OrganizationId,
    ) -> Result<Vec<OrganizationContractPriceRecord>, OrganizationContractPriceRepositoryError>
    {
        ensure_active_organization(&self.pool, organization_id).await?;
        organization_contract_prices::Entity::find()
            .filter(organization_contract_prices::Column::OrganizationId.eq(organization_id.get()))
            .order_by_asc(organization_contract_prices::Column::Model)
            .order_by_asc(organization_contract_prices::Column::Protocol)
            .order_by_desc(organization_contract_prices::Column::EffectiveFrom)
            .order_by_desc(organization_contract_prices::Column::Id)
            .all(self.pool.connection())
            .await
            .map_err(query_error)?
            .into_iter()
            .map(record_from_model)
            .collect()
    }

    async fn get_inner(
        &self,
        organization_id: OrganizationId,
        id: OrganizationContractPriceId,
    ) -> Result<OrganizationContractPriceRecord, OrganizationContractPriceRepositoryError> {
        ensure_active_organization(&self.pool, organization_id).await?;
        let model = organization_contract_prices::Entity::find_by_id(id.get())
            .filter(organization_contract_prices::Column::OrganizationId.eq(organization_id.get()))
            .one(self.pool.connection())
            .await
            .map_err(query_error)?
            .ok_or(OrganizationContractPriceRepositoryError::NotFound)?;
        record_from_model(model)
    }

    async fn close_inner(
        &self,
        organization_id: OrganizationId,
        id: OrganizationContractPriceId,
        effective_until: TimeDateTimeWithTimeZone,
        expected_version: i64,
    ) -> Result<OrganizationContractPriceRecord, OrganizationContractPriceRepositoryError> {
        validate_version(expected_version)?;
        let transaction = begin(&self.pool).await?;
        lock_active_organization(&transaction, organization_id).await?;
        let current = lock_price(&transaction, organization_id, id).await?;
        if current.version != expected_version {
            rollback(transaction).await?;
            return Err(OrganizationContractPriceRepositoryError::Conflict);
        }
        if current.effective_until.is_some() {
            rollback(transaction).await?;
            return Err(OrganizationContractPriceRepositoryError::InvalidTransition);
        }
        if effective_until <= current.effective_from {
            rollback(transaction).await?;
            return Err(OrganizationContractPriceRepositoryError::InvalidInput);
        }
        let next_version = current
            .version
            .checked_add(1)
            .filter(|value| *value > 0)
            .ok_or_else(invariant)?;
        let result = organization_contract_prices::Entity::update_many()
            .filter(organization_contract_prices::Column::Id.eq(id.get()))
            .filter(organization_contract_prices::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_contract_prices::Column::Version.eq(expected_version))
            .filter(organization_contract_prices::Column::EffectiveUntil.is_null())
            .col_expr(
                organization_contract_prices::Column::EffectiveUntil,
                Expr::value(effective_until),
            )
            .col_expr(
                organization_contract_prices::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_contract_prices::Column::UpdatedAt,
                Expr::value(TimeDateTimeWithTimeZone::now_utc()),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationContractPriceRepositoryError::Conflict);
        }
        let model = organization_contract_prices::Entity::find_by_id(id.get())
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = record_from_model(model)?;
        commit(transaction).await?;
        Ok(record)
    }
}

impl fmt::Debug for OrganizationContractPriceRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationContractPriceRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn lock_active_organization(
    transaction: &DatabaseTransaction,
    id: OrganizationId,
) -> Result<(), OrganizationContractPriceRepositoryError> {
    let mut query =
        organizations::Entity::find_by_id(id.get()).filter(organizations::Column::Status.eq(1_i16));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    if query.one(transaction).await.map_err(query_error)?.is_none() {
        return Err(OrganizationContractPriceRepositoryError::OrganizationUnavailable);
    }
    Ok(())
}

async fn ensure_active_organization(
    pool: &DatabasePool,
    id: OrganizationId,
) -> Result<(), OrganizationContractPriceRepositoryError> {
    if organizations::Entity::find_by_id(id.get())
        .filter(organizations::Column::Status.eq(1_i16))
        .one(pool.connection())
        .await
        .map_err(query_error)?
        .is_none()
    {
        return Err(OrganizationContractPriceRepositoryError::NotFound);
    }
    Ok(())
}

async fn ensure_active_creator(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<(), OrganizationContractPriceRepositoryError> {
    let found = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(user_id.get()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .one(transaction)
        .await
        .map_err(query_error)?;
    found
        .map(|_| ())
        .ok_or(OrganizationContractPriceRepositoryError::CreatorUnavailable)
}

async fn lock_scope(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    model: &str,
    protocol: Protocol,
) -> Result<Vec<organization_contract_prices::Model>, OrganizationContractPriceRepositoryError> {
    let mut query = organization_contract_prices::Entity::find()
        .filter(organization_contract_prices::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_contract_prices::Column::Model.eq(model))
        .filter(organization_contract_prices::Column::Protocol.eq(protocol.as_str()))
        .order_by_asc(organization_contract_prices::Column::EffectiveFrom)
        .order_by_asc(organization_contract_prices::Column::Id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.all(transaction).await.map_err(query_error)
}

async fn lock_price(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    id: OrganizationContractPriceId,
) -> Result<organization_contract_prices::Model, OrganizationContractPriceRepositoryError> {
    let mut query = organization_contract_prices::Entity::find()
        .filter(organization_contract_prices::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_contract_prices::Column::Id.eq(id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationContractPriceRepositoryError::NotFound)
}

fn validate_write(
    write: &OrganizationContractPriceWrite,
) -> Result<(), OrganizationContractPriceRepositoryError> {
    if write.model.is_empty()
        || write.model.len() > 256
        || write.model.trim() != write.model
        || write.model.chars().any(char::is_control)
        || write
            .effective_until
            .is_some_and(|until| until <= write.effective_from)
        || write.expected_version.is_some_and(|version| version < 0)
    {
        return Err(OrganizationContractPriceRepositoryError::InvalidInput);
    }
    if write.prices.iter().any(|value| {
        value.is_sign_negative()
            || value.scale() > 28
            || value
                .normalize()
                .to_string()
                .split('.')
                .next()
                .is_some_and(|part| part.len() > 10)
    }) {
        return Err(OrganizationContractPriceRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_version(version: i64) -> Result<(), OrganizationContractPriceRepositoryError> {
    if version <= 0 || version == i64::MAX {
        Err(OrganizationContractPriceRepositoryError::InvalidInput)
    } else {
        Ok(())
    }
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
    model: organization_contract_prices::Model,
) -> Result<OrganizationContractPriceRecord, OrganizationContractPriceRepositoryError> {
    let protocol = model
        .protocol
        .parse::<Protocol>()
        .map_err(|_| invariant())?;
    if model.version <= 0
        || model
            .effective_until
            .is_some_and(|until| until <= model.effective_from)
    {
        return Err(invariant());
    }
    Ok(OrganizationContractPriceRecord {
        id: OrganizationContractPriceId::new(model.id).map_err(|_| invariant())?,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        model: model.model.as_str().to_owned(),
        protocol,
        prices: [
            model.input_price.expose(),
            model.output_price.expose(),
            model.cache_read_price.expose(),
            model.cache_creation_5m_price.expose(),
            model.cache_creation_1h_price.expose(),
        ],
        effective_from: model.effective_from,
        effective_until: model.effective_until,
        version: model.version,
        created_by_user_id: UserId::new(model.created_by_user_id).map_err(|_| invariant())?,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

async fn begin(
    pool: &DatabasePool,
) -> Result<DatabaseTransaction, OrganizationContractPriceRepositoryError> {
    pool.connection().begin().await.map_err(query_error)
}
async fn commit(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationContractPriceRepositoryError> {
    transaction.commit().await.map_err(query_error)
}
async fn rollback(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationContractPriceRepositoryError> {
    transaction.rollback().await.map_err(query_error)
}
fn query_error(_: sea_orm::DbErr) -> OrganizationContractPriceRepositoryError {
    OrganizationContractPriceRepositoryError::Query
}
fn invariant() -> OrganizationContractPriceRepositoryError {
    OrganizationContractPriceRepositoryError::Invariant
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(seconds: i64) -> TimeDateTimeWithTimeZone {
        TimeDateTimeWithTimeZone::from_unix_timestamp(seconds).expect("时间戳应有效")
    }

    #[test]
    fn adjacent_intervals_do_not_overlap() {
        assert!(!intervals_overlap(
            timestamp(0),
            Some(timestamp(10)),
            timestamp(10),
            None
        ));
        assert!(intervals_overlap(
            timestamp(0),
            Some(timestamp(11)),
            timestamp(10),
            None
        ));
    }

    #[test]
    fn open_interval_overlaps_only_future_ranges() {
        assert!(intervals_overlap(
            timestamp(10),
            None,
            timestamp(0),
            Some(timestamp(11))
        ));
        assert!(!intervals_overlap(
            timestamp(10),
            None,
            timestamp(0),
            Some(timestamp(10))
        ));
    }
}
