use std::{fmt, future::Future, time::Duration};

use af_domain::{
    OrganizationCreditCurrency, OrganizationCreditInvoiceAmount, OrganizationCreditInvoiceId,
    OrganizationCreditInvoiceKey, OrganizationCreditInvoiceSnapshot,
    OrganizationCreditInvoiceSourceKey, OrganizationCreditInvoiceSourceKind,
    OrganizationCreditInvoiceStatus, OrganizationCreditTermPeriod, OrganizationId,
};
use sea_orm::{ColumnTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{organization_credit_invoices, organizations},
};

/// 企业账单查询允许的最大页大小。
pub const MAX_ORGANIZATION_CREDIT_INVOICE_PAGE_SIZE: usize = 100;

/// 已持久化的企业账单安全投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationCreditInvoiceRecord {
    id: OrganizationCreditInvoiceId,
    snapshot: OrganizationCreditInvoiceSnapshot,
    status_changed_at: i64,
    settled_at: Option<i64>,
    voided_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

impl OrganizationCreditInvoiceRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationCreditInvoiceId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.snapshot.organization_id()
    }
    #[must_use]
    pub const fn invoice_key(&self) -> OrganizationCreditInvoiceKey {
        self.snapshot.invoice_key()
    }
    #[must_use]
    pub const fn source_kind(&self) -> OrganizationCreditInvoiceSourceKind {
        self.snapshot.source_kind()
    }
    #[must_use]
    pub const fn source_key(&self) -> OrganizationCreditInvoiceSourceKey {
        self.snapshot.source_key()
    }
    #[must_use]
    pub fn currency(&self) -> &OrganizationCreditCurrency {
        self.snapshot.currency()
    }
    #[must_use]
    pub const fn amount(&self) -> OrganizationCreditInvoiceAmount {
        self.snapshot.amount()
    }
    #[must_use]
    pub const fn term_version(&self) -> i64 {
        self.snapshot.term_version()
    }
    #[must_use]
    pub const fn period(&self) -> OrganizationCreditTermPeriod {
        self.snapshot.period()
    }
    #[must_use]
    pub const fn issued_at(&self) -> i64 {
        self.snapshot.issued_at()
    }
    #[must_use]
    pub const fn due_at(&self) -> i64 {
        self.snapshot.due_at()
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationCreditInvoiceStatus {
        self.snapshot.status()
    }
    #[must_use]
    pub const fn status_changed_at(&self) -> i64 {
        self.status_changed_at
    }
    #[must_use]
    pub const fn settled_at(&self) -> Option<i64> {
        self.settled_at
    }
    #[must_use]
    pub const fn voided_at(&self) -> Option<i64> {
        self.voided_at
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

/// 一页企业账单结果。
pub struct OrganizationCreditInvoicePageRecord {
    invoices: Vec<OrganizationCreditInvoiceRecord>,
    next_cursor: Option<i64>,
}

impl OrganizationCreditInvoicePageRecord {
    /// 消费页面并返回账单和下一页游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<OrganizationCreditInvoiceRecord>, Option<i64>) {
        (self.invoices, self.next_cursor)
    }
}

/// 企业账单查询错误；错误不携带来源或资金字段。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditInvoiceRepositoryError {
    #[error("企业账单查询输入无效")]
    InvalidInput,
    #[error("企业账单不存在")]
    NotFound,
    #[error("企业不可用")]
    OrganizationUnavailable,
    #[error("企业账单数据库查询失败")]
    Query,
    #[error("企业账单查询超时")]
    Timeout,
    #[error("企业账单持久化状态损坏")]
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditInvoiceRepositoryConfigError {
    #[error("企业账单查询超时必须大于零")]
    ZeroOperationTimeout,
}

/// 企业范围账单只读仓储；所有查询均带企业过滤条件。
#[derive(Clone)]
pub struct OrganizationCreditInvoiceRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationCreditInvoiceRepository {
    /// 使用共享连接池和非零查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationCreditInvoiceRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationCreditInvoiceRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }
    /// 按账单 ID 从新到旧读取一页账单事实。
    pub async fn list(
        &self,
        organization_id: OrganizationId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<OrganizationCreditInvoicePageRecord, OrganizationCreditInvoiceRepositoryError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_ORGANIZATION_CREDIT_INVOICE_PAGE_SIZE).contains(&limit)
        {
            return Err(OrganizationCreditInvoiceRepositoryError::InvalidInput);
        }
        self.run(self.list_inner(organization_id, before, limit))
            .await
    }
    /// 按企业和账单 ID 读取账单事实。
    pub async fn get(
        &self,
        organization_id: OrganizationId,
        id: OrganizationCreditInvoiceId,
    ) -> Result<OrganizationCreditInvoiceRecord, OrganizationCreditInvoiceRepositoryError> {
        self.run(self.get_inner(organization_id, id)).await
    }
    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, OrganizationCreditInvoiceRepositoryError>>,
    ) -> Result<T, OrganizationCreditInvoiceRepositoryError> {
        timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| OrganizationCreditInvoiceRepositoryError::Timeout)?
    }
    async fn list_inner(
        &self,
        organization_id: OrganizationId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<OrganizationCreditInvoicePageRecord, OrganizationCreditInvoiceRepositoryError> {
        ensure_active_organization(&self.pool, organization_id).await?;
        let mut query = organization_credit_invoices::Entity::find()
            .filter(organization_credit_invoices::Column::OrganizationId.eq(organization_id.get()))
            .order_by_desc(organization_credit_invoices::Column::Id)
            .limit((limit + 1) as u64);
        if let Some(before) = before {
            query = query.filter(organization_credit_invoices::Column::Id.lt(before));
        }
        let mut models = query
            .all(self.pool.connection())
            .await
            .map_err(query_error)?;
        let has_more = models.len() > limit;
        if has_more {
            models.truncate(limit);
        }
        let invoices = models
            .into_iter()
            .map(record_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| invoices.last().map(|record| record.id().get()))
            .flatten();
        Ok(OrganizationCreditInvoicePageRecord {
            invoices,
            next_cursor,
        })
    }
    async fn get_inner(
        &self,
        organization_id: OrganizationId,
        id: OrganizationCreditInvoiceId,
    ) -> Result<OrganizationCreditInvoiceRecord, OrganizationCreditInvoiceRepositoryError> {
        ensure_active_organization(&self.pool, organization_id).await?;
        organization_credit_invoices::Entity::find_by_id(id.get())
            .filter(organization_credit_invoices::Column::OrganizationId.eq(organization_id.get()))
            .one(self.pool.connection())
            .await
            .map_err(query_error)?
            .ok_or(OrganizationCreditInvoiceRepositoryError::NotFound)
            .and_then(record_from_model)
    }
}

impl fmt::Debug for OrganizationCreditInvoiceRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationCreditInvoiceRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn ensure_active_organization(
    pool: &DatabasePool,
    organization_id: OrganizationId,
) -> Result<(), OrganizationCreditInvoiceRepositoryError> {
    if organizations::Entity::find_by_id(organization_id.get())
        .filter(organizations::Column::Status.eq(1_i16))
        .one(pool.connection())
        .await
        .map_err(query_error)?
        .is_none()
    {
        return Err(OrganizationCreditInvoiceRepositoryError::NotFound);
    }
    Ok(())
}

fn record_from_model(
    model: organization_credit_invoices::Model,
) -> Result<OrganizationCreditInvoiceRecord, OrganizationCreditInvoiceRepositoryError> {
    let issued_at = model.issued_at.unix_timestamp();
    let due_at = model.due_at.unix_timestamp();
    let status_changed_at = model.status_changed_at.unix_timestamp();
    let settled_at = model.settled_at.map(|value| value.unix_timestamp());
    let voided_at = model.voided_at.map(|value| value.unix_timestamp());
    let status =
        OrganizationCreditInvoiceStatus::try_from(model.status).map_err(|_| invariant())?;
    let status_shape_valid = match status {
        OrganizationCreditInvoiceStatus::Settled => settled_at.is_some() && voided_at.is_none(),
        OrganizationCreditInvoiceStatus::Voided => settled_at.is_none() && voided_at.is_some(),
        OrganizationCreditInvoiceStatus::Due | OrganizationCreditInvoiceStatus::Overdue => {
            settled_at.is_none() && voided_at.is_none()
        }
    };
    if !status_shape_valid
        || model.updated_at < model.created_at
        || model.created_at.unix_timestamp() < 0
        || model.updated_at.unix_timestamp() < 0
        || settled_at.is_some_and(|value| value < issued_at)
        || voided_at.is_some_and(|value| value < issued_at)
    {
        return Err(invariant());
    }
    let period_days = model
        .period_days
        .map(|value| u16::try_from(value).map_err(|_| invariant()))
        .transpose()?;
    let snapshot = OrganizationCreditInvoiceSnapshot::new(
        OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        OrganizationCreditInvoiceKey::from_persistence_key(&model.invoice_key)
            .map_err(|_| invariant())?,
        OrganizationCreditInvoiceSourceKind::try_from(model.source_kind)
            .map_err(|_| invariant())?,
        OrganizationCreditInvoiceSourceKey::from_persistence_key(&model.source_key)
            .map_err(|_| invariant())?,
        OrganizationCreditCurrency::new(model.currency).map_err(|_| invariant())?,
        OrganizationCreditInvoiceAmount::new(model.amount_minor).map_err(|_| invariant())?,
        model.term_version,
        OrganizationCreditTermPeriod::from_persistence(model.period_kind, period_days)
            .map_err(|_| invariant())?,
        issued_at,
        due_at,
        status,
        status_changed_at,
        model.version,
    )
    .map_err(|_| invariant())?;
    Ok(OrganizationCreditInvoiceRecord {
        id: OrganizationCreditInvoiceId::new(model.id).map_err(|_| invariant())?,
        snapshot,
        status_changed_at,
        settled_at,
        voided_at,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
    })
}

fn query_error(_: DbErr) -> OrganizationCreditInvoiceRepositoryError {
    OrganizationCreditInvoiceRepositoryError::Query
}
fn invariant() -> OrganizationCreditInvoiceRepositoryError {
    OrganizationCreditInvoiceRepositoryError::Invariant
}
