use std::{fmt, time::Duration};

use af_domain::{
    BillingReservationId, OrganizationId, OrganizationServiceAccountId,
    OrganizationServiceAccountKey, OrganizationServiceAccountRuntimeIdentity,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::LockType,
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    DatabasePool,
    entity::{
        organization_service_account_audit_logs, organization_service_account_keys,
        organization_service_accounts, usage_logs,
    },
};

const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
/// 服务账号调用审计单页允许返回的最大记录数。
pub const MAX_ORGANIZATION_SERVICE_ACCOUNT_AUDIT_PAGE_SIZE: usize = 100;

/// 已通过运行时鉴权的服务账号调用身份写入事实。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct OrganizationServiceAccountAuditWrite {
    event_id: BillingReservationId,
    identity: OrganizationServiceAccountRuntimeIdentity,
}

impl OrganizationServiceAccountAuditWrite {
    /// 组合已落库用量事件与脱敏服务账号身份；不接收密钥明文或摘要。
    #[must_use]
    pub const fn new(
        event_id: BillingReservationId,
        identity: OrganizationServiceAccountRuntimeIdentity,
    ) -> Self {
        Self { event_id, identity }
    }
}

impl fmt::Debug for OrganizationServiceAccountAuditWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationServiceAccountAuditWrite(<redacted>)")
    }
}

/// 服务账号调用身份审计读取记录。
pub struct OrganizationServiceAccountAuditRecord {
    id: i64,
    event_id: BillingReservationId,
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
    service_account_key: OrganizationServiceAccountKey,
    credential_key: OrganizationServiceAccountKey,
    created_at: i64,
}

impl OrganizationServiceAccountAuditRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    #[must_use]
    pub const fn event_id(&self) -> BillingReservationId {
        self.event_id
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    #[must_use]
    pub const fn service_account_id(&self) -> OrganizationServiceAccountId {
        self.service_account_id
    }

    #[must_use]
    pub const fn service_account_key(&self) -> OrganizationServiceAccountKey {
        self.service_account_key
    }

    #[must_use]
    pub const fn credential_key(&self) -> OrganizationServiceAccountKey {
        self.credential_key
    }

    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
}

impl fmt::Debug for OrganizationServiceAccountAuditRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationServiceAccountAuditRecord(<redacted>)")
    }
}

/// 服务账号调用审计的企业范围稳定游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationServiceAccountAuditQuery {
    before: Option<i64>,
    limit: usize,
    service_account_key: Option<OrganizationServiceAccountKey>,
}

impl OrganizationServiceAccountAuditQuery {
    /// 校验游标、页大小和可选服务账号公开标识。
    pub fn new(
        before: Option<i64>,
        limit: usize,
        service_account_key: Option<OrganizationServiceAccountKey>,
    ) -> Result<Self, OrganizationServiceAccountAuditRepositoryError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_ORGANIZATION_SERVICE_ACCOUNT_AUDIT_PAGE_SIZE).contains(&limit)
        {
            return Err(OrganizationServiceAccountAuditRepositoryError::InvalidInput);
        }
        Ok(Self {
            before,
            limit,
            service_account_key,
        })
    }

    #[must_use]
    pub const fn before(self) -> Option<i64> {
        self.before
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }

    #[must_use]
    pub const fn service_account_key(self) -> Option<OrganizationServiceAccountKey> {
        self.service_account_key
    }
}

/// 服务账号调用身份投影的稳定游标页。
pub struct OrganizationServiceAccountAuditPage {
    records: Vec<OrganizationServiceAccountAuditRecord>,
    next_cursor: Option<i64>,
}

impl OrganizationServiceAccountAuditPage {
    /// 消费页面并返回记录与下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<OrganizationServiceAccountAuditRecord>, Option<i64>) {
        (self.records, self.next_cursor)
    }
}

/// 服务账号调用身份投影仓储错误；不携带主体、密钥或请求原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationServiceAccountAuditRepositoryError {
    #[error("服务账号调用审计输入无效")]
    InvalidInput,
    #[error("服务账号调用对应的用量事实不存在")]
    UsageNotFound,
    #[error("服务账号调用主体不存在")]
    AccountNotFound,
    #[error("服务账号调用密钥事实不存在")]
    CredentialNotFound,
    #[error("服务账号调用审计数据库查询失败")]
    Query,
    #[error("服务账号调用审计写入结果未知")]
    OutcomeUnknown,
    #[error("服务账号调用审计幂等事实冲突")]
    Conflict,
    #[error("服务账号调用审计持久化状态损坏")]
    Invariant,
    #[error("服务账号调用审计操作超时")]
    Timeout,
}

/// 企业范围只追加服务账号调用身份投影仓储。
#[derive(Clone)]
pub struct OrganizationServiceAccountAuditRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationServiceAccountAuditRepository {
    /// 使用默认五秒截止时间构造仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
        }
    }

    /// 使用显式非零截止时间构造仓储。
    pub fn with_operation_timeout(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationServiceAccountAuditRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationServiceAccountAuditRepositoryError::InvalidInput);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 幂等追加身份投影；调用方必须先把同一 `event_id` 的用量事实落库。
    pub async fn record(
        &self,
        write: &OrganizationServiceAccountAuditWrite,
    ) -> Result<(), OrganizationServiceAccountAuditRepositoryError> {
        timeout(self.operation_timeout, self.record_inner(write))
            .await
            .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Timeout)?
    }

    /// 按企业范围读取服务账号调用身份投影。
    pub async fn list(
        &self,
        organization_id: OrganizationId,
        query: OrganizationServiceAccountAuditQuery,
    ) -> Result<OrganizationServiceAccountAuditPage, OrganizationServiceAccountAuditRepositoryError>
    {
        timeout(
            self.operation_timeout,
            self.list_inner(organization_id, query),
        )
        .await
        .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Timeout)?
    }

    async fn record_inner(
        &self,
        write: &OrganizationServiceAccountAuditWrite,
    ) -> Result<(), OrganizationServiceAccountAuditRepositoryError> {
        let event_id = write.event_id.persistence_key();
        let organization_id = write.identity.locator().organization_id();
        let service_account_id = write.identity.locator().service_account_id();
        let service_account_key = write.identity.account_key().persistence_key();
        let credential_key = write.identity.credential_key().persistence_key();
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationServiceAccountAuditRepositoryError::OutcomeUnknown)?;

        let usage = usage_logs::Entity::find()
            .filter(usage_logs::Column::EventId.eq(&event_id))
            .one(&transaction)
            .await
            .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Query)?
            .ok_or(OrganizationServiceAccountAuditRepositoryError::UsageNotFound)?;
        if usage.organization_id != Some(organization_id.get()) {
            rollback(transaction).await?;
            return Err(OrganizationServiceAccountAuditRepositoryError::Invariant);
        }

        let account = account_query(&transaction, organization_id, service_account_id)
            .await?
            .ok_or(OrganizationServiceAccountAuditRepositoryError::AccountNotFound)?;
        if account.public_key != service_account_key {
            rollback(transaction).await?;
            return Err(OrganizationServiceAccountAuditRepositoryError::Invariant);
        }

        let credential = organization_service_account_keys::Entity::find()
            .filter(organization_service_account_keys::Column::KeyKey.eq(&credential_key))
            .one(&transaction)
            .await
            .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Query)?
            .ok_or(OrganizationServiceAccountAuditRepositoryError::CredentialNotFound)?;
        if credential.organization_id != organization_id.get()
            || credential.service_account_id != service_account_id.get()
        {
            rollback(transaction).await?;
            return Err(OrganizationServiceAccountAuditRepositoryError::Invariant);
        }

        let model = organization_service_account_audit_logs::ActiveModel {
            event_id: Set(event_id),
            organization_id: Set(organization_id.get()),
            service_account_id: Set(service_account_id.get()),
            service_account_key: Set(service_account_key),
            credential_key: Set(credential_key),
            created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
            ..Default::default()
        };
        match model.insert(&transaction).await {
            Ok(_) => transaction
                .commit()
                .await
                .map_err(|_| OrganizationServiceAccountAuditRepositoryError::OutcomeUnknown),
            Err(error) if is_unique_violation(&error) => {
                rollback(transaction).await?;
                self.classify_existing(write).await
            }
            Err(_) => {
                rollback(transaction).await?;
                Err(OrganizationServiceAccountAuditRepositoryError::OutcomeUnknown)
            }
        }
    }

    async fn classify_existing(
        &self,
        write: &OrganizationServiceAccountAuditWrite,
    ) -> Result<(), OrganizationServiceAccountAuditRepositoryError> {
        let event_id = write.event_id.persistence_key();
        let existing = organization_service_account_audit_logs::Entity::find()
            .filter(organization_service_account_audit_logs::Column::EventId.eq(&event_id))
            .one(self.pool.connection())
            .await
            .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Query)?
            .ok_or(OrganizationServiceAccountAuditRepositoryError::Invariant)?;
        let identity = write.identity;
        if existing.organization_id == identity.locator().organization_id().get()
            && existing.service_account_id == identity.locator().service_account_id().get()
            && existing.service_account_key == identity.account_key().persistence_key()
            && existing.credential_key == identity.credential_key().persistence_key()
        {
            Ok(())
        } else {
            Err(OrganizationServiceAccountAuditRepositoryError::Conflict)
        }
    }

    async fn list_inner(
        &self,
        organization_id: OrganizationId,
        query: OrganizationServiceAccountAuditQuery,
    ) -> Result<OrganizationServiceAccountAuditPage, OrganizationServiceAccountAuditRepositoryError>
    {
        let mut select = organization_service_account_audit_logs::Entity::find()
            .filter(
                organization_service_account_audit_logs::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .order_by_desc(organization_service_account_audit_logs::Column::Id)
            .limit((query.limit() + 1) as u64);
        if let Some(before) = query.before() {
            select = select.filter(organization_service_account_audit_logs::Column::Id.lt(before));
        }
        if let Some(service_account_key) = query.service_account_key() {
            select = select.filter(
                organization_service_account_audit_logs::Column::ServiceAccountKey
                    .eq(service_account_key.persistence_key()),
            );
        }
        let mut models = select
            .all(self.pool.connection())
            .await
            .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Query)?;
        let has_more = models.len() > query.limit();
        if has_more {
            models.truncate(query.limit());
        }
        let records = models
            .into_iter()
            .map(to_record)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| {
                records
                    .last()
                    .map(OrganizationServiceAccountAuditRecord::id)
            })
            .flatten();
        Ok(OrganizationServiceAccountAuditPage {
            records,
            next_cursor,
        })
    }
}

impl fmt::Debug for OrganizationServiceAccountAuditRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationServiceAccountAuditRepository(<redacted>)")
    }
}

async fn account_query(
    transaction: &sea_orm::DatabaseTransaction,
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
) -> Result<
    Option<organization_service_accounts::Model>,
    OrganizationServiceAccountAuditRepositoryError,
> {
    let mut query = organization_service_accounts::Entity::find_by_id(service_account_id.get())
        .filter(organization_service_accounts::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Share);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Query)
}

async fn rollback(
    transaction: sea_orm::DatabaseTransaction,
) -> Result<(), OrganizationServiceAccountAuditRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| OrganizationServiceAccountAuditRepositoryError::OutcomeUnknown)
}

fn to_record(
    model: organization_service_account_audit_logs::Model,
) -> Result<OrganizationServiceAccountAuditRecord, OrganizationServiceAccountAuditRepositoryError> {
    let event_id = BillingReservationId::from_persistence_key(&model.event_id)
        .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Invariant)?;
    let organization_id = OrganizationId::new(model.organization_id)
        .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Invariant)?;
    let service_account_id = OrganizationServiceAccountId::new(model.service_account_id)
        .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Invariant)?;
    let service_account_key =
        OrganizationServiceAccountKey::from_persistence_key(&model.service_account_key)
            .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Invariant)?;
    let credential_key = OrganizationServiceAccountKey::from_persistence_key(&model.credential_key)
        .map_err(|_| OrganizationServiceAccountAuditRepositoryError::Invariant)?;
    if model.id <= 0 || model.created_at.unix_timestamp() < 0 {
        return Err(OrganizationServiceAccountAuditRepositoryError::Invariant);
    }
    Ok(OrganizationServiceAccountAuditRecord {
        id: model.id,
        event_id,
        organization_id,
        service_account_id,
        service_account_key,
        credential_key,
        created_at: model.created_at.unix_timestamp(),
    })
}

fn is_unique_violation(error: &sea_orm::DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}
