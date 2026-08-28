use std::{fmt, time::Duration};

use af_domain::{
    OrganizationId, OrganizationServiceAccount, OrganizationServiceAccountId,
    OrganizationServiceAccountKey, OrganizationServiceAccountLifecycleDisposition,
    OrganizationServiceAccountLocator, OrganizationServiceAccountName,
    OrganizationServiceAccountSecret, OrganizationServiceAccountSecretDigest,
    OrganizationServiceAccountStatus, classify_organization_service_account_lifecycle,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    DatabasePool,
    entity::{
        SensitiveString, organization_service_account_keys, organization_service_accounts,
        organizations,
    },
};

const ACTIVE_ACCOUNT: i16 = OrganizationServiceAccountStatus::Active.code();
const ACTIVE_KEY: i16 = 1;
const REVOKED_KEY: i16 = 2;

/// 企业服务账号目录单页最大记录数。
pub const MAX_ORGANIZATION_SERVICE_ACCOUNT_PAGE_SIZE: usize = 100;

/// 已持久化的服务账号主体及其版本事实。
#[derive(Clone, PartialEq)]
pub struct OrganizationServiceAccountRecord {
    account: OrganizationServiceAccount,
    version: i64,
    created_at: i64,
    updated_at: i64,
    revoked_at: Option<i64>,
}

/// 企业范围服务账号稳定游标分页结果。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationServiceAccountPageRecord {
    items: Vec<OrganizationServiceAccountRecord>,
    next_cursor: Option<OrganizationServiceAccountKey>,
}

impl OrganizationServiceAccountPageRecord {
    #[must_use]
    pub fn items(&self) -> &[OrganizationServiceAccountRecord] {
        &self.items
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<OrganizationServiceAccountKey> {
        self.next_cursor
    }
}

impl OrganizationServiceAccountRecord {
    #[must_use]
    pub const fn account(&self) -> &OrganizationServiceAccount {
        &self.account
    }
    #[must_use]
    pub const fn locator(&self) -> OrganizationServiceAccountLocator {
        self.account.locator()
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
    #[must_use]
    pub const fn revoked_at(&self) -> Option<i64> {
        self.revoked_at
    }
}

impl fmt::Debug for OrganizationServiceAccountRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationServiceAccountRecord")
            .field("locator", &self.locator())
            .field("status", &self.account.status())
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 已持久化的密钥轮换事实；不包含明文密钥。
#[derive(Clone, PartialEq)]
pub struct OrganizationServiceAccountKeyRecord {
    locator: OrganizationServiceAccountLocator,
    key_key: OrganizationServiceAccountKey,
    key_prefix: String,
    status: i16,
    version: i64,
    created_at: i64,
    revoked_at: Option<i64>,
}

impl OrganizationServiceAccountKeyRecord {
    #[must_use]
    pub const fn locator(&self) -> OrganizationServiceAccountLocator {
        self.locator
    }
    #[must_use]
    pub const fn key_key(&self) -> OrganizationServiceAccountKey {
        self.key_key
    }
    #[must_use]
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }
    #[must_use]
    pub const fn status(&self) -> i16 {
        self.status
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    #[must_use]
    pub const fn revoked_at(&self) -> Option<i64> {
        self.revoked_at
    }
}

impl fmt::Debug for OrganizationServiceAccountKeyRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationServiceAccountKeyRecord")
            .field("locator", &self.locator)
            .field("key_key", &self.key_key)
            .field("status", &self.status)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 创建服务账号时一次性返回的主体和密钥；密钥明文只能从此结果消费一次。
pub struct OrganizationServiceAccountIssued {
    account: OrganizationServiceAccountRecord,
    key: OrganizationServiceAccountKeyRecord,
    secret: OrganizationServiceAccountSecret,
}

impl OrganizationServiceAccountIssued {
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        OrganizationServiceAccountRecord,
        OrganizationServiceAccountKeyRecord,
        OrganizationServiceAccountSecret,
    ) {
        (self.account, self.key, self.secret)
    }
}

impl fmt::Debug for OrganizationServiceAccountIssued {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationServiceAccountIssued(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationServiceAccountRepositoryError {
    #[error("企业服务账号输入无效")]
    InvalidInput,
    #[error("企业服务账号不存在")]
    NotFound,
    #[error("企业服务账号企业范围不可用")]
    Restricted,
    #[error("企业服务账号版本冲突")]
    Conflict,
    #[error("企业服务账号数据库操作失败")]
    Query,
    #[error("企业服务账号操作超时")]
    Timeout,
    #[error("企业服务账号持久化状态损坏")]
    Invariant,
}

#[derive(Clone)]
pub struct OrganizationServiceAccountRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationServiceAccountRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationServiceAccountRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationServiceAccountRepositoryError::InvalidInput);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 在活动企业内创建主体和首个密钥摘要。
    pub async fn create(
        &self,
        organization_id: OrganizationId,
        name: OrganizationServiceAccountName,
    ) -> Result<OrganizationServiceAccountIssued, OrganizationServiceAccountRepositoryError> {
        timeout(
            self.operation_timeout,
            self.create_inner(organization_id, name),
        )
        .await
        .map_err(|_| OrganizationServiceAccountRepositoryError::Timeout)?
    }

    pub async fn get(
        &self,
        locator: OrganizationServiceAccountLocator,
    ) -> Result<OrganizationServiceAccountRecord, OrganizationServiceAccountRepositoryError> {
        timeout(self.operation_timeout, self.get_inner(locator))
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Timeout)?
    }

    /// 按企业和随机公开标识读取主体，避免把数据库自增 ID 暴露到管理接口。
    pub async fn get_by_public_key(
        &self,
        organization_id: OrganizationId,
        public_key: OrganizationServiceAccountKey,
    ) -> Result<OrganizationServiceAccountRecord, OrganizationServiceAccountRepositoryError> {
        timeout(self.operation_timeout, async {
            let model = organization_service_accounts::Entity::find()
                .filter(
                    organization_service_accounts::Column::OrganizationId.eq(organization_id.get()),
                )
                .filter(
                    organization_service_accounts::Column::PublicKey
                        .eq(public_key.persistence_key()),
                )
                .one(self.pool.connection())
                .await
                .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?
                .ok_or(OrganizationServiceAccountRepositoryError::NotFound)?;
            account_record_from_model(model)
                .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)
        })
        .await
        .map_err(|_| OrganizationServiceAccountRepositoryError::Timeout)?
    }

    /// 按内部 ID 稳定排序、以随机公开标识承载游标列出同一企业的服务账号主体。
    pub async fn list(
        &self,
        organization_id: OrganizationId,
        after: Option<OrganizationServiceAccountKey>,
        limit: usize,
    ) -> Result<OrganizationServiceAccountPageRecord, OrganizationServiceAccountRepositoryError>
    {
        if limit == 0 || limit > MAX_ORGANIZATION_SERVICE_ACCOUNT_PAGE_SIZE {
            return Err(OrganizationServiceAccountRepositoryError::InvalidInput);
        }
        let fetch_limit = u64::try_from(limit)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(OrganizationServiceAccountRepositoryError::InvalidInput)?;
        timeout(self.operation_timeout, async {
            let after_id = if let Some(after) = after {
                let model = organization_service_accounts::Entity::find()
                    .filter(
                        organization_service_accounts::Column::OrganizationId
                            .eq(organization_id.get()),
                    )
                    .filter(
                        organization_service_accounts::Column::PublicKey
                            .eq(after.persistence_key()),
                    )
                    .one(self.pool.connection())
                    .await
                    .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?
                    .ok_or(OrganizationServiceAccountRepositoryError::NotFound)?;
                Some(
                    OrganizationServiceAccountId::new(model.id)
                        .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?,
                )
            } else {
                None
            };
            let mut query = organization_service_accounts::Entity::find()
                .filter(
                    organization_service_accounts::Column::OrganizationId.eq(organization_id.get()),
                )
                .order_by_asc(organization_service_accounts::Column::Id)
                .limit(fetch_limit);
            if let Some(after_id) = after_id {
                query = query.filter(organization_service_accounts::Column::Id.gt(after_id.get()));
            }
            let mut models = query
                .all(self.pool.connection())
                .await
                .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
            let has_more = models.len() > limit;
            models.truncate(limit);
            let next_cursor = if has_more {
                models
                    .last()
                    .map(|model| {
                        OrganizationServiceAccountKey::from_persistence_key(&model.public_key)
                    })
                    .transpose()
                    .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?
            } else {
                None
            };
            let items = models
                .into_iter()
                .map(account_record_from_model)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?;
            Ok(OrganizationServiceAccountPageRecord { items, next_cursor })
        })
        .await
        .map_err(|_| OrganizationServiceAccountRepositoryError::Timeout)?
    }

    /// 使用企业与当前版本原子变更主体生命周期。
    pub async fn set_status(
        &self,
        locator: OrganizationServiceAccountLocator,
        expected_version: i64,
        target: OrganizationServiceAccountStatus,
    ) -> Result<OrganizationServiceAccountRecord, OrganizationServiceAccountRepositoryError> {
        if expected_version <= 0 {
            return Err(OrganizationServiceAccountRepositoryError::InvalidInput);
        }
        timeout(
            self.operation_timeout,
            self.set_status_inner(locator, expected_version, target),
        )
        .await
        .map_err(|_| OrganizationServiceAccountRepositoryError::Timeout)?
    }

    /// 在活动主体内原子撤销旧密钥并签发新密钥，主体版本同步递增。
    pub async fn rotate_key(
        &self,
        locator: OrganizationServiceAccountLocator,
        expected_version: i64,
    ) -> Result<OrganizationServiceAccountIssued, OrganizationServiceAccountRepositoryError> {
        if expected_version <= 0 {
            return Err(OrganizationServiceAccountRepositoryError::InvalidInput);
        }
        timeout(
            self.operation_timeout,
            self.rotate_key_inner(locator, expected_version),
        )
        .await
        .map_err(|_| OrganizationServiceAccountRepositoryError::Timeout)?
    }

    /// 仅接受同企业、活动主体和活动密钥的摘要认证。
    pub async fn authenticate_secret(
        &self,
        locator: OrganizationServiceAccountLocator,
        secret: OrganizationServiceAccountSecret,
    ) -> Result<OrganizationServiceAccountKeyRecord, OrganizationServiceAccountRepositoryError>
    {
        timeout(
            self.operation_timeout,
            self.authenticate_secret_inner(locator, secret.digest()),
        )
        .await
        .map_err(|_| OrganizationServiceAccountRepositoryError::Timeout)?
    }

    async fn create_inner(
        &self,
        organization_id: OrganizationId,
        name: OrganizationServiceAccountName,
    ) -> Result<OrganizationServiceAccountIssued, OrganizationServiceAccountRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let organization = organizations::Entity::find_by_id(organization_id.get())
            .one(&transaction)
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?
            .ok_or(OrganizationServiceAccountRepositoryError::Restricted)?;
        if organization.status != 1 || organization.closed_at.is_some() {
            return Err(OrganizationServiceAccountRepositoryError::Restricted);
        }
        let account_key = OrganizationServiceAccountKey::generate()
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let secret = OrganizationServiceAccountSecret::generate()
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let account = organization_service_accounts::ActiveModel {
            organization_id: Set(organization_id.get()),
            public_key: Set(account_key.persistence_key()),
            name: Set(name.as_str().to_owned()),
            status: Set(ACTIVE_ACCOUNT),
            version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            revoked_at: Set(None),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        OrganizationServiceAccountId::new(account.id)
            .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?;
        let key_key = OrganizationServiceAccountKey::generate()
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let key = organization_service_account_keys::ActiveModel {
            organization_id: Set(organization_id.get()),
            service_account_id: Set(account.id),
            key_key: Set(key_key.persistence_key()),
            key_prefix: Set(secret.prefix().to_owned()),
            key_digest: Set(SensitiveString::from(secret.digest().as_str())),
            status: Set(ACTIVE_KEY),
            version: Set(1),
            created_at: Set(now),
            revoked_at: Set(None),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let account = account_record_from_model(account)
            .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?;
        let key = key_record_from_model(key)
            .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?;
        Ok(OrganizationServiceAccountIssued {
            account,
            key,
            secret,
        })
    }

    async fn get_inner(
        &self,
        locator: OrganizationServiceAccountLocator,
    ) -> Result<OrganizationServiceAccountRecord, OrganizationServiceAccountRepositoryError> {
        let model =
            organization_service_accounts::Entity::find_by_id(locator.service_account_id().get())
                .filter(
                    organization_service_accounts::Column::OrganizationId
                        .eq(locator.organization_id().get()),
                )
                .one(self.pool.connection())
                .await
                .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?
                .ok_or(OrganizationServiceAccountRepositoryError::NotFound)?;
        account_record_from_model(model)
            .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)
    }

    async fn set_status_inner(
        &self,
        locator: OrganizationServiceAccountLocator,
        expected_version: i64,
        target: OrganizationServiceAccountStatus,
    ) -> Result<OrganizationServiceAccountRecord, OrganizationServiceAccountRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let current = lock_account(&transaction, locator).await?;
        let current_record = account_record_from_model(current.clone())
            .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?;
        match classify_organization_service_account_lifecycle(
            current_record.account.status(),
            target,
        ) {
            OrganizationServiceAccountLifecycleDisposition::Reject(_) => {
                return Err(OrganizationServiceAccountRepositoryError::Restricted);
            }
            OrganizationServiceAccountLifecycleDisposition::Noop => {
                if current_record.version() != expected_version {
                    return Err(OrganizationServiceAccountRepositoryError::Conflict);
                }
                transaction
                    .commit()
                    .await
                    .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
                return Ok(current_record);
            }
            OrganizationServiceAccountLifecycleDisposition::Suspend
            | OrganizationServiceAccountLifecycleDisposition::Resume
            | OrganizationServiceAccountLifecycleDisposition::Revoke => {}
        }
        let next_version = expected_version
            .checked_add(1)
            .ok_or(OrganizationServiceAccountRepositoryError::InvalidInput)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = organization_service_accounts::Entity::update_many()
            .filter(
                organization_service_accounts::Column::Id.eq(locator.service_account_id().get()),
            )
            .filter(
                organization_service_accounts::Column::OrganizationId
                    .eq(locator.organization_id().get()),
            )
            .filter(organization_service_accounts::Column::Version.eq(expected_version))
            .col_expr(
                organization_service_accounts::Column::Status,
                Expr::value(target.code()),
            )
            .col_expr(
                organization_service_accounts::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_service_accounts::Column::UpdatedAt,
                Expr::value(now),
            )
            .col_expr(
                organization_service_accounts::Column::RevokedAt,
                Expr::value(if target == OrganizationServiceAccountStatus::Revoked {
                    Some(now)
                } else {
                    None
                }),
            )
            .exec(&transaction)
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        if result.rows_affected != 1 {
            return Err(OrganizationServiceAccountRepositoryError::Conflict);
        }
        let updated =
            organization_service_accounts::Entity::find_by_id(locator.service_account_id().get())
                .one(&transaction)
                .await
                .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?
                .ok_or(OrganizationServiceAccountRepositoryError::Invariant)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        account_record_from_model(updated)
            .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)
    }

    async fn rotate_key_inner(
        &self,
        locator: OrganizationServiceAccountLocator,
        expected_version: i64,
    ) -> Result<OrganizationServiceAccountIssued, OrganizationServiceAccountRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let current = lock_account(&transaction, locator).await?;
        let current_record = account_record_from_model(current.clone())
            .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?;
        if current_record.version() != expected_version {
            return Err(OrganizationServiceAccountRepositoryError::Conflict);
        }
        if !current_record.account.status().is_usable() {
            return Err(OrganizationServiceAccountRepositoryError::Restricted);
        }
        let next_version = expected_version
            .checked_add(1)
            .ok_or(OrganizationServiceAccountRepositoryError::InvalidInput)?;
        let secret = OrganizationServiceAccountSecret::generate()
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let key_key = OrganizationServiceAccountKey::generate()
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        organization_service_account_keys::Entity::update_many()
            .filter(
                organization_service_account_keys::Column::OrganizationId
                    .eq(locator.organization_id().get()),
            )
            .filter(
                organization_service_account_keys::Column::ServiceAccountId
                    .eq(locator.service_account_id().get()),
            )
            .filter(organization_service_account_keys::Column::Status.eq(ACTIVE_KEY))
            .col_expr(
                organization_service_account_keys::Column::Status,
                Expr::value(REVOKED_KEY),
            )
            .col_expr(
                organization_service_account_keys::Column::RevokedAt,
                Expr::value(Some(now)),
            )
            .exec(&transaction)
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let key = organization_service_account_keys::ActiveModel {
            organization_id: Set(locator.organization_id().get()),
            service_account_id: Set(locator.service_account_id().get()),
            key_key: Set(key_key.persistence_key()),
            key_prefix: Set(secret.prefix().to_owned()),
            key_digest: Set(SensitiveString::from(secret.digest().as_str())),
            status: Set(ACTIVE_KEY),
            version: Set(1),
            created_at: Set(now),
            revoked_at: Set(None),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        let result = organization_service_accounts::Entity::update_many()
            .filter(
                organization_service_accounts::Column::Id.eq(locator.service_account_id().get()),
            )
            .filter(
                organization_service_accounts::Column::OrganizationId
                    .eq(locator.organization_id().get()),
            )
            .filter(organization_service_accounts::Column::Version.eq(expected_version))
            .col_expr(
                organization_service_accounts::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_service_accounts::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        if result.rows_affected != 1 {
            return Err(OrganizationServiceAccountRepositoryError::Conflict);
        }
        let account =
            organization_service_accounts::Entity::find_by_id(locator.service_account_id().get())
                .one(&transaction)
                .await
                .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?
                .ok_or(OrganizationServiceAccountRepositoryError::Invariant)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?;
        Ok(OrganizationServiceAccountIssued {
            account: account_record_from_model(account)
                .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?,
            key: key_record_from_model(key)
                .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?,
            secret,
        })
    }

    async fn authenticate_secret_inner(
        &self,
        locator: OrganizationServiceAccountLocator,
        digest: OrganizationServiceAccountSecretDigest,
    ) -> Result<OrganizationServiceAccountKeyRecord, OrganizationServiceAccountRepositoryError>
    {
        let account =
            organization_service_accounts::Entity::find_by_id(locator.service_account_id().get())
                .filter(
                    organization_service_accounts::Column::OrganizationId
                        .eq(locator.organization_id().get()),
                )
                .one(self.pool.connection())
                .await
                .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?
                .ok_or(OrganizationServiceAccountRepositoryError::NotFound)?;
        let account = account_record_from_model(account)
            .map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)?;
        if !account.account.status().is_usable() {
            return Err(OrganizationServiceAccountRepositoryError::Restricted);
        }
        let key = organization_service_account_keys::Entity::find()
            .filter(
                organization_service_account_keys::Column::OrganizationId
                    .eq(locator.organization_id().get()),
            )
            .filter(
                organization_service_account_keys::Column::ServiceAccountId
                    .eq(locator.service_account_id().get()),
            )
            .filter(
                organization_service_account_keys::Column::KeyDigest
                    .eq(SensitiveString::from(digest.as_str())),
            )
            .filter(organization_service_account_keys::Column::Status.eq(ACTIVE_KEY))
            .one(self.pool.connection())
            .await
            .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?
            .ok_or(OrganizationServiceAccountRepositoryError::NotFound)?;
        key_record_from_model(key).map_err(|_| OrganizationServiceAccountRepositoryError::Invariant)
    }
}

async fn lock_account(
    transaction: &DatabaseTransaction,
    locator: OrganizationServiceAccountLocator,
) -> Result<organization_service_accounts::Model, OrganizationServiceAccountRepositoryError> {
    let mut query =
        organization_service_accounts::Entity::find_by_id(locator.service_account_id().get())
            .filter(
                organization_service_accounts::Column::OrganizationId
                    .eq(locator.organization_id().get()),
            );
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationServiceAccountRepositoryError::Query)?
        .ok_or(OrganizationServiceAccountRepositoryError::NotFound)
}

fn account_record_from_model(
    model: organization_service_accounts::Model,
) -> Result<OrganizationServiceAccountRecord, ()> {
    let organization_id = OrganizationId::new(model.organization_id).map_err(|_| ())?;
    let service_account_id = OrganizationServiceAccountId::new(model.id).map_err(|_| ())?;
    let public_key =
        OrganizationServiceAccountKey::from_persistence_key(&model.public_key).map_err(|_| ())?;
    let name = OrganizationServiceAccountName::new(model.name).map_err(|_| ())?;
    let status = OrganizationServiceAccountStatus::from_code(model.status).map_err(|_| ())?;
    if model.version <= 0
        || (status == OrganizationServiceAccountStatus::Revoked) != model.revoked_at.is_some()
    {
        return Err(());
    }
    let account = OrganizationServiceAccount::from_persistence(
        OrganizationServiceAccountLocator::new(organization_id, service_account_id),
        public_key,
        name,
        status,
    );
    Ok(OrganizationServiceAccountRecord {
        account,
        version: model.version,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
        revoked_at: model.revoked_at.map(|value| value.unix_timestamp()),
    })
}

fn key_record_from_model(
    model: organization_service_account_keys::Model,
) -> Result<OrganizationServiceAccountKeyRecord, ()> {
    let organization_id = OrganizationId::new(model.organization_id).map_err(|_| ())?;
    let service_account_id =
        OrganizationServiceAccountId::new(model.service_account_id).map_err(|_| ())?;
    let key_key =
        OrganizationServiceAccountKey::from_persistence_key(&model.key_key).map_err(|_| ())?;
    if model.key_prefix != "sa_"
        || model.version <= 0
        || !matches!(model.status, ACTIVE_KEY | REVOKED_KEY)
    {
        return Err(());
    }
    if (model.status == REVOKED_KEY) != model.revoked_at.is_some() {
        return Err(());
    }
    Ok(OrganizationServiceAccountKeyRecord {
        locator: OrganizationServiceAccountLocator::new(organization_id, service_account_id),
        key_key,
        key_prefix: model.key_prefix,
        status: model.status,
        version: model.version,
        created_at: model.created_at.unix_timestamp(),
        revoked_at: model.revoked_at.map(|value| value.unix_timestamp()),
    })
}
