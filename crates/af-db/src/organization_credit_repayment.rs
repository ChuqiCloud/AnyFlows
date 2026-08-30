use std::{fmt, future::Future, time::Duration};

use af_domain::{
    OrganizationCreditCurrency, OrganizationCreditInvoiceKey, OrganizationCreditInvoiceStatus,
    OrganizationCreditRepaymentAllocation, OrganizationCreditRepaymentAllocationId,
    OrganizationCreditRepaymentAllocationKey, OrganizationCreditRepaymentAmount,
    OrganizationCreditRepaymentId, OrganizationCreditRepaymentKey,
    OrganizationCreditRepaymentSnapshot, OrganizationCreditRepaymentSourceKey,
    OrganizationCreditRepaymentSourceKind, OrganizationId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{
        organization_credit_invoices, organization_credit_repayment_allocations,
        organization_credit_repayments,
    },
};

/// 已持久化的企业还款安全投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationCreditRepaymentRecord {
    id: OrganizationCreditRepaymentId,
    snapshot: OrganizationCreditRepaymentSnapshot,
    created_at: TimeDateTimeWithTimeZone,
}

impl OrganizationCreditRepaymentRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationCreditRepaymentId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.snapshot.organization_id()
    }
    #[must_use]
    pub const fn repayment_key(&self) -> OrganizationCreditRepaymentKey {
        self.snapshot.repayment_key()
    }
    #[must_use]
    pub const fn source_kind(&self) -> OrganizationCreditRepaymentSourceKind {
        self.snapshot.source_kind()
    }
    #[must_use]
    pub const fn source_key(&self) -> OrganizationCreditRepaymentSourceKey {
        self.snapshot.source_key()
    }
    #[must_use]
    pub fn currency(&self) -> &OrganizationCreditCurrency {
        self.snapshot.currency()
    }
    #[must_use]
    pub const fn amount(&self) -> OrganizationCreditRepaymentAmount {
        self.snapshot.amount()
    }
    #[must_use]
    pub const fn received_at(&self) -> i64 {
        self.snapshot.received_at()
    }
    #[must_use]
    pub const fn created_at(&self) -> TimeDateTimeWithTimeZone {
        self.created_at
    }

    fn matches(&self, write: &OrganizationCreditRepaymentSnapshot) -> bool {
        &self.snapshot == write
    }
}

/// 企业还款登记的幂等结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationCreditRepaymentCreateOutcome {
    Created(OrganizationCreditRepaymentRecord),
    Existing(OrganizationCreditRepaymentRecord),
}

/// 已持久化的追加式核销分配安全投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationCreditRepaymentAllocationRecord {
    id: OrganizationCreditRepaymentAllocationId,
    snapshot: OrganizationCreditRepaymentAllocation,
    repayment_id: OrganizationCreditRepaymentId,
    invoice_id: af_domain::OrganizationCreditInvoiceId,
    invoice_settled: bool,
    created_at: TimeDateTimeWithTimeZone,
}

impl OrganizationCreditRepaymentAllocationRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationCreditRepaymentAllocationId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.snapshot.organization_id()
    }
    #[must_use]
    pub const fn allocation_key(&self) -> OrganizationCreditRepaymentAllocationKey {
        self.snapshot.allocation_key()
    }
    #[must_use]
    pub const fn repayment_key(&self) -> OrganizationCreditRepaymentKey {
        self.snapshot.repayment_key()
    }
    #[must_use]
    pub const fn invoice_key(&self) -> OrganizationCreditInvoiceKey {
        self.snapshot.invoice_key()
    }
    #[must_use]
    pub const fn repayment_id(&self) -> OrganizationCreditRepaymentId {
        self.repayment_id
    }
    #[must_use]
    pub const fn invoice_id(&self) -> af_domain::OrganizationCreditInvoiceId {
        self.invoice_id
    }
    #[must_use]
    pub fn currency(&self) -> &OrganizationCreditCurrency {
        self.snapshot.currency()
    }
    #[must_use]
    pub const fn amount(&self) -> OrganizationCreditRepaymentAmount {
        self.snapshot.amount()
    }
    #[must_use]
    pub const fn allocated_at(&self) -> i64 {
        self.snapshot.allocated_at()
    }
    #[must_use]
    pub const fn invoice_settled(&self) -> bool {
        self.invoice_settled
    }
    #[must_use]
    pub const fn created_at(&self) -> TimeDateTimeWithTimeZone {
        self.created_at
    }

    fn matches(&self, write: &OrganizationCreditRepaymentAllocation) -> bool {
        &self.snapshot == write
    }
}

/// 企业还款核销分配的幂等结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationCreditRepaymentAllocationOutcome {
    Created(OrganizationCreditRepaymentAllocationRecord),
    Existing(OrganizationCreditRepaymentAllocationRecord),
}

/// 企业还款核销仓储错误；不携带来源、金额或主体原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditRepaymentRepositoryError {
    #[error("企业还款核销输入无效")]
    InvalidInput,
    #[error("企业还款或账单不存在")]
    NotFound,
    #[error("企业还款或核销幂等事实冲突")]
    Conflict,
    #[error("企业还款与账单币种不一致")]
    CurrencyMismatch,
    #[error("企业账期账单状态不允许核销")]
    InvalidInvoiceState,
    #[error("企业还款金额已被超额分配")]
    RepaymentOverallocated,
    #[error("企业账期账单已被超额核销")]
    InvoiceOverallocated,
    #[error("企业还款核销数据库操作失败")]
    Query,
    #[error("企业还款核销操作超时")]
    Timeout,
    #[error("企业还款核销持久化状态损坏")]
    Invariant,
}

/// 企业还款核销仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditRepaymentRepositoryConfigError {
    #[error("企业还款核销仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 企业范围还款核销仓储；核销固定按还款、账单顺序加锁。
#[derive(Clone)]
pub struct OrganizationCreditRepaymentRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationCreditRepaymentRepository {
    /// 使用共享连接池和非零操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationCreditRepaymentRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationCreditRepaymentRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 幂等登记一笔已确认企业还款；相同来源不能绑定不同事实。
    pub async fn record(
        &self,
        write: OrganizationCreditRepaymentSnapshot,
    ) -> Result<OrganizationCreditRepaymentCreateOutcome, OrganizationCreditRepaymentRepositoryError>
    {
        self.run(self.record_inner(write)).await
    }

    /// 追加一条核销分配，并在事务内保护还款与账单不被超额分配。
    pub async fn allocate(
        &self,
        write: OrganizationCreditRepaymentAllocation,
    ) -> Result<
        OrganizationCreditRepaymentAllocationOutcome,
        OrganizationCreditRepaymentRepositoryError,
    > {
        self.run(self.allocate_inner(write)).await
    }

    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, OrganizationCreditRepaymentRepositoryError>>,
    ) -> Result<T, OrganizationCreditRepaymentRepositoryError> {
        timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| OrganizationCreditRepaymentRepositoryError::Timeout)?
    }

    async fn record_inner(
        &self,
        write: OrganizationCreditRepaymentSnapshot,
    ) -> Result<OrganizationCreditRepaymentCreateOutcome, OrganizationCreditRepaymentRepositoryError>
    {
        if let Some(existing) =
            find_repayment_by_key(self.pool.connection(), write.repayment_key()).await?
        {
            return existing_repayment_outcome(existing, &write);
        }
        if let Some(existing) = find_repayment_by_source(
            self.pool.connection(),
            write.organization_id(),
            write.source_kind(),
            write.source_key(),
        )
        .await?
        {
            return existing_repayment_outcome(existing, &write);
        }

        let now = TimeDateTimeWithTimeZone::now_utc();
        let received_at = to_database_time(write.received_at())?;
        if received_at > now {
            return Err(OrganizationCreditRepaymentRepositoryError::InvalidInput);
        }
        let model = organization_credit_repayments::ActiveModel {
            repayment_key: Set(write.repayment_key().persistence_key()),
            organization_id: Set(write.organization_id().get()),
            source_kind: Set(write.source_kind().code()),
            source_key: Set(write.source_key().persistence_key()),
            currency: Set(write.currency().as_str().to_owned()),
            amount_minor: Set(write.amount().units()),
            received_at: Set(received_at),
            created_at: Set(now),
            ..Default::default()
        };
        match model.insert(self.pool.connection()).await {
            Ok(model) => Ok(OrganizationCreditRepaymentCreateOutcome::Created(
                repayment_record(model)?,
            )),
            Err(_) => recover_repayment(&self.pool, &write).await,
        }
    }

    async fn allocate_inner(
        &self,
        write: OrganizationCreditRepaymentAllocation,
    ) -> Result<
        OrganizationCreditRepaymentAllocationOutcome,
        OrganizationCreditRepaymentRepositoryError,
    > {
        if let Some(existing) =
            find_allocation_by_key(self.pool.connection(), write.allocation_key()).await?
        {
            return existing_allocation_outcome(self.pool.connection(), existing, &write).await;
        }

        let now = TimeDateTimeWithTimeZone::now_utc();
        let allocated_at = to_database_time(write.allocated_at())?;
        if allocated_at > now {
            return Err(OrganizationCreditRepaymentRepositoryError::InvalidInput);
        }
        let transaction = self.pool.connection().begin().await.map_err(query_error)?;
        let repayment =
            lock_repayment(&transaction, write.organization_id(), write.repayment_key()).await?;
        let invoice =
            lock_invoice(&transaction, write.organization_id(), write.invoice_key()).await?;

        if repayment.currency != invoice.currency || repayment.currency != write.currency().as_str()
        {
            rollback(transaction).await?;
            return Err(OrganizationCreditRepaymentRepositoryError::CurrencyMismatch);
        }
        if allocated_at < repayment.received_at || allocated_at < invoice.issued_at {
            rollback(transaction).await?;
            return Err(OrganizationCreditRepaymentRepositoryError::InvalidInput);
        }
        let invoice_status =
            OrganizationCreditInvoiceStatus::try_from(invoice.status).map_err(|_| invariant())?;
        if !matches!(
            invoice_status,
            OrganizationCreditInvoiceStatus::Due | OrganizationCreditInvoiceStatus::Overdue
        ) {
            rollback(transaction).await?;
            return Err(OrganizationCreditRepaymentRepositoryError::InvalidInvoiceState);
        }

        let repayment_allocated =
            sum_allocations_for_repayment(&transaction, write.organization_id(), repayment.id)
                .await?;
        let invoice_allocated =
            sum_allocations_for_invoice(&transaction, write.organization_id(), invoice.id).await?;
        let repayment_after = repayment_allocated
            .checked_add(write.amount().units())
            .ok_or_else(invariant)?;
        if repayment_after > repayment.amount_minor {
            rollback(transaction).await?;
            return Err(OrganizationCreditRepaymentRepositoryError::RepaymentOverallocated);
        }
        let invoice_after = invoice_allocated
            .checked_add(write.amount().units())
            .ok_or_else(invariant)?;
        if invoice_after > invoice.amount_minor {
            rollback(transaction).await?;
            return Err(OrganizationCreditRepaymentRepositoryError::InvoiceOverallocated);
        }

        let active = organization_credit_repayment_allocations::ActiveModel {
            allocation_key: Set(write.allocation_key().persistence_key()),
            organization_id: Set(write.organization_id().get()),
            repayment_id: Set(repayment.id),
            invoice_id: Set(invoice.id),
            currency: Set(repayment.currency.clone()),
            amount_minor: Set(write.amount().units()),
            allocated_at: Set(allocated_at),
            created_at: Set(now),
            ..Default::default()
        };
        let allocation = match active.insert(&transaction).await {
            Ok(model) => model,
            Err(_) => {
                rollback(transaction).await?;
                return recover_allocation(&self.pool, &write).await;
            }
        };

        let invoice_settled = invoice_after == invoice.amount_minor;
        if invoice_settled {
            settle_invoice(&transaction, &invoice, allocated_at, now).await?;
        }
        let record = allocation_record(allocation, &repayment, &invoice, invoice_settled)?;
        match transaction.commit().await {
            Ok(()) => Ok(OrganizationCreditRepaymentAllocationOutcome::Created(
                record,
            )),
            Err(_) => recover_allocation(&self.pool, &write).await,
        }
    }
}

impl fmt::Debug for OrganizationCreditRepaymentRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationCreditRepaymentRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn lock_repayment(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    key: OrganizationCreditRepaymentKey,
) -> Result<organization_credit_repayments::Model, OrganizationCreditRepaymentRepositoryError> {
    let key = key.persistence_key();
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = organization_credit_repayments::Entity::update_many()
            .filter(
                organization_credit_repayments::Column::OrganizationId.eq(organization_id.get()),
            )
            .filter(organization_credit_repayments::Column::RepaymentKey.eq(&key))
            .col_expr(
                organization_credit_repayments::Column::Id,
                Expr::col(organization_credit_repayments::Column::Id),
            )
            .exec(transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            return Err(OrganizationCreditRepaymentRepositoryError::NotFound);
        }
    }
    let mut query = organization_credit_repayments::Entity::find()
        .filter(organization_credit_repayments::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_credit_repayments::Column::RepaymentKey.eq(key));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationCreditRepaymentRepositoryError::NotFound)
}

async fn lock_invoice(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    key: OrganizationCreditInvoiceKey,
) -> Result<organization_credit_invoices::Model, OrganizationCreditRepaymentRepositoryError> {
    let key = key.persistence_key();
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = organization_credit_invoices::Entity::update_many()
            .filter(organization_credit_invoices::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_credit_invoices::Column::InvoiceKey.eq(&key))
            .col_expr(
                organization_credit_invoices::Column::Id,
                Expr::col(organization_credit_invoices::Column::Id),
            )
            .exec(transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            return Err(OrganizationCreditRepaymentRepositoryError::NotFound);
        }
    }
    let mut query = organization_credit_invoices::Entity::find()
        .filter(organization_credit_invoices::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_credit_invoices::Column::InvoiceKey.eq(key));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationCreditRepaymentRepositoryError::NotFound)
}

async fn sum_allocations_for_repayment(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    repayment_id: i64,
) -> Result<i64, OrganizationCreditRepaymentRepositoryError> {
    let rows = organization_credit_repayment_allocations::Entity::find()
        .filter(
            organization_credit_repayment_allocations::Column::OrganizationId
                .eq(organization_id.get()),
        )
        .filter(organization_credit_repayment_allocations::Column::RepaymentId.eq(repayment_id))
        .all(transaction)
        .await
        .map_err(query_error)?;
    checked_allocation_sum(&rows)
}

async fn sum_allocations_for_invoice(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    invoice_id: i64,
) -> Result<i64, OrganizationCreditRepaymentRepositoryError> {
    let rows = organization_credit_repayment_allocations::Entity::find()
        .filter(
            organization_credit_repayment_allocations::Column::OrganizationId
                .eq(organization_id.get()),
        )
        .filter(organization_credit_repayment_allocations::Column::InvoiceId.eq(invoice_id))
        .all(transaction)
        .await
        .map_err(query_error)?;
    checked_allocation_sum(&rows)
}

fn checked_allocation_sum(
    rows: &[organization_credit_repayment_allocations::Model],
) -> Result<i64, OrganizationCreditRepaymentRepositoryError> {
    rows.iter().try_fold(0_i64, |total, row| {
        if row.amount_minor <= 0 {
            return Err(invariant());
        }
        total.checked_add(row.amount_minor).ok_or_else(invariant)
    })
}

async fn settle_invoice(
    transaction: &DatabaseTransaction,
    invoice: &organization_credit_invoices::Model,
    settled_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationCreditRepaymentRepositoryError> {
    let next_version = invoice
        .version
        .checked_add(1)
        .filter(|value| *value > 0)
        .ok_or_else(invariant)?;
    let result = organization_credit_invoices::Entity::update_many()
        .filter(organization_credit_invoices::Column::Id.eq(invoice.id))
        .filter(organization_credit_invoices::Column::OrganizationId.eq(invoice.organization_id))
        .filter(organization_credit_invoices::Column::Version.eq(invoice.version))
        .filter(organization_credit_invoices::Column::Status.is_in([
            OrganizationCreditInvoiceStatus::Due.code(),
            OrganizationCreditInvoiceStatus::Overdue.code(),
        ]))
        .col_expr(
            organization_credit_invoices::Column::Status,
            Expr::value(OrganizationCreditInvoiceStatus::Settled.code()),
        )
        .col_expr(
            organization_credit_invoices::Column::StatusChangedAt,
            Expr::value(settled_at),
        )
        .col_expr(
            organization_credit_invoices::Column::SettledAt,
            Expr::value(settled_at),
        )
        .col_expr(
            organization_credit_invoices::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            organization_credit_invoices::Column::UpdatedAt,
            Expr::value(updated_at),
        )
        .exec(transaction)
        .await
        .map_err(query_error)?;
    if result.rows_affected != 1 {
        return Err(OrganizationCreditRepaymentRepositoryError::Conflict);
    }
    Ok(())
}

async fn recover_repayment(
    pool: &DatabasePool,
    write: &OrganizationCreditRepaymentSnapshot,
) -> Result<OrganizationCreditRepaymentCreateOutcome, OrganizationCreditRepaymentRepositoryError> {
    if let Some(existing) = find_repayment_by_key(pool.connection(), write.repayment_key()).await? {
        return existing_repayment_outcome(existing, write);
    }
    if let Some(existing) = find_repayment_by_source(
        pool.connection(),
        write.organization_id(),
        write.source_kind(),
        write.source_key(),
    )
    .await?
    {
        return existing_repayment_outcome(existing, write);
    }
    Err(OrganizationCreditRepaymentRepositoryError::Query)
}

fn existing_repayment_outcome(
    model: organization_credit_repayments::Model,
    write: &OrganizationCreditRepaymentSnapshot,
) -> Result<OrganizationCreditRepaymentCreateOutcome, OrganizationCreditRepaymentRepositoryError> {
    let record = repayment_record(model)?;
    if record.matches(write) {
        Ok(OrganizationCreditRepaymentCreateOutcome::Existing(record))
    } else {
        Err(OrganizationCreditRepaymentRepositoryError::Conflict)
    }
}

async fn recover_allocation(
    pool: &DatabasePool,
    write: &OrganizationCreditRepaymentAllocation,
) -> Result<OrganizationCreditRepaymentAllocationOutcome, OrganizationCreditRepaymentRepositoryError>
{
    let existing = find_allocation_by_key(pool.connection(), write.allocation_key())
        .await?
        .ok_or(OrganizationCreditRepaymentRepositoryError::Query)?;
    existing_allocation_outcome(pool.connection(), existing, write).await
}

async fn existing_allocation_outcome<C>(
    connection: &C,
    model: organization_credit_repayment_allocations::Model,
    write: &OrganizationCreditRepaymentAllocation,
) -> Result<OrganizationCreditRepaymentAllocationOutcome, OrganizationCreditRepaymentRepositoryError>
where
    C: ConnectionTrait,
{
    let repayment = organization_credit_repayments::Entity::find_by_id(model.repayment_id)
        .one(connection)
        .await
        .map_err(query_error)?
        .ok_or_else(invariant)?;
    let invoice = organization_credit_invoices::Entity::find_by_id(model.invoice_id)
        .one(connection)
        .await
        .map_err(query_error)?
        .ok_or_else(invariant)?;
    let invoice_status =
        OrganizationCreditInvoiceStatus::try_from(invoice.status).map_err(|_| invariant())?;
    let record = allocation_record(
        model,
        &repayment,
        &invoice,
        invoice_status == OrganizationCreditInvoiceStatus::Settled,
    )?;
    if record.matches(write) {
        Ok(OrganizationCreditRepaymentAllocationOutcome::Existing(
            record,
        ))
    } else {
        Err(OrganizationCreditRepaymentRepositoryError::Conflict)
    }
}

async fn find_repayment_by_key<C>(
    connection: &C,
    key: OrganizationCreditRepaymentKey,
) -> Result<Option<organization_credit_repayments::Model>, OrganizationCreditRepaymentRepositoryError>
where
    C: ConnectionTrait,
{
    organization_credit_repayments::Entity::find()
        .filter(organization_credit_repayments::Column::RepaymentKey.eq(key.persistence_key()))
        .one(connection)
        .await
        .map_err(query_error)
}

async fn find_repayment_by_source<C>(
    connection: &C,
    organization_id: OrganizationId,
    source_kind: OrganizationCreditRepaymentSourceKind,
    source_key: OrganizationCreditRepaymentSourceKey,
) -> Result<Option<organization_credit_repayments::Model>, OrganizationCreditRepaymentRepositoryError>
where
    C: ConnectionTrait,
{
    organization_credit_repayments::Entity::find()
        .filter(organization_credit_repayments::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_credit_repayments::Column::SourceKind.eq(source_kind.code()))
        .filter(organization_credit_repayments::Column::SourceKey.eq(source_key.persistence_key()))
        .one(connection)
        .await
        .map_err(query_error)
}

async fn find_allocation_by_key<C>(
    connection: &C,
    key: OrganizationCreditRepaymentAllocationKey,
) -> Result<
    Option<organization_credit_repayment_allocations::Model>,
    OrganizationCreditRepaymentRepositoryError,
>
where
    C: ConnectionTrait,
{
    organization_credit_repayment_allocations::Entity::find()
        .filter(
            organization_credit_repayment_allocations::Column::AllocationKey
                .eq(key.persistence_key()),
        )
        .one(connection)
        .await
        .map_err(query_error)
}

fn repayment_record(
    model: organization_credit_repayments::Model,
) -> Result<OrganizationCreditRepaymentRecord, OrganizationCreditRepaymentRepositoryError> {
    let snapshot = OrganizationCreditRepaymentSnapshot::new(
        OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        OrganizationCreditRepaymentKey::from_persistence_key(&model.repayment_key)
            .map_err(|_| invariant())?,
        OrganizationCreditRepaymentSourceKind::try_from(model.source_kind)
            .map_err(|_| invariant())?,
        OrganizationCreditRepaymentSourceKey::from_persistence_key(&model.source_key)
            .map_err(|_| invariant())?,
        OrganizationCreditCurrency::new(&model.currency).map_err(|_| invariant())?,
        OrganizationCreditRepaymentAmount::new(model.amount_minor).map_err(|_| invariant())?,
        model.received_at.unix_timestamp(),
    )
    .map_err(|_| invariant())?;
    if model.created_at < model.received_at {
        return Err(invariant());
    }
    Ok(OrganizationCreditRepaymentRecord {
        id: OrganizationCreditRepaymentId::new(model.id).map_err(|_| invariant())?,
        snapshot,
        created_at: model.created_at,
    })
}

fn allocation_record(
    model: organization_credit_repayment_allocations::Model,
    repayment: &organization_credit_repayments::Model,
    invoice: &organization_credit_invoices::Model,
    invoice_settled: bool,
) -> Result<OrganizationCreditRepaymentAllocationRecord, OrganizationCreditRepaymentRepositoryError>
{
    if model.organization_id != repayment.organization_id
        || model.organization_id != invoice.organization_id
        || model.currency != repayment.currency
        || model.currency != invoice.currency
        || model.created_at < model.allocated_at
    {
        return Err(invariant());
    }
    let snapshot = OrganizationCreditRepaymentAllocation::new(
        OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        OrganizationCreditRepaymentAllocationKey::from_persistence_key(&model.allocation_key)
            .map_err(|_| invariant())?,
        OrganizationCreditRepaymentKey::from_persistence_key(&repayment.repayment_key)
            .map_err(|_| invariant())?,
        OrganizationCreditInvoiceKey::from_persistence_key(&invoice.invoice_key)
            .map_err(|_| invariant())?,
        OrganizationCreditCurrency::new(&model.currency).map_err(|_| invariant())?,
        OrganizationCreditRepaymentAmount::new(model.amount_minor).map_err(|_| invariant())?,
        model.allocated_at.unix_timestamp(),
    )
    .map_err(|_| invariant())?;
    Ok(OrganizationCreditRepaymentAllocationRecord {
        id: OrganizationCreditRepaymentAllocationId::new(model.id).map_err(|_| invariant())?,
        snapshot,
        repayment_id: OrganizationCreditRepaymentId::new(model.repayment_id)
            .map_err(|_| invariant())?,
        invoice_id: af_domain::OrganizationCreditInvoiceId::new(model.invoice_id)
            .map_err(|_| invariant())?,
        invoice_settled,
        created_at: model.created_at,
    })
}

fn to_database_time(
    value: i64,
) -> Result<TimeDateTimeWithTimeZone, OrganizationCreditRepaymentRepositoryError> {
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| OrganizationCreditRepaymentRepositoryError::InvalidInput)
}

async fn rollback(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationCreditRepaymentRepositoryError> {
    transaction.rollback().await.map_err(query_error)
}

fn query_error(_: sea_orm::DbErr) -> OrganizationCreditRepaymentRepositoryError {
    OrganizationCreditRepaymentRepositoryError::Query
}

fn invariant() -> OrganizationCreditRepaymentRepositoryError {
    OrganizationCreditRepaymentRepositoryError::Invariant
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_sum_rejects_non_positive_and_overflowed_facts() {
        let row = |amount_minor| organization_credit_repayment_allocations::Model {
            id: 1,
            allocation_key: "1".repeat(32),
            organization_id: 1,
            repayment_id: 1,
            invoice_id: 1,
            currency: "CNY".to_owned(),
            amount_minor,
            allocated_at: TimeDateTimeWithTimeZone::from_unix_timestamp(0).expect("时间有效"),
            created_at: TimeDateTimeWithTimeZone::from_unix_timestamp(0).expect("时间有效"),
        };
        assert_eq!(checked_allocation_sum(&[row(1), row(2)]), Ok(3));
        assert_eq!(checked_allocation_sum(&[row(0)]), Err(invariant()));
        assert_eq!(
            checked_allocation_sum(&[row(i64::MAX), row(1)]),
            Err(invariant())
        );
    }
}
