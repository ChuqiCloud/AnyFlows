use super::*;
use crate::entity::{merchant_service_watch_events as events, merchant_service_watches as watches};
use sea_orm::sea_query::OnConflict;

pub const MAX_MERCHANT_SERVICE_WATCHES_PER_USER: u64 = 1000;
pub const MAX_MERCHANT_SERVICE_WATCHERS_PER_SERVICE: u64 = 500;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantServiceWatch {
    pub pool_id: i64,
    pub pool_name: String,
    pub service_id: i64,
    pub merchant_id: i64,
    pub service_name: String,
    pub merchant_name: String,
    pub active: bool,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantServiceWatchPage {
    pub items: Vec<MerchantServiceWatch>,
    pub total: u64,
}

impl MerchantPoolRepository {
    pub async fn watch_service(
        &self,
        owner: UserId,
        pool_id: i64,
        service_id: i64,
    ) -> Result<MerchantServiceWatch, MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_owner(&txn, owner).await?;
            let default_group = active_owner(&txn, owner).await?;
            let pool = find_pool(&txn, pool_id).await?;
            if !pool.enabled {
                return Err(MerchantPoolError::NotFound);
            }
            let target_group = GroupId::new(pool.group_id).map_err(internal)?;
            if !user_can_select_key_group(&txn, owner, default_group, target_group)
                .await
                .map_err(internal)?
            {
                return Err(MerchantPoolError::NotFound);
            }
            let service = lock_service(&txn, service_id).await?;
            if service.pool_id != pool_id || service.published_revision_id.is_none() {
                return Err(MerchantPoolError::NotFound);
            }

            let existing = watches::Entity::find()
                .filter(watches::Column::UserId.eq(owner.get()))
                .filter(watches::Column::ServiceId.eq(service_id))
                .one(&txn)
                .await
                .map_err(internal)?;
            let now = DatabaseTimestamp::now_utc();
            let row = if let Some(existing) = existing {
                if !existing.active {
                    let total = watches::Entity::find()
                        .filter(watches::Column::UserId.eq(owner.get()))
                        .filter(watches::Column::Active.eq(true))
                        .count(&txn)
                        .await
                        .map_err(internal)?;
                    let followers = watches::Entity::find()
                        .filter(watches::Column::ServiceId.eq(service_id))
                        .filter(watches::Column::Active.eq(true))
                        .count(&txn)
                        .await
                        .map_err(internal)?;
                    if total >= MAX_MERCHANT_SERVICE_WATCHES_PER_USER
                        || followers >= MAX_MERCHANT_SERVICE_WATCHERS_PER_SERVICE
                    {
                        return Err(MerchantPoolError::LimitReached);
                    }
                }
                let mut active = existing.into_active_model();
                active.pool_id = Set(pool_id);
                active.active = Set(true);
                active.updated_at = Set(now);
                active.update(&txn).await.map_err(internal)?
            } else {
                let total = watches::Entity::find()
                    .filter(watches::Column::UserId.eq(owner.get()))
                    .filter(watches::Column::Active.eq(true))
                    .count(&txn)
                    .await
                    .map_err(internal)?;
                if total >= MAX_MERCHANT_SERVICE_WATCHES_PER_USER {
                    return Err(MerchantPoolError::LimitReached);
                }
                let followers = watches::Entity::find()
                    .filter(watches::Column::ServiceId.eq(service_id))
                    .filter(watches::Column::Active.eq(true))
                    .count(&txn)
                    .await
                    .map_err(internal)?;
                if followers >= MAX_MERCHANT_SERVICE_WATCHERS_PER_SERVICE {
                    return Err(MerchantPoolError::LimitReached);
                }
                watches::ActiveModel {
                    user_id: Set(owner.get()),
                    pool_id: Set(pool_id),
                    service_id: Set(service_id),
                    active: Set(true),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(&txn)
                .await
                .map_err(internal)?
            };
            let result = watch_snapshot(&txn, row).await?;
            txn.commit().await.map_err(internal)?;
            Ok(result)
        })
        .await
    }

    /// Unwatch is keyed by the session user, so it remains possible after pool access is revoked.
    pub async fn unwatch_service(
        &self,
        owner: UserId,
        pool_id: i64,
        service_id: i64,
    ) -> Result<bool, MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let row = watches::Entity::find()
                .filter(watches::Column::UserId.eq(owner.get()))
                .filter(watches::Column::PoolId.eq(pool_id))
                .filter(watches::Column::ServiceId.eq(service_id))
                .filter(watches::Column::Active.eq(true))
                .one(&txn)
                .await
                .map_err(internal)?;
            let Some(row) = row else {
                txn.commit().await.map_err(internal)?;
                return Ok(false);
            };
            let now = DatabaseTimestamp::now_utc();
            let mut active = row.into_active_model();
            active.active = Set(false);
            active.updated_at = Set(now);
            active.update(&txn).await.map_err(internal)?;
            txn.commit().await.map_err(internal)?;
            Ok(true)
        })
        .await
    }

    pub async fn watched_services(
        &self,
        owner: UserId,
    ) -> Result<MerchantServiceWatchPage, MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let default_group = active_owner(&txn, owner).await?;
            let rows = watches::Entity::find()
                .filter(watches::Column::UserId.eq(owner.get()))
                .filter(watches::Column::Active.eq(true))
                .order_by_desc(watches::Column::UpdatedAt)
                .order_by_desc(watches::Column::Id)
                .limit(MAX_MERCHANT_SERVICE_WATCHES_PER_USER + 1)
                .all(&txn)
                .await
                .map_err(internal)?;
            if rows.len() as u64 > MAX_MERCHANT_SERVICE_WATCHES_PER_USER {
                return Err(MerchantPoolError::LimitReached);
            }
            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                let pool = match find_pool(&txn, row.pool_id).await {
                    Ok(pool) => pool,
                    Err(MerchantPoolError::NotFound) => continue,
                    Err(error) => return Err(error),
                };
                if !pool.enabled {
                    continue;
                }
                let target_group = GroupId::new(pool.group_id).map_err(internal)?;
                if !user_can_select_key_group(&txn, owner, default_group, target_group)
                    .await
                    .map_err(internal)?
                {
                    continue;
                }
                let snapshot = match watch_snapshot(&txn, row).await {
                    Ok(snapshot) => snapshot,
                    Err(MerchantPoolError::NotFound) => continue,
                    Err(error) => return Err(error),
                };
                items.push(snapshot);
            }
            let page = MerchantServiceWatchPage {
                total: items.len() as u64,
                items,
            };
            txn.commit().await.map_err(internal)?;
            Ok(page)
        })
        .await
    }
}

async fn watch_snapshot<C: ConnectionTrait>(
    connection: &C,
    row: watches::Model,
) -> Result<MerchantServiceWatch, MerchantPoolError> {
    let pool = find_pool(connection, row.pool_id).await?;
    let service = find_service(connection, row.service_id).await?;
    if service.pool_id != pool.id {
        return Err(MerchantPoolError::NotFound);
    }
    let merchant = profiles::Entity::find_by_id(service.merchant_id)
        .filter(profiles::Column::DeletedAt.is_null())
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::NotFound)?;
    Ok(MerchantServiceWatch {
        pool_id: pool.id,
        pool_name: pool.name,
        service_id: service.id,
        merchant_id: service.merchant_id,
        service_name: service.name,
        merchant_name: merchant.name,
        active: row.active,
        created_at: row.created_at.unix_timestamp(),
    })
}

/// Called inside the transaction that makes a public catalog transition effective.
pub(super) async fn record_service_event(
    txn: &DatabaseTransaction,
    service: &services::Model,
    event_kind: &str,
    transition_key: &str,
    occurred_at: DatabaseTimestamp,
) -> Result<(), MerchantPoolError> {
    const EVENT_KINDS: &[&str] = &[
        "price_changed",
        "maintenance_announced",
        "maintenance_updated",
        "maintenance_canceled",
        "maintenance_ended",
        "service_updated",
        "service_published",
        "service_suspended",
        "service_restored",
    ];
    if service.deleted_at.is_some()
        || service.published_revision_id.is_none()
        || !EVENT_KINDS.contains(&event_kind)
        || transition_key.is_empty()
        || transition_key.len() > 64
    {
        return Ok(());
    }
    let pool = find_pool(txn, service.pool_id).await?;
    if !pool.enabled {
        return Ok(());
    }
    let source_key = format!("service:{}:{event_kind}:{transition_key}", service.id);
    if source_key.len() > 128 {
        return Err(MerchantPoolError::InvalidInput);
    }
    let followers = watches::Entity::find()
        .filter(watches::Column::ServiceId.eq(service.id))
        .filter(watches::Column::Active.eq(true))
        .order_by_asc(watches::Column::UserId)
        .limit(MAX_MERCHANT_SERVICE_WATCHERS_PER_SERVICE)
        .all(txn)
        .await
        .map_err(internal)?;
    if followers.is_empty() {
        return Ok(());
    }
    let event = events::Entity::insert(events::ActiveModel {
        pool_id: Set(pool.id),
        service_id: Set(service.id),
        merchant_id: Set(service.merchant_id),
        event_kind: Set(event_kind.to_owned()),
        source_key: Set(source_key.clone()),
        service_name: Set(service.name.clone()),
        occurred_at: Set(occurred_at),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::column(events::Column::SourceKey)
            .do_nothing_on([events::Column::SourceKey])
            .to_owned(),
    )
    .exec_without_returning(txn)
    .await
    .map_err(internal)?;
    let _ = event;
    let row = events::Entity::find()
        .filter(events::Column::SourceKey.eq(source_key))
        .one(txn)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::Unavailable)?;
    for follower in followers {
        let owner = match UserId::new(follower.user_id) {
            Ok(owner) => owner,
            Err(_) => continue,
        };
        let default_group = match active_owner(txn, owner).await {
            Ok(group) => group,
            Err(MerchantPoolError::Forbidden) => continue,
            Err(error) => return Err(error),
        };
        let target_group = GroupId::new(pool.group_id).map_err(internal)?;
        if !user_can_select_key_group(txn, owner, default_group, target_group)
            .await
            .map_err(internal)?
        {
            continue;
        }
        crate::notification::insert_merchant_service_update(txn, owner.get(), row.id, occurred_at)
            .await
            .map_err(internal)?;
    }
    Ok(())
}

pub(super) async fn record_elapsed_maintenance_events(
    txn: &DatabaseTransaction,
    service: &services::Model,
    now: DatabaseTimestamp,
) -> Result<usize, MerchantPoolError> {
    let rows = crate::entity::merchant_maintenance::Entity::find()
        .filter(crate::entity::merchant_maintenance::Column::ServiceId.eq(service.id))
        .filter(crate::entity::merchant_maintenance::Column::Cancelled.eq(false))
        .filter(crate::entity::merchant_maintenance::Column::WatchEventEmittedAt.is_null())
        .filter(crate::entity::merchant_maintenance::Column::EndsAt.lte(now.unix_timestamp()))
        .order_by_asc(crate::entity::merchant_maintenance::Column::EndsAt)
        .order_by_asc(crate::entity::merchant_maintenance::Column::Id)
        .all(txn)
        .await
        .map_err(internal)?;
    let count = rows.len();
    for row in rows {
        record_service_event(
            txn,
            service,
            "maintenance_ended",
            &format!("notice:{}:end:{}", row.id, row.ends_at),
            now,
        )
        .await?;
        crate::entity::merchant_maintenance::Entity::update_many()
            .filter(crate::entity::merchant_maintenance::Column::Id.eq(row.id))
            .filter(crate::entity::merchant_maintenance::Column::WatchEventEmittedAt.is_null())
            .col_expr(
                crate::entity::merchant_maintenance::Column::WatchEventEmittedAt,
                Expr::value(now),
            )
            .exec(txn)
            .await
            .map_err(internal)?;
    }
    Ok(count)
}
