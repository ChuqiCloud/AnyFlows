use std::{fmt, time::Duration};

use af_domain::{
    OrganizationEntitlementStatus, OrganizationId, OrganizationMembershipId,
    OrganizationMembershipStatus, OrganizationStatus, OrganizationTeamId, OrganizationTeamStatus,
    TokenId, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    PaginatorTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, LockType, Order},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminTokenRecord, DatabasePool,
    admin_token::base_query,
    entity::{
        TokenHash, TokenIpAllowlist, TokenModelAllowlist, groups, organization_entitlements,
        organization_memberships, organization_teams, organizations, tokens, users,
    },
};

pub const MAX_ORGANIZATION_TOKEN_PAGE_SIZE: usize = 100;

/// 企业 Key 的完整持久化写入字段；不包含明文 Key。
pub struct OrganizationTokenWriteRecord {
    pub(crate) name: String,
    pub(crate) status: i16,
    pub(crate) remain_quota: i64,
    pub(crate) unlimited_quota: bool,
    pub(crate) expired_at: Option<i64>,
    pub(crate) model_limits: Option<Vec<String>>,
    pub(crate) allow_ips: Option<Vec<String>>,
    pub(crate) rate_limit_5h: Option<i64>,
    pub(crate) rate_limit_1d: Option<i64>,
    pub(crate) rate_limit_7d: Option<i64>,
    pub(crate) max_requests: Option<i64>,
}

impl OrganizationTokenWriteRecord {
    #[allow(clippy::too_many_arguments, reason = "字段与企业 Key 写入契约一一对应")]
    #[must_use]
    pub fn new(
        name: String,
        status: i16,
        remain_quota: i64,
        unlimited_quota: bool,
        expired_at: Option<i64>,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
        rate_limit_5h: Option<i64>,
        rate_limit_1d: Option<i64>,
        rate_limit_7d: Option<i64>,
        max_requests: Option<i64>,
    ) -> Self {
        Self {
            name,
            status,
            remain_quota,
            unlimited_quota,
            expired_at,
            model_limits,
            allow_ips,
            rate_limit_5h,
            rate_limit_1d,
            rate_limit_7d,
            max_requests,
        }
    }
}

impl fmt::Debug for OrganizationTokenWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationTokenWriteRecord(<redacted>)")
    }
}

/// 企业 Key 签发时已经派生的摘要和组织主体。
pub struct OrganizationTokenCreateRecord {
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    owner_user_id: UserId,
    key_hash: String,
    key_prefix: String,
    fields: OrganizationTokenWriteRecord,
}

impl OrganizationTokenCreateRecord {
    #[must_use]
    pub fn new(
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        owner_user_id: UserId,
        key_hash: String,
        key_prefix: String,
        fields: OrganizationTokenWriteRecord,
    ) -> Self {
        Self {
            organization_id,
            membership_id,
            owner_user_id,
            key_hash,
            key_prefix,
            fields,
        }
    }
}

impl fmt::Debug for OrganizationTokenCreateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationTokenCreateRecord(<redacted>)")
    }
}

/// 企业脱敏目录中的单个 Key 快照。
pub struct OrganizationTokenRecord {
    token: AdminTokenRecord,
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    team_id: Option<OrganizationTeamId>,
}

impl OrganizationTokenRecord {
    #[must_use]
    pub const fn token(&self) -> &AdminTokenRecord {
        &self.token
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    #[must_use]
    pub const fn membership_id(&self) -> OrganizationMembershipId {
        self.membership_id
    }

    #[must_use]
    pub const fn team_id(&self) -> Option<OrganizationTeamId> {
        self.team_id
    }
}

impl fmt::Debug for OrganizationTokenRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationTokenRecord(<redacted>)")
    }
}

pub struct OrganizationTokenPageRecord {
    tokens: Vec<OrganizationTokenRecord>,
    next_cursor: Option<TokenId>,
}

impl OrganizationTokenPageRecord {
    #[must_use]
    pub fn into_parts(self) -> (Vec<OrganizationTokenRecord>, Option<TokenId>) {
        (self.tokens, self.next_cursor)
    }
}

pub enum OrganizationTokenLookupOutcome {
    Found(Box<OrganizationTokenRecord>),
    NotFound,
}

pub enum OrganizationTokenMutationOutcome {
    Mutated(Box<OrganizationTokenRecord>),
    NotFound,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationTokenRepositoryConfigError {
    #[error("企业 API Key 操作超时必须大于零")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationTokenRepositoryError {
    #[error("企业 API Key 输入无效")]
    InvalidInput,
    #[error("企业 API Key 不存在")]
    NotFound,
    #[error("企业 API Key 授权不足")]
    Restricted,
    #[error("企业 API Key 数量已达上限")]
    LimitReached,
    #[error("企业 API Key 数据库操作失败")]
    Query,
    #[error("企业 API Key 操作结果未知")]
    OutcomeUnknown,
    #[error("企业 API Key 操作超时")]
    Timeout,
    #[error("企业 API Key 持久化状态损坏")]
    Invariant,
}

#[derive(Clone)]
pub struct OrganizationTokenRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationTokenRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationTokenRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationTokenRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    pub async fn list(
        &self,
        organization_id: OrganizationId,
        owner_user_id: Option<UserId>,
        after: Option<TokenId>,
        limit: usize,
    ) -> Result<OrganizationTokenPageRecord, OrganizationTokenRepositoryError> {
        if !(1..=MAX_ORGANIZATION_TOKEN_PAGE_SIZE).contains(&limit) {
            return Err(OrganizationTokenRepositoryError::InvalidInput);
        }
        timeout(
            self.operation_timeout,
            self.list_inner(organization_id, owner_user_id, after, limit),
        )
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Timeout)?
    }

    pub async fn get(
        &self,
        organization_id: OrganizationId,
        owner_user_id: Option<UserId>,
        token_id: TokenId,
    ) -> Result<OrganizationTokenLookupOutcome, OrganizationTokenRepositoryError> {
        timeout(
            self.operation_timeout,
            self.get_inner(organization_id, owner_user_id, token_id),
        )
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Timeout)?
    }

    pub async fn create(
        &self,
        record: OrganizationTokenCreateRecord,
    ) -> Result<OrganizationTokenRecord, OrganizationTokenRepositoryError> {
        timeout(self.operation_timeout, self.create_inner(record))
            .await
            .map_err(|_| OrganizationTokenRepositoryError::OutcomeUnknown)?
    }

    pub async fn update_self(
        &self,
        organization_id: OrganizationId,
        owner_user_id: UserId,
        token_id: TokenId,
        fields: OrganizationTokenWriteRecord,
    ) -> Result<OrganizationTokenMutationOutcome, OrganizationTokenRepositoryError> {
        timeout(
            self.operation_timeout,
            self.update_self_inner(organization_id, owner_user_id, token_id, fields),
        )
        .await
        .map_err(|_| OrganizationTokenRepositoryError::OutcomeUnknown)?
    }

    pub async fn disable(
        &self,
        organization_id: OrganizationId,
        token_id: TokenId,
        owner_user_id: Option<UserId>,
    ) -> Result<OrganizationTokenMutationOutcome, OrganizationTokenRepositoryError> {
        timeout(
            self.operation_timeout,
            self.disable_inner(organization_id, token_id, owner_user_id),
        )
        .await
        .map_err(|_| OrganizationTokenRepositoryError::OutcomeUnknown)?
    }

    async fn list_inner(
        &self,
        organization_id: OrganizationId,
        owner_user_id: Option<UserId>,
        after: Option<TokenId>,
        limit: usize,
    ) -> Result<OrganizationTokenPageRecord, OrganizationTokenRepositoryError> {
        let mut query = organization_query(self.pool.connection().get_database_backend());
        query
            .and_where(tokens::Column::OrganizationId.eq(organization_id.get()))
            .and_where(tokens::Column::DeletedAt.is_null())
            .order_by(tokens::Column::Id, Order::Asc)
            .limit((limit + 1) as u64);
        if let Some(owner_user_id) = owner_user_id {
            query.and_where(tokens::Column::UserId.eq(owner_user_id.get()));
        }
        if let Some(after) = after {
            query.and_where(tokens::Column::Id.gt(after.get()));
        }
        let statement = self.pool.connection().get_database_backend().build(&query);
        let mut rows = self
            .pool
            .connection()
            .query_all(statement)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| OrganizationTokenRepositoryError::Query)?;
        let has_more = rows.len() > limit;
        if has_more {
            rows.truncate(limit);
        }
        let tokens = rows
            .iter()
            .map(record_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| tokens.last().map(|record| record.token().token_id()))
            .flatten();
        Ok(OrganizationTokenPageRecord {
            tokens,
            next_cursor,
        })
    }

    async fn get_inner(
        &self,
        organization_id: OrganizationId,
        owner_user_id: Option<UserId>,
        token_id: TokenId,
    ) -> Result<OrganizationTokenLookupOutcome, OrganizationTokenRepositoryError> {
        let mut query = organization_query(self.pool.connection().get_database_backend());
        query
            .and_where(tokens::Column::OrganizationId.eq(organization_id.get()))
            .and_where(tokens::Column::Id.eq(token_id.get()))
            .and_where(tokens::Column::DeletedAt.is_null())
            .limit(2);
        if let Some(owner_user_id) = owner_user_id {
            query.and_where(tokens::Column::UserId.eq(owner_user_id.get()));
        }
        let rows = self
            .pool
            .connection()
            .query_all(self.pool.connection().get_database_backend().build(&query))
            .await
            .map_err(|_| OrganizationTokenRepositoryError::Query)?;
        match rows.as_slice() {
            [] => Ok(OrganizationTokenLookupOutcome::NotFound),
            [row] => Ok(OrganizationTokenLookupOutcome::Found(Box::new(
                record_from_row(row)?,
            ))),
            _ => Err(OrganizationTokenRepositoryError::Invariant),
        }
    }

    async fn create_inner(
        &self,
        record: OrganizationTokenCreateRecord,
    ) -> Result<OrganizationTokenRecord, OrganizationTokenRepositoryError> {
        let fields = StoredFields::try_from(record.fields)?;
        let key_hash = TokenHash::parse(&record.key_hash)
            .map_err(|_| OrganizationTokenRepositoryError::InvalidInput)?;
        if !valid_key_prefix(&record.key_prefix) {
            return Err(OrganizationTokenRepositoryError::InvalidInput);
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationTokenRepositoryError::Query)?;
        let provision = validate_provisioning(
            &transaction,
            record.organization_id,
            record.membership_id,
            record.owner_user_id,
        )
        .await?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let inserted = tokens::ActiveModel {
            user_id: Set(record.owner_user_id.get()),
            key_hash: Set(key_hash),
            key_prefix: Set(record.key_prefix),
            name: Set(fields.name),
            status: Set(fields.status),
            group_id: Set(None),
            organization_id: Set(Some(record.organization_id.get())),
            organization_membership_id: Set(Some(record.membership_id.get())),
            organization_team_id: Set(provision.team_id.map(OrganizationTeamId::get)),
            remain_quota: Set(fields.remain_quota),
            unlimited_quota: Set(fields.unlimited_quota),
            expired_at: Set(fields.expired_at),
            model_limits: Set(fields.model_limits),
            allow_ips: Set(fields.allow_ips),
            cross_group_retry: Set(false),
            rate_limit_5h: Set(fields.rate_limit_5h),
            rate_limit_1d: Set(fields.rate_limit_1d),
            rate_limit_7d: Set(fields.rate_limit_7d),
            window_5h_start: Set(now),
            window_1d_start: Set(now),
            window_7d_start: Set(now),
            max_requests: Set(fields.max_requests),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?;
        let token_id =
            TokenId::new(inserted.id).map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
        let snapshot = fetch_snapshot(&transaction, record.organization_id, token_id).await?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationTokenRepositoryError::OutcomeUnknown)?;
        Ok(snapshot)
    }

    async fn update_self_inner(
        &self,
        organization_id: OrganizationId,
        owner_user_id: UserId,
        token_id: TokenId,
        fields: OrganizationTokenWriteRecord,
    ) -> Result<OrganizationTokenMutationOutcome, OrganizationTokenRepositoryError> {
        let fields = StoredFields::try_from(fields)?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationTokenRepositoryError::Query)?;
        validate_existing_member(&transaction, organization_id, owner_user_id).await?;
        if fields.status == 1 {
            ensure_entitlement_allows_provisioning(&transaction, organization_id).await?;
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = tokens::Entity::update_many()
            .filter(tokens::Column::Id.eq(token_id.get()))
            .filter(tokens::Column::OrganizationId.eq(organization_id.get()))
            .filter(tokens::Column::UserId.eq(owner_user_id.get()))
            .filter(tokens::Column::DeletedAt.is_null())
            .col_expr(tokens::Column::Name, Expr::value(fields.name))
            .col_expr(tokens::Column::Status, Expr::value(fields.status))
            .col_expr(
                tokens::Column::RemainQuota,
                Expr::value(fields.remain_quota),
            )
            .col_expr(
                tokens::Column::UnlimitedQuota,
                Expr::value(fields.unlimited_quota),
            )
            .col_expr(tokens::Column::ExpiredAt, Expr::value(fields.expired_at))
            .col_expr(
                tokens::Column::ModelLimits,
                Expr::value(fields.model_limits),
            )
            .col_expr(tokens::Column::AllowIps, Expr::value(fields.allow_ips))
            .col_expr(
                tokens::Column::RateLimit5h,
                Expr::value(fields.rate_limit_5h),
            )
            .col_expr(
                tokens::Column::RateLimit1d,
                Expr::value(fields.rate_limit_1d),
            )
            .col_expr(
                tokens::Column::RateLimit7d,
                Expr::value(fields.rate_limit_7d),
            )
            .col_expr(
                tokens::Column::MaxRequests,
                Expr::value(fields.max_requests),
            )
            .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .await
            .map_err(|_| OrganizationTokenRepositoryError::Query)?;
        if result.rows_affected == 0 {
            let _ = transaction.rollback().await;
            return Ok(OrganizationTokenMutationOutcome::NotFound);
        }
        if result.rows_affected != 1 {
            let _ = transaction.rollback().await;
            return Err(OrganizationTokenRepositoryError::Invariant);
        }
        let snapshot = fetch_snapshot(&transaction, organization_id, token_id).await?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationTokenRepositoryError::OutcomeUnknown)?;
        Ok(OrganizationTokenMutationOutcome::Mutated(Box::new(
            snapshot,
        )))
    }

    async fn disable_inner(
        &self,
        organization_id: OrganizationId,
        token_id: TokenId,
        owner_user_id: Option<UserId>,
    ) -> Result<OrganizationTokenMutationOutcome, OrganizationTokenRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationTokenRepositoryError::Query)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let mut update = tokens::Entity::update_many()
            .filter(tokens::Column::Id.eq(token_id.get()))
            .filter(tokens::Column::OrganizationId.eq(organization_id.get()))
            .filter(tokens::Column::DeletedAt.is_null())
            .col_expr(tokens::Column::Status, Expr::value(2_i16))
            .col_expr(tokens::Column::UpdatedAt, Expr::value(now));
        if let Some(owner_user_id) = owner_user_id {
            update = update.filter(tokens::Column::UserId.eq(owner_user_id.get()));
        }
        let result = update
            .exec(&transaction)
            .await
            .map_err(|_| OrganizationTokenRepositoryError::Query)?;
        if result.rows_affected == 0 {
            let _ = transaction.rollback().await;
            return Ok(OrganizationTokenMutationOutcome::NotFound);
        }
        if result.rows_affected != 1 {
            let _ = transaction.rollback().await;
            return Err(OrganizationTokenRepositoryError::Invariant);
        }
        let snapshot = fetch_snapshot(&transaction, organization_id, token_id).await?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationTokenRepositoryError::OutcomeUnknown)?;
        Ok(OrganizationTokenMutationOutcome::Mutated(Box::new(
            snapshot,
        )))
    }
}

impl fmt::Debug for OrganizationTokenRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationTokenRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

struct StoredFields {
    name: String,
    status: i16,
    remain_quota: i64,
    unlimited_quota: bool,
    expired_at: Option<TimeDateTimeWithTimeZone>,
    model_limits: Option<TokenModelAllowlist>,
    allow_ips: Option<TokenIpAllowlist>,
    rate_limit_5h: Option<i64>,
    rate_limit_1d: Option<i64>,
    rate_limit_7d: Option<i64>,
    max_requests: Option<i64>,
}

impl TryFrom<OrganizationTokenWriteRecord> for StoredFields {
    type Error = OrganizationTokenRepositoryError;

    fn try_from(value: OrganizationTokenWriteRecord) -> Result<Self, Self::Error> {
        if !valid_text(&value.name, 128)
            || !matches!(value.status, 1 | 2)
            || value.remain_quota < 0
            || value.expired_at.is_some_and(|value| value < 0)
            || [
                value.rate_limit_5h,
                value.rate_limit_1d,
                value.rate_limit_7d,
                value.max_requests,
            ]
            .into_iter()
            .flatten()
            .any(|value| value < 0)
        {
            return Err(OrganizationTokenRepositoryError::InvalidInput);
        }
        Ok(Self {
            name: value.name,
            status: value.status,
            remain_quota: value.remain_quota,
            unlimited_quota: value.unlimited_quota,
            expired_at: value
                .expired_at
                .map(TimeDateTimeWithTimeZone::from_unix_timestamp)
                .transpose()
                .map_err(|_| OrganizationTokenRepositoryError::InvalidInput)?,
            model_limits: value
                .model_limits
                .map(|items| TokenModelAllowlist::validate(serde_json::json!(items)))
                .transpose()
                .map_err(|_| OrganizationTokenRepositoryError::InvalidInput)?,
            allow_ips: value
                .allow_ips
                .map(|items| TokenIpAllowlist::validate(serde_json::json!(items)))
                .transpose()
                .map_err(|_| OrganizationTokenRepositoryError::InvalidInput)?,
            rate_limit_5h: value.rate_limit_5h,
            rate_limit_1d: value.rate_limit_1d,
            rate_limit_7d: value.rate_limit_7d,
            max_requests: value.max_requests,
        })
    }
}

struct ProvisioningSnapshot {
    team_id: Option<OrganizationTeamId>,
}

async fn validate_provisioning(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    user_id: UserId,
) -> Result<ProvisioningSnapshot, OrganizationTokenRepositoryError> {
    let organization = lock_organization(transaction, organization_id).await?;
    let status = OrganizationStatus::try_from(organization.status)
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
    let membership = organization_memberships::Entity::find_by_id(membership_id.get())
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(user_id.get()))
        .one(transaction)
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?
        .ok_or(OrganizationTokenRepositoryError::NotFound)?;
    let membership_status = OrganizationMembershipStatus::try_from(membership.status)
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
    if !status.is_active() || !membership_status.is_active() {
        return Err(OrganizationTokenRepositoryError::Restricted);
    }
    let entitlement = organization_entitlements::Entity::find()
        .filter(organization_entitlements::Column::OrganizationId.eq(organization_id.get()))
        .one(transaction)
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?
        .ok_or(OrganizationTokenRepositoryError::Invariant)?;
    let snapshot = crate::organization::read::entitlement_snapshot(entitlement)
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
    if snapshot.status(status, TimeDateTimeWithTimeZone::now_utc().unix_timestamp())
        != OrganizationEntitlementStatus::Active
    {
        return Err(OrganizationTokenRepositoryError::Restricted);
    }
    let key_count = tokens::Entity::find()
        .filter(tokens::Column::OrganizationId.eq(organization_id.get()))
        .filter(tokens::Column::DeletedAt.is_null())
        .count(transaction)
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?;
    let limit = u64::try_from(snapshot.capacity().organization_key_limit())
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
    if key_count >= limit {
        return Err(OrganizationTokenRepositoryError::LimitReached);
    }
    let user = users::Entity::find_by_id(user_id.get())
        .filter(users::Column::Status.eq(1_i16))
        .filter(users::Column::DeletedAt.is_null())
        .one(transaction)
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?
        .ok_or(OrganizationTokenRepositoryError::Restricted)?;
    if groups::Entity::find_by_id(user.default_group_id)
        .filter(groups::Column::DeletedAt.is_null())
        .one(transaction)
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?
        .is_none()
    {
        return Err(OrganizationTokenRepositoryError::Invariant);
    }
    let team_id = membership
        .team_id
        .map(OrganizationTeamId::new)
        .transpose()
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
    if let Some(team_id) = team_id {
        if !snapshot.team_management_enabled() {
            return Err(OrganizationTokenRepositoryError::Restricted);
        }
        let team = organization_teams::Entity::find_by_id(team_id.get())
            .filter(organization_teams::Column::OrganizationId.eq(organization_id.get()))
            .one(transaction)
            .await
            .map_err(|_| OrganizationTokenRepositoryError::Query)?
            .ok_or(OrganizationTokenRepositoryError::Invariant)?;
        if !OrganizationTeamStatus::try_from(team.status)
            .map_err(|_| OrganizationTokenRepositoryError::Invariant)?
            .is_active()
        {
            return Err(OrganizationTokenRepositoryError::Restricted);
        }
    }
    Ok(ProvisioningSnapshot { team_id })
}

async fn validate_existing_member(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<(), OrganizationTokenRepositoryError> {
    let membership = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(user_id.get()))
        .one(transaction)
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?
        .ok_or(OrganizationTokenRepositoryError::NotFound)?;
    if !OrganizationMembershipStatus::try_from(membership.status)
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?
        .is_active()
    {
        return Err(OrganizationTokenRepositoryError::Restricted);
    }
    Ok(())
}

async fn ensure_entitlement_allows_provisioning(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<(), OrganizationTokenRepositoryError> {
    let organization = organizations::Entity::find_by_id(organization_id.get())
        .one(transaction)
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?
        .ok_or(OrganizationTokenRepositoryError::NotFound)?;
    let status = OrganizationStatus::try_from(organization.status)
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
    let entitlement = organization_entitlements::Entity::find()
        .filter(organization_entitlements::Column::OrganizationId.eq(organization_id.get()))
        .one(transaction)
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?
        .ok_or(OrganizationTokenRepositoryError::Invariant)?;
    if crate::organization::read::entitlement_snapshot(entitlement)
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?
        .status(status, TimeDateTimeWithTimeZone::now_utc().unix_timestamp())
        .allows_provisioning()
    {
        Ok(())
    } else {
        Err(OrganizationTokenRepositoryError::Restricted)
    }
}

async fn lock_organization(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<organizations::Model, OrganizationTokenRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = organizations::Entity::update_many()
            .filter(organizations::Column::Id.eq(organization_id.get()))
            .col_expr(
                organizations::Column::Version,
                Expr::col(organizations::Column::Version).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| OrganizationTokenRepositoryError::Query)?;
        if result.rows_affected != 1 {
            return Err(OrganizationTokenRepositoryError::NotFound);
        }
    }
    let mut query = organizations::Entity::find_by_id(organization_id.get());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?
        .ok_or(OrganizationTokenRepositoryError::NotFound)
}

fn organization_query(backend: DbBackend) -> sea_orm::sea_query::SelectStatement {
    base_query(backend)
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::OrganizationId)),
            Alias::new("organization_id"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::OrganizationMembershipId)),
            Alias::new("organization_membership_id"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::OrganizationTeamId)),
            Alias::new("organization_team_id"),
        )
        .to_owned()
}

fn record_from_row(
    row: &sea_orm::QueryResult,
) -> Result<OrganizationTokenRecord, OrganizationTokenRepositoryError> {
    let token = AdminTokenRecord::try_from_query_result(row)
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
    let organization_id = OrganizationId::new(
        row.try_get("", "organization_id")
            .map_err(|_| OrganizationTokenRepositoryError::Invariant)?,
    )
    .map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
    let membership_id = OrganizationMembershipId::new(
        row.try_get("", "organization_membership_id")
            .map_err(|_| OrganizationTokenRepositoryError::Invariant)?,
    )
    .map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
    let team_id = row
        .try_get::<Option<i64>>("", "organization_team_id")
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?
        .map(OrganizationTeamId::new)
        .transpose()
        .map_err(|_| OrganizationTokenRepositoryError::Invariant)?;
    Ok(OrganizationTokenRecord {
        token,
        organization_id,
        membership_id,
        team_id,
    })
}

async fn fetch_snapshot(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    token_id: TokenId,
) -> Result<OrganizationTokenRecord, OrganizationTokenRepositoryError> {
    let query = organization_query(transaction.get_database_backend())
        .and_where(tokens::Column::OrganizationId.eq(organization_id.get()))
        .and_where(tokens::Column::Id.eq(token_id.get()))
        .and_where(tokens::Column::DeletedAt.is_null())
        .limit(2)
        .to_owned();
    let rows = transaction
        .query_all(transaction.get_database_backend().build(&query))
        .await
        .map_err(|_| OrganizationTokenRepositoryError::Query)?;
    match rows.as_slice() {
        [row] => record_from_row(row),
        _ => Err(OrganizationTokenRepositoryError::Invariant),
    }
}

fn valid_key_prefix(value: &str) -> bool {
    value.len() == 18
        && value.starts_with("sk-af-")
        && value[6..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
