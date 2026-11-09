//! Sales prices and maintenance are independent of immutable approved credentials.
use super::*;
use crate::entity::{merchant_maintenance as maintenance, merchant_service_prices as prices};
use af_domain::{
    MerchantMaintenance, MerchantMaintenanceWrite, MerchantModelRatio, MerchantServicePrice,
    MerchantServicePriceWrite,
};

impl MerchantPoolRepository {
    pub async fn service_prices(
        &self,
        owner: Option<UserId>,
        service: i64,
        query: MerchantListQuery,
    ) -> Result<MerchantPage<MerchantServicePrice>, MerchantPoolError> {
        self.bounded(async {
            scoped_service(self.pool.connection(), owner, service).await?;
            self.apply_due_service_prices(service, DatabaseTimestamp::now_utc())
                .await?;
            let (page, size) = pagination(&query)?;
            let selection = prices::Entity::find().filter(prices::Column::ServiceId.eq(service));
            let total = selection
                .clone()
                .count(self.pool.connection())
                .await
                .map_err(internal)?;
            let rows = selection
                .order_by_desc(prices::Column::EffectiveAt)
                .order_by_desc(prices::Column::Id)
                .offset(u64::from(page - 1) * u64::from(size))
                .limit(u64::from(size))
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            Ok(MerchantPage {
                items: rows
                    .into_iter()
                    .map(price_snapshot)
                    .collect::<Result<_, _>>()?,
                total,
                page,
                page_size: size,
            })
        })
        .await
    }

    pub async fn schedule_service_price(
        &self,
        owner: Option<UserId>,
        actor: UserId,
        service: i64,
        write: MerchantServicePriceWrite,
    ) -> Result<MerchantServicePrice, MerchantPoolError> {
        let now = DatabaseTimestamp::now_utc();
        write.validate(now.unix_timestamp())?;
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let service = lock_scoped_service(&txn, owner, service).await?;
            if service.published_revision_id != Some(write.expected_revision_id) { return Err(MerchantPoolError::Conflict); }
            let revision = load_revision(&txn, write.expected_revision_id, service.id).await?;
            if revision.status != "published" { return Err(MerchantPoolError::Conflict); }
            if active_price(&txn, service.id, Some(revision.id)).await?
                .is_some_and(|price| price.effective_at > write.effective_at) { return Err(MerchantPoolError::Conflict); }
            let mut models = revision_models(&revision)?;
            if write.model_ratios.iter().any(|item| !models.iter().any(|model| model.model == item.model)) {
                return Err(MerchantPoolError::InvalidInput);
            }
            for model in &mut models {
                model.ratio_micros = write.model_ratios.iter().find(|item| item.model == model.model)
                    .and_then(|item| item.ratio_micros);
            }
            let pool = find_pool(&txn, service.pool_id).await?;
            validate_price(&txn, &pool, write.default_ratio_micros, &models).await?;
            if prices::Entity::find().filter(prices::Column::ServiceId.eq(service.id))
                .filter(prices::Column::Status.eq("scheduled")).count(&txn).await.map_err(internal)? >= 32 {
                return Err(MerchantPoolError::LimitReached);
            }
            if prices::Entity::find().filter(prices::Column::ServiceId.eq(service.id))
                .filter(prices::Column::RevisionId.eq(revision.id))
                .filter(prices::Column::EffectiveAt.eq(write.effective_at))
                .filter(prices::Column::Status.is_in(["scheduled", "active"]))
                .one(&txn).await.map_err(internal)?.is_some() { return Err(MerchantPoolError::Conflict); }
            let row = prices::ActiveModel {
                service_id: Set(service.id), revision_id: Set(revision.id), effective_at: Set(write.effective_at),
                default_ratio_micros: Set(write.default_ratio_micros), models: Set(serde_json::to_value(models).map_err(internal)?),
                status: Set("scheduled".to_owned()), reason: Set(None), created_at: Set(now), updated_at: Set(now), ..Default::default()
            }.insert(&txn).await.map_err(internal)?;
            audit_control(&txn, actor, "service.price.schedule", service.id, None,
                Some(serde_json::json!({"price_id":row.id,"revision_id":revision.id,"effective_at":row.effective_at}))).await?;
            apply_due_in_transaction(&txn, &service, now).await?;
            let result = prices::Entity::find_by_id(row.id).one(&txn).await.map_err(internal)?
                .ok_or(MerchantPoolError::Unavailable)?;
            txn.commit().await.map_err(internal)?;
            price_snapshot(result)
        }).await
    }

    pub async fn cancel_service_price(
        &self,
        owner: Option<UserId>,
        actor: UserId,
        service: i64,
        price: i64,
    ) -> Result<MerchantServicePrice, MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let service = lock_scoped_service(&txn, owner, service).await?;
            let now = DatabaseTimestamp::now_utc();
            apply_due_in_transaction(&txn, &service, now).await?;
            let row = prices::Entity::find_by_id(price)
                .filter(prices::Column::ServiceId.eq(service.id))
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(MerchantPoolError::NotFound)?;
            if !matches!(row.status.as_str(), "scheduled" | "cancelled") {
                return Err(MerchantPoolError::Conflict);
            }
            let mut active = row.into_active_model();
            active.status = Set("cancelled".to_owned());
            active.updated_at = Set(now);
            let result = active.update(&txn).await.map_err(internal)?;
            audit_control(
                &txn,
                actor,
                "service.price.cancel",
                service.id,
                None,
                Some(serde_json::json!({"price_id":price})),
            )
            .await?;
            txn.commit().await.map_err(internal)?;
            price_snapshot(result)
        })
        .await
    }

    /// Runs without page requests; service row locks make activation safe across replicas.
    pub async fn process_due_pricing(&self, limit: usize) -> Result<usize, MerchantPoolError> {
        if !(1..=1000).contains(&limit) {
            return Err(MerchantPoolError::InvalidInput);
        }
        self.bounded(async {
            let now = DatabaseTimestamp::now_utc();
            let services: Vec<i64> = prices::Entity::find()
                .select_only()
                .column(prices::Column::ServiceId)
                .filter(prices::Column::Status.eq("scheduled"))
                .filter(prices::Column::EffectiveAt.lte(now.unix_timestamp()))
                .order_by_asc(prices::Column::EffectiveAt)
                .order_by_asc(prices::Column::Id)
                .limit(limit as u64)
                .into_tuple()
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            let mut processed = 0;
            for service in services
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
            {
                processed += self.apply_due_service_prices(service, now).await?;
            }
            Ok(processed)
        })
        .await
    }

    pub(super) async fn apply_due_service_prices(
        &self,
        service: i64,
        now: DatabaseTimestamp,
    ) -> Result<usize, MerchantPoolError> {
        // Most catalog reads need no write lock.
        if prices::Entity::find()
            .filter(prices::Column::ServiceId.eq(service))
            .filter(prices::Column::Status.eq("scheduled"))
            .filter(prices::Column::EffectiveAt.lte(now.unix_timestamp()))
            .one(self.pool.connection())
            .await
            .map_err(internal)?
            .is_none()
        {
            return Ok(0);
        }
        let txn = self.pool.connection().begin().await.map_err(internal)?;
        let service = lock_service(&txn, service).await?;
        let processed = apply_due_in_transaction(&txn, &service, now).await?;
        txn.commit().await.map_err(internal)?;
        Ok(processed)
    }

    pub async fn service_maintenance(
        &self,
        owner: Option<UserId>,
        service: i64,
        query: MerchantListQuery,
    ) -> Result<MerchantPage<MerchantMaintenance>, MerchantPoolError> {
        self.bounded(async {
            scoped_service(self.pool.connection(), owner, service).await?;
            let (page, size) = pagination(&query)?;
            let selection =
                maintenance::Entity::find().filter(maintenance::Column::ServiceId.eq(service));
            let total = selection
                .clone()
                .count(self.pool.connection())
                .await
                .map_err(internal)?;
            let now = DatabaseTimestamp::now_utc().unix_timestamp();
            let items = selection
                .order_by_desc(maintenance::Column::StartsAt)
                .order_by_desc(maintenance::Column::Id)
                .offset(u64::from(page - 1) * u64::from(size))
                .limit(u64::from(size))
                .all(self.pool.connection())
                .await
                .map_err(internal)?
                .into_iter()
                .map(|row| maintenance_snapshot(row, now))
                .collect();
            Ok(MerchantPage {
                items,
                total,
                page,
                page_size: size,
            })
        })
        .await
    }

    pub async fn write_service_maintenance(
        &self,
        owner: Option<UserId>,
        actor: UserId,
        service: i64,
        notice: Option<i64>,
        write: MerchantMaintenanceWrite,
    ) -> Result<MerchantMaintenance, MerchantPoolError> {
        let now = DatabaseTimestamp::now_utc();
        write.validate(now.unix_timestamp())?;
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let service = lock_scoped_service(&txn, owner, service).await?;
            let mut active = if let Some(id) = notice {
                let row = maintenance::Entity::find_by_id(id).filter(maintenance::Column::ServiceId.eq(service.id))
                    .one(&txn).await.map_err(internal)?.ok_or(MerchantPoolError::NotFound)?;
                if row.cancelled || row.ends_at <= now.unix_timestamp() { return Err(MerchantPoolError::Conflict); }
                row.into_active_model()
            } else {
                if maintenance::Entity::find().filter(maintenance::Column::ServiceId.eq(service.id))
                    .filter(maintenance::Column::Cancelled.eq(false)).filter(maintenance::Column::EndsAt.gt(now.unix_timestamp()))
                    .count(&txn).await.map_err(internal)? >= 32 { return Err(MerchantPoolError::LimitReached); }
                maintenance::ActiveModel { service_id: Set(service.id), cancelled: Set(false), created_at: Set(now), ..Default::default() }
            };
            active.starts_at = Set(write.starts_at); active.ends_at = Set(write.ends_at); active.message = Set(write.message);
            active.blocks_requests = Set(write.blocks_requests); active.updated_at = Set(now);
            let row = if notice.is_some() { active.update(&txn).await } else { active.insert(&txn).await }.map_err(internal)?;
            audit_control(&txn, actor, "service.maintenance.write", service.id, None,
                Some(serde_json::json!({"notice_id":row.id,"starts_at":row.starts_at,"ends_at":row.ends_at,"blocks_requests":row.blocks_requests}))).await?;
            txn.commit().await.map_err(internal)?;
            Ok(maintenance_snapshot(row, now.unix_timestamp()))
        }).await
    }

    pub async fn cancel_service_maintenance(
        &self,
        owner: Option<UserId>,
        actor: UserId,
        service: i64,
        notice: i64,
    ) -> Result<MerchantMaintenance, MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let service = lock_scoped_service(&txn, owner, service).await?;
            let row = maintenance::Entity::find_by_id(notice)
                .filter(maintenance::Column::ServiceId.eq(service.id))
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(MerchantPoolError::NotFound)?;
            let now = DatabaseTimestamp::now_utc();
            if !row.cancelled && row.ends_at <= now.unix_timestamp() {
                return Err(MerchantPoolError::Conflict);
            }
            let mut active = row.into_active_model();
            active.cancelled = Set(true);
            active.updated_at = Set(now);
            let row = active.update(&txn).await.map_err(internal)?;
            audit_control(
                &txn,
                actor,
                "service.maintenance.cancel",
                service.id,
                None,
                Some(serde_json::json!({"notice_id":notice})),
            )
            .await?;
            txn.commit().await.map_err(internal)?;
            Ok(maintenance_snapshot(row, now.unix_timestamp()))
        })
        .await
    }
}

async fn scoped_service<C: ConnectionTrait>(
    connection: &C,
    owner: Option<UserId>,
    id: i64,
) -> Result<services::Model, MerchantPoolError> {
    let service = find_service(connection, id).await?;
    if let Some(owner) = owner {
        let merchant = require_merchant(connection, owner, false).await?;
        if merchant.id != service.merchant_id {
            return Err(MerchantPoolError::NotFound);
        }
    }
    Ok(service)
}

async fn lock_scoped_service(
    txn: &DatabaseTransaction,
    owner: Option<UserId>,
    id: i64,
) -> Result<services::Model, MerchantPoolError> {
    if let Some(owner) = owner {
        lock_owner(txn, owner).await?;
    }
    let initial = scoped_service(txn, owner, id).await?;
    let merchant = profiles::Entity::find_by_id(initial.merchant_id)
        .one(txn)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::NotFound)?;
    lock_owner(txn, UserId::new(merchant.owner_user_id).map_err(internal)?).await?;
    let service = lock_service(txn, id).await?;
    if service.merchant_id != initial.merchant_id {
        return Err(MerchantPoolError::Conflict);
    }
    Ok(service)
}

pub(super) fn price_snapshot(
    row: prices::Model,
) -> Result<MerchantServicePrice, MerchantPoolError> {
    let models: Vec<MerchantModelWrite> = serde_json::from_value(row.models).map_err(internal)?;
    Ok(MerchantServicePrice {
        id: row.id,
        service_id: row.service_id,
        revision_id: row.revision_id,
        effective_at: row.effective_at,
        default_ratio_micros: row.default_ratio_micros,
        model_ratios: models
            .into_iter()
            .map(|model| MerchantModelRatio {
                model: model.model,
                ratio_micros: model.ratio_micros,
            })
            .collect(),
        status: row.status,
        reason: row.reason,
        created_at: row.created_at.unix_timestamp(),
        updated_at: row.updated_at.unix_timestamp(),
    })
}

pub(super) fn price_models(
    row: &prices::Model,
) -> Result<Vec<MerchantModelWrite>, MerchantPoolError> {
    serde_json::from_value(row.models.clone()).map_err(internal)
}

pub(super) async fn effective_price_terms<C: ConnectionTrait>(
    connection: &C,
    revision: &revisions::Model,
) -> Result<(Vec<MerchantModelWrite>, i64), MerchantPoolError> {
    if revision.status == "published" {
        let price = active_price(connection, revision.service_id, Some(revision.id))
            .await?
            .ok_or(MerchantPoolError::Unavailable)?;
        Ok((price_models(&price)?, price.default_ratio_micros))
    } else {
        Ok((revision_models(revision)?, revision.default_ratio_micros))
    }
}

pub(super) async fn active_price<C: ConnectionTrait>(
    connection: &C,
    service: i64,
    revision: Option<i64>,
) -> Result<Option<prices::Model>, MerchantPoolError> {
    let Some(revision) = revision else {
        return Ok(None);
    };
    prices::Entity::find()
        .filter(prices::Column::ServiceId.eq(service))
        .filter(prices::Column::RevisionId.eq(revision))
        .filter(prices::Column::Status.eq("active"))
        .order_by_desc(prices::Column::EffectiveAt)
        .order_by_desc(prices::Column::Id)
        .one(connection)
        .await
        .map_err(internal)
}

/// Called in the approval transaction. Restoring the same revision retains its price.
pub(super) async fn activate_revision_price(
    txn: &DatabaseTransaction,
    revision: &revisions::Model,
    now: DatabaseTimestamp,
) -> Result<(), MerchantPoolError> {
    prices::Entity::update_many()
        .filter(prices::Column::ServiceId.eq(revision.service_id))
        .filter(prices::Column::RevisionId.ne(revision.id))
        .filter(prices::Column::Status.eq("scheduled"))
        .col_expr(prices::Column::Status, Expr::value("invalidated"))
        .col_expr(
            prices::Column::Reason,
            Expr::value("approved_revision_changed"),
        )
        .col_expr(prices::Column::UpdatedAt, Expr::value(now))
        .exec(txn)
        .await
        .map_err(internal)?;
    prices::Entity::update_many()
        .filter(prices::Column::ServiceId.eq(revision.service_id))
        .filter(prices::Column::RevisionId.ne(revision.id))
        .filter(prices::Column::Status.eq("active"))
        .col_expr(prices::Column::Status, Expr::value("superseded"))
        .col_expr(prices::Column::UpdatedAt, Expr::value(now))
        .exec(txn)
        .await
        .map_err(internal)?;
    if active_price(txn, revision.service_id, Some(revision.id))
        .await?
        .is_none()
    {
        prices::ActiveModel {
            service_id: Set(revision.service_id),
            revision_id: Set(revision.id),
            effective_at: Set(0),
            default_ratio_micros: Set(revision.default_ratio_micros),
            models: Set(revision.models.clone()),
            status: Set("active".to_owned()),
            reason: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(txn)
        .await
        .map_err(internal)?;
    }
    Ok(())
}

async fn validate_price<C: ConnectionTrait>(
    connection: &C,
    pool: &pools::Model,
    default: i64,
    models: &[MerchantModelWrite],
) -> Result<(), MerchantPoolError> {
    super::writes::validate_ratios(pool, default, models)?;
    for model in models {
        if super::catalog::model_price(
            connection,
            &model.model,
            model.ratio_micros.unwrap_or(default),
        )
        .await?
        .is_none()
        {
            return Err(MerchantPoolError::InvalidInput);
        }
    }
    Ok(())
}

pub(super) async fn apply_due_in_transaction(
    txn: &DatabaseTransaction,
    service: &services::Model,
    now: DatabaseTimestamp,
) -> Result<usize, MerchantPoolError> {
    let due = prices::Entity::find()
        .filter(prices::Column::ServiceId.eq(service.id))
        .filter(prices::Column::Status.eq("scheduled"))
        .filter(prices::Column::EffectiveAt.lte(now.unix_timestamp()))
        .order_by_asc(prices::Column::EffectiveAt)
        .order_by_asc(prices::Column::Id)
        .all(txn)
        .await
        .map_err(internal)?;
    let count = due.len();
    for row in due {
        let reason = if service.published_revision_id != Some(row.revision_id) {
            Some("approved_revision_changed")
        } else {
            let revision = load_revision(txn, row.revision_id, service.id).await?;
            if revision.status != "published" {
                Some("approved_revision_changed")
            } else {
                let pool = find_pool(txn, service.pool_id).await?;
                let models = price_models(&row)?;
                match validate_price(txn, &pool, row.default_ratio_micros, &models).await {
                    Ok(()) => None,
                    Err(MerchantPoolError::InvalidInput) => Some("price_no_longer_permitted"),
                    Err(error) => return Err(error),
                }
            }
        };
        if reason.is_none() {
            prices::Entity::update_many()
                .filter(prices::Column::ServiceId.eq(service.id))
                .filter(prices::Column::Status.eq("active"))
                .col_expr(prices::Column::Status, Expr::value("superseded"))
                .col_expr(prices::Column::UpdatedAt, Expr::value(now))
                .exec(txn)
                .await
                .map_err(internal)?;
        }
        let mut active = row.into_active_model();
        active.status = Set(if reason.is_some() {
            "invalidated"
        } else {
            "active"
        }
        .to_owned());
        active.reason = Set(reason.map(str::to_owned));
        active.updated_at = Set(now);
        active.update(txn).await.map_err(internal)?;
    }
    Ok(count)
}

fn maintenance_snapshot(row: maintenance::Model, now: i64) -> MerchantMaintenance {
    let status = if row.cancelled {
        "cancelled"
    } else if now >= row.ends_at {
        "completed"
    } else if now < row.starts_at {
        "scheduled"
    } else {
        "active"
    };
    MerchantMaintenance {
        id: row.id,
        service_id: row.service_id,
        starts_at: row.starts_at,
        ends_at: row.ends_at,
        message: row.message,
        blocks_requests: row.blocks_requests,
        status: status.to_owned(),
        created_at: row.created_at.unix_timestamp(),
        updated_at: row.updated_at.unix_timestamp(),
    }
}

pub(super) async fn active_maintenance<C: ConnectionTrait>(
    connection: &C,
    service: i64,
    now: i64,
) -> Result<Vec<MerchantMaintenance>, MerchantPoolError> {
    Ok(maintenance::Entity::find()
        .filter(maintenance::Column::ServiceId.eq(service))
        .filter(maintenance::Column::Cancelled.eq(false))
        .filter(maintenance::Column::StartsAt.lte(now))
        .filter(maintenance::Column::EndsAt.gt(now))
        .order_by_asc(maintenance::Column::StartsAt)
        .order_by_asc(maintenance::Column::Id)
        .all(connection)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|row| maintenance_snapshot(row, now))
        .collect())
}
