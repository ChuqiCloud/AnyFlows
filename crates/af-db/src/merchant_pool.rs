//! Merchant tenancy, approved supply and catalog access. Never exports keys.
use crate::{
    DatabasePool, DatabaseTimestamp, EncryptedCredentialEnvelope, SchedulerCatalogSubject,
    ability_write::{
        ChannelAbilityMetadata, ValidatedChannelRouting, set_channel_abilities_enabled,
        synchronize_channel_routing,
    },
    entity::{
        ChannelBaseUrl, EncryptedJson, HeaderOverrides, SensitiveJson, SensitiveString, channels,
        credentials, groups, merchant_pools as pools, merchant_profiles as profiles,
        merchant_service_revisions as revisions, merchant_services as services, model_prices,
        users,
    },
    group_key_access::user_can_select_key_group,
    scheduler_outbox::enqueue_scheduler_catalog_change,
    token_owner_guard::lock_non_deleted_owner,
};
use af_domain::{ChannelId, CredentialId, GroupId, Status, UserId};
pub use af_domain::{
    MerchantApplication, MerchantCatalogService, MerchantListQuery, MerchantModelOffer,
    MerchantModelPrices, MerchantModelWrite, MerchantOwnedService, MerchantPage, MerchantPool,
    MerchantPoolError, MerchantPoolWrite, MerchantProfile, MerchantReview, MerchantRevision,
    MerchantRuntimeCandidate, MerchantServiceWrite,
};
pub use af_domain::{MerchantPoolSettings, MerchantPoolTerms};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rust_decimal::Decimal;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, LockType},
};
use std::{future::Future, time::Duration};

mod catalog;
mod policies;
mod terms;
#[cfg(test)]
mod tests;
mod writes;

#[derive(Clone)]
pub struct MerchantPoolRepository {
    pool: DatabasePool,
    timeout: Duration,
}

/// Encrypted reference used only while re-binding an unchanged write-only key.
pub struct MerchantCredentialMaterial {
    pub channel_id: ChannelId,
    pub credential_id: CredentialId,
    pub envelope: EncryptedCredentialEnvelope,
}

impl MerchantPoolRepository {
    #[must_use]
    pub fn new(pool: DatabasePool, timeout: Duration) -> Self {
        Self { pool, timeout }
    }

    pub async fn platform_key_exists(&self, digest: &str) -> Result<bool, MerchantPoolError> {
        self.bounded(async {
            let hash = crate::entity::TokenHash::parse(digest).map_err(invalid)?;
            Ok(crate::entity::tokens::Entity::find()
                .filter(crate::entity::tokens::Column::KeyHash.eq(hash))
                .one(self.pool.connection())
                .await
                .map_err(internal)?
                .is_some())
        })
        .await
    }

    async fn bounded<T>(
        &self,
        future: impl Future<Output = Result<T, MerchantPoolError>>,
    ) -> Result<T, MerchantPoolError> {
        tokio::time::timeout(self.timeout, future)
            .await
            .map_err(|_| MerchantPoolError::Unavailable)?
    }

    pub async fn owner_merchant(
        &self,
        owner: UserId,
    ) -> Result<Option<MerchantProfile>, MerchantPoolError> {
        self.bounded(async {
            active_owner(self.pool.connection(), owner).await?;
            profiles::Entity::find()
                .filter(profiles::Column::OwnerUserId.eq(owner.get()))
                .one(self.pool.connection())
                .await
                .map_err(internal)
                .map(|row| row.map(profile_snapshot))
        })
        .await
    }

    pub async fn get_merchant(&self, id: i64) -> Result<MerchantProfile, MerchantPoolError> {
        self.bounded(async {
            profiles::Entity::find_by_id(id)
                .one(self.pool.connection())
                .await
                .map_err(internal)?
                .map(profile_snapshot)
                .ok_or(MerchantPoolError::NotFound)
        })
        .await
    }

    pub async fn get_pool(&self, id: i64) -> Result<MerchantPool, MerchantPoolError> {
        self.bounded(async {
            self.effective_pool(find_pool(self.pool.connection(), id).await?, None)
                .await
        })
        .await
    }

    pub async fn pool_for_group(
        &self,
        group_id: GroupId,
    ) -> Result<Option<MerchantPool>, MerchantPoolError> {
        self.bounded(async {
            let row = pools::Entity::find()
                .filter(pools::Column::GroupId.eq(group_id.get()))
                .one(self.pool.connection())
                .await
                .map_err(internal)?;
            match row {
                Some(row) => Ok(Some(self.effective_pool(row, None).await?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub async fn check_pool_access(
        &self,
        owner: UserId,
        pool_id: i64,
    ) -> Result<bool, MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let group = active_owner(&txn, owner).await?;
            let pool = find_pool(&txn, pool_id).await?;
            if !pool.enabled {
                return Ok(false);
            }
            let target = GroupId::new(pool.group_id).map_err(|_| MerchantPoolError::Unavailable)?;
            let allowed = user_can_select_key_group(&txn, owner, group, target)
                .await
                .map_err(internal)?;
            txn.commit().await.map_err(internal)?;
            Ok(allowed)
        })
        .await
    }

    pub async fn list_profiles(
        &self,
        query: MerchantListQuery,
    ) -> Result<MerchantPage<MerchantProfile>, MerchantPoolError> {
        self.bounded(async {
            let (page, size) = pagination(&query)?;
            let mut selection = profiles::Entity::find();
            if let Some(status) = &query.status {
                selection = selection.filter(profiles::Column::Status.eq(status));
            }
            if let Some(search) = &query.search {
                selection = selection.filter(profiles::Column::Name.contains(search));
            }
            let total = selection
                .clone()
                .count(self.pool.connection())
                .await
                .map_err(internal)?;
            let rows = selection
                .order_by_desc(profiles::Column::Id)
                .offset(u64::from(page - 1) * u64::from(size))
                .limit(u64::from(size))
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            Ok(MerchantPage {
                items: rows.into_iter().map(profile_snapshot).collect(),
                total,
                page,
                page_size: size,
            })
        })
        .await
    }

    pub async fn list_pools(
        &self,
        owner: Option<UserId>,
        query: MerchantListQuery,
    ) -> Result<MerchantPage<MerchantPool>, MerchantPoolError> {
        self.bounded(async {
            let mut selection = pools::Entity::find();
            if owner.is_some() {
                selection = selection.filter(pools::Column::Enabled.eq(true));
            }
            if let Some(search) = &query.search {
                selection = selection.filter(pools::Column::Name.contains(search));
            }
            let rows = selection
                .order_by_asc(pools::Column::Id)
                .limit(1001)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            if rows.len() > 1000 {
                return Err(MerchantPoolError::LimitReached);
            }
            let mut result = Vec::new();
            for row in rows {
                if let Some(user) = owner
                    && !self.check_pool_access(user, row.id).await?
                {
                    continue;
                }
                result.push(self.effective_pool(row, None).await?);
            }
            page_items(result, &query)
        })
        .await
    }

    pub async fn list_services(
        &self,
        owner: Option<UserId>,
        query: MerchantListQuery,
    ) -> Result<MerchantPage<MerchantOwnedService>, MerchantPoolError> {
        self.bounded(async {
            let mut selection = services::Entity::find();
            if let Some(owner) = owner {
                let merchant = require_merchant(self.pool.connection(), owner, false).await?;
                selection = selection.filter(services::Column::MerchantId.eq(merchant.id));
            }
            if let Some(search) = &query.search {
                selection = selection.filter(services::Column::Name.contains(search));
            }
            // Pending revisions on a published service must remain visible in review queues.
            if let Some(status) = &query.status {
                if status == "pending" {
                    let pending: Vec<i64> = revisions::Entity::find()
                        .select_only()
                        .column(revisions::Column::ServiceId)
                        .filter(revisions::Column::Status.eq("pending"))
                        .into_tuple()
                        .all(self.pool.connection())
                        .await
                        .map_err(internal)?;
                    selection = selection.filter(services::Column::Id.is_in(pending));
                } else {
                    selection = selection.filter(services::Column::Status.eq(status));
                }
            }
            let (page, size) = pagination(&query)?;
            let total = selection
                .clone()
                .count(self.pool.connection())
                .await
                .map_err(internal)?;
            let rows = selection
                .order_by_desc(services::Column::Id)
                .offset(u64::from(page - 1) * u64::from(size))
                .limit(u64::from(size))
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                self.apply_due_service_prices(row.id, DatabaseTimestamp::now_utc())
                    .await?;
                items.push(service_snapshot(self.pool.connection(), row).await?);
            }
            Ok(MerchantPage {
                items,
                total,
                page,
                page_size: size,
            })
        })
        .await
    }

    pub async fn get_service(
        &self,
        owner: Option<UserId>,
        id: i64,
    ) -> Result<MerchantOwnedService, MerchantPoolError> {
        self.bounded(async {
            let row = find_service(self.pool.connection(), id).await?;
            if let Some(user) = owner {
                let merchant = require_merchant(self.pool.connection(), user, false).await?;
                if merchant.id != row.merchant_id {
                    return Err(MerchantPoolError::NotFound);
                }
            }
            self.apply_due_service_prices(id, DatabaseTimestamp::now_utc())
                .await?;
            service_snapshot(self.pool.connection(), row).await
        })
        .await
    }
}

fn internal(_: impl std::fmt::Debug) -> MerchantPoolError {
    MerchantPoolError::Unavailable
}
fn invalid(_: impl std::fmt::Debug) -> MerchantPoolError {
    MerchantPoolError::InvalidInput
}

async fn active_owner<C: ConnectionTrait>(
    connection: &C,
    owner: UserId,
) -> Result<GroupId, MerchantPoolError> {
    let user = users::Entity::find_by_id(owner.get())
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::Status.eq(Status::Enabled.code()))
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::Forbidden)?;
    GroupId::new(user.default_group_id).map_err(internal)
}

async fn lock_owner(txn: &DatabaseTransaction, owner: UserId) -> Result<(), MerchantPoolError> {
    let row = lock_non_deleted_owner(txn, owner)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::Forbidden)?;
    if row.status() != Status::Enabled.code() {
        return Err(MerchantPoolError::Forbidden);
    }
    Ok(())
}

async fn require_merchant<C: ConnectionTrait>(
    connection: &C,
    owner: UserId,
    approved: bool,
) -> Result<profiles::Model, MerchantPoolError> {
    active_owner(connection, owner).await?;
    let row = profiles::Entity::find()
        .filter(profiles::Column::OwnerUserId.eq(owner.get()))
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::NotFound)?;
    if approved && row.status != "approved" {
        return Err(MerchantPoolError::Forbidden);
    }
    Ok(row)
}

async fn find_pool<C: ConnectionTrait>(
    connection: &C,
    id: i64,
) -> Result<pools::Model, MerchantPoolError> {
    pools::Entity::find_by_id(id)
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::NotFound)
}
async fn find_service<C: ConnectionTrait>(
    connection: &C,
    id: i64,
) -> Result<services::Model, MerchantPoolError> {
    services::Entity::find_by_id(id)
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::NotFound)
}
async fn lock_service(
    txn: &DatabaseTransaction,
    id: i64,
) -> Result<services::Model, MerchantPoolError> {
    if txn.get_database_backend() == DbBackend::Sqlite {
        services::Entity::update_many()
            .filter(services::Column::Id.eq(id))
            .col_expr(services::Column::Id, Expr::col(services::Column::Id).into())
            .exec(txn)
            .await
            .map_err(internal)?;
    }
    let mut selection = services::Entity::find_by_id(id);
    if txn.get_database_backend() != DbBackend::Sqlite {
        selection = selection.lock(LockType::Update);
    }
    selection
        .one(txn)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::NotFound)
}

fn profile_snapshot(row: profiles::Model) -> MerchantProfile {
    MerchantProfile {
        id: row.id,
        owner_user_id: row.owner_user_id,
        name: row.name,
        description: row.description,
        status: row.status,
        review_reason: row.review_reason,
        created_at: row.created_at.unix_timestamp(),
        updated_at: row.updated_at.unix_timestamp(),
    }
}
fn pool_snapshot(row: pools::Model) -> Result<MerchantPool, MerchantPoolError> {
    Ok(MerchantPool {
        id: row.id,
        group_id: row.group_id,
        name: row.name,
        description: row.description,
        enabled: row.enabled,
        commission_bps: commission_value(row.commission_bps)?,
        commission_inherit_global: row.commission_inherit_global,
        effective_commission_bps: commission_value(row.commission_bps)?,
        min_ratio_micros: row.min_ratio_micros,
        max_ratio_micros: row.max_ratio_micros,
        settlement_delay_secs: row.settlement_delay_secs.try_into().map_err(internal)?,
        created_at: row.created_at.unix_timestamp(),
        updated_at: row.updated_at.unix_timestamp(),
    })
}
fn commission_value(value: i32) -> Result<u16, MerchantPoolError> {
    if !(0..=10_000).contains(&value) {
        return Err(MerchantPoolError::Unavailable);
    }
    value.try_into().map_err(internal)
}
fn revision_models(row: &revisions::Model) -> Result<Vec<MerchantModelWrite>, MerchantPoolError> {
    serde_json::from_value(row.models.clone()).map_err(internal)
}
fn revision_snapshot(row: revisions::Model) -> Result<MerchantRevision, MerchantPoolError> {
    let models = revision_models(&row)?;
    Ok(MerchantRevision {
        id: row.id,
        revision: row.revision,
        status: row.status,
        name: row.name,
        description: row.description,
        base_url: row.base_url.as_str().to_owned(),
        protocol: row.protocol,
        default_ratio_micros: row.default_ratio_micros,
        models,
        concurrency: row
            .concurrency
            .map(u32::try_from)
            .transpose()
            .map_err(internal)?,
        timeout_secs: row.timeout_secs.try_into().map_err(internal)?,
        has_api_key: true,
        review_reason: row.review_reason,
        created_at: row.created_at.unix_timestamp(),
        updated_at: row.updated_at.unix_timestamp(),
    })
}
async fn service_snapshot<C: ConnectionTrait>(
    connection: &C,
    row: services::Model,
) -> Result<MerchantOwnedService, MerchantPoolError> {
    let published = match row.published_revision_id {
        Some(id) => Some(load_revision(connection, id, row.id).await?),
        None => None,
    };
    let draft = match row.pending_revision_id {
        Some(id) => Some(load_revision(connection, id, row.id).await?),
        None => None,
    };
    Ok(MerchantOwnedService {
        id: row.id,
        merchant_id: row.merchant_id,
        pool_id: row.pool_id,
        name: row.name,
        description: row.description,
        status: row.status,
        published: published.map(revision_snapshot).transpose()?,
        draft: draft.map(revision_snapshot).transpose()?,
        active_price: policies::active_price(connection, row.id, row.published_revision_id)
            .await?
            .map(policies::price_snapshot)
            .transpose()?,
        maintenance: policies::active_maintenance(
            connection,
            row.id,
            DatabaseTimestamp::now_utc().unix_timestamp(),
        )
        .await?,
        created_at: row.created_at.unix_timestamp(),
        updated_at: row.updated_at.unix_timestamp(),
    })
}
async fn load_revision<C: ConnectionTrait>(
    connection: &C,
    id: i64,
    service: i64,
) -> Result<revisions::Model, MerchantPoolError> {
    revisions::Entity::find_by_id(id)
        .filter(revisions::Column::ServiceId.eq(service))
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::Unavailable)
}
fn pagination(query: &MerchantListQuery) -> Result<(u32, u32), MerchantPoolError> {
    let page = query.page.unwrap_or(1);
    let size = query.page_size.unwrap_or(20);
    if page == 0
        || page > 1_000_000
        || size == 0
        || size > 100
        || query.search.as_ref().is_some_and(|s| s.len() > 128)
        || query.model.as_ref().is_some_and(|s| s.len() > 255)
        || query.protocol.as_ref().is_some_and(|s| {
            !matches!(
                s.as_str(),
                "openai_chat"
                    | "openai_responses"
                    | "openai_images"
                    | "openai_audio"
                    | "openai_speech"
                    | "xai_video"
            )
        })
        || query
            .capability
            .as_ref()
            .is_some_and(|s| !matches!(s.as_str(), "streaming" | "tools" | "vision"))
        || query
            .min_ratio_micros
            .is_some_and(|n| !(0..=100_000_000).contains(&n))
        || query
            .max_ratio_micros
            .is_some_and(|n| !(0..=100_000_000).contains(&n))
        || matches!((query.min_ratio_micros,query.max_ratio_micros),(Some(min),Some(max)) if min>max)
        || query
            .window_seconds
            .is_some_and(|n| ![900, 3600, 86400, 604800].contains(&n))
        || query.status.as_ref().is_some_and(|s| s.len() > 32)
        || query
            .sort
            .as_ref()
            .is_some_and(|s| !matches!(s.as_str(), "price" | "name" | "success_rate" | "latency"))
        || (query
            .sort
            .as_ref()
            .is_some_and(|s| matches!(s.as_str(), "price" | "success_rate" | "latency"))
            && query.model.as_ref().is_none_or(|model| model.is_empty()))
        || (query.sort.as_deref() == Some("latency") && query.streaming.is_none())
    {
        return Err(MerchantPoolError::InvalidInput);
    }
    Ok((page, size))
}
fn page_items<T>(
    items: Vec<T>,
    query: &MerchantListQuery,
) -> Result<MerchantPage<T>, MerchantPoolError> {
    let (page, size) = pagination(query)?;
    let total = items.len() as u64;
    Ok(MerchantPage {
        items: items
            .into_iter()
            .skip(((page - 1) * size) as usize)
            .take(size as usize)
            .collect(),
        total,
        page,
        page_size: size,
    })
}

async fn audit_control<C: ConnectionTrait>(
    connection: &C,
    actor: UserId,
    action: &str,
    target: i64,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
) -> Result<(), MerchantPoolError> {
    let now = DatabaseTimestamp::now_utc();
    crate::entity::platform_audit_logs::ActiveModel {
        operator_user_id: Set(actor.get()),
        permission_code: Set("merchant_pool.manage".to_owned()),
        route: Set("/api/admin/merchant-pools".to_owned()),
        operation: Set(action.to_owned()),
        resource: Set("merchant_pool".to_owned()),
        resource_id: Set(Some(target.to_string())),
        outcome: Set(1),
        before_value: Set(before.map(|value| value.to_string())),
        after_value: Set(after.map(|value| value.to_string())),
        audit_info: Set(None),
        request_id: Set(format!("merchant:{target}:{}", now.unix_timestamp_nanos())),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(connection)
    .await
    .map_err(internal)?;
    Ok(())
}
