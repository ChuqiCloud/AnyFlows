//! Retire supply atomically while keeping commercial and audit history intact.
use super::*;
use crate::ability_write::delete_channel_abilities;
use crate::entity::{
    async_task_submission_claims, async_tasks, channel_groups, channel_models,
    merchant_maintenance, merchant_service_prices,
};
use sea_orm::sea_query::{Alias, Query};

impl MerchantPoolRepository {
    pub async fn delete_pool(&self, actor: UserId, id: i64) -> Result<(), MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let now = DatabaseTimestamp::now_utc();
            // This write serializes pool edits, new service creation and billing admission.
            let changed = pools::Entity::update_many()
                .filter(pools::Column::Id.eq(id))
                .filter(pools::Column::DeletedAt.is_null())
                .col_expr(pools::Column::DeletedAt, Expr::value(now))
                .col_expr(pools::Column::Enabled, Expr::value(false))
                .col_expr(pools::Column::UpdatedAt, Expr::value(now))
                .exec(&txn)
                .await
                .map_err(internal)?;
            if changed.rows_affected == 0 {
                if pools::Entity::find_by_id(id)
                    .one(&txn)
                    .await
                    .map_err(internal)?
                    .is_none()
                {
                    return Err(MerchantPoolError::NotFound);
                }
                return Ok(());
            }
            ensure_no_active_billing(&txn, "pool_id", id).await?;
            ensure_no_active_tasks(&txn, Some(id), None).await?;
            let mut selection = services::Entity::find()
                .filter(services::Column::PoolId.eq(id))
                .filter(services::Column::DeletedAt.is_null())
                .order_by_asc(services::Column::Id);
            if txn.get_database_backend() != DbBackend::Sqlite {
                selection = selection.lock(LockType::Update);
            }
            let list = selection.all(&txn).await.map_err(internal)?;
            for service in list {
                retire_service(&txn, service.id, now).await?;
            }
            let route_ids = Query::select()
                .column(Alias::new("route_id"))
                .from(Alias::new("merchant_pool_routes"))
                .and_where(Expr::col(Alias::new("pool_id")).eq(id))
                .to_owned();
            let routes = Query::update()
                .table(Alias::new("user_routes"))
                .value(Alias::new("enabled"), false)
                .value(Alias::new("updated_at"), now)
                .and_where(Expr::col(Alias::new("id")).in_subquery(route_ids))
                .to_owned();
            txn.execute(txn.get_database_backend().build(&routes))
                .await
                .map_err(internal)?;
            audit_control(&txn, actor, "pool.delete", id, None, None).await?;
            txn.commit().await.map_err(internal)?;
            Ok(())
        })
        .await
    }

    pub async fn delete_merchant(&self, actor: UserId, id: i64) -> Result<(), MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let initial = profiles::Entity::find_by_id(id)
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(MerchantPoolError::NotFound)?;
            // Deletion remains available for disabled owners, but shares their write lock.
            users::Entity::update_many()
                .filter(users::Column::Id.eq(initial.owner_user_id))
                .col_expr(users::Column::Id, Expr::col(users::Column::Id).into())
                .exec(&txn)
                .await
                .map_err(internal)?;
            let now = DatabaseTimestamp::now_utc();
            let changed = profiles::Entity::update_many()
                .filter(profiles::Column::Id.eq(id))
                .filter(profiles::Column::DeletedAt.is_null())
                .col_expr(profiles::Column::DeletedAt, Expr::value(now))
                .col_expr(profiles::Column::Status, Expr::value("suspended"))
                .col_expr(profiles::Column::UpdatedAt, Expr::value(now))
                .exec(&txn)
                .await
                .map_err(internal)?;
            if changed.rows_affected == 0 {
                return Ok(());
            }
            // Settlement locks its reservation before the finance account.
            // Keep the same order while the profile lock blocks new admission.
            ensure_no_active_billing(&txn, "merchant_id", id).await?;
            crate::merchant_finance::ensure_account(&txn, id)
                .await
                .map_err(internal)?;
            let balance = crate::merchant_finance::load_account(&txn, id, true)
                .await
                .map_err(internal)?;
            if [
                balance.pending_quota,
                balance.available_quota,
                balance.withdrawing_quota,
                balance.frozen_quota,
                balance.debt_quota,
            ]
            .into_iter()
            .any(|value| value != 0)
            {
                return Err(MerchantPoolError::InUse);
            }
            for (table, states) in [
                ("merchant_withdrawals", vec!["pending"]),
                ("merchant_payouts", vec!["pending", "processing", "unknown"]),
            ] {
                let query = Query::select()
                    .column(Alias::new("merchant_id"))
                    .from(Alias::new(table))
                    .and_where(Expr::col(Alias::new("merchant_id")).eq(id))
                    .and_where(Expr::col(Alias::new("status")).is_in(states))
                    .limit(1)
                    .to_owned();
                if txn
                    .query_one(txn.get_database_backend().build(&query))
                    .await
                    .map_err(internal)?
                    .is_some()
                {
                    return Err(MerchantPoolError::InUse);
                }
            }
            ensure_no_active_tasks(&txn, None, Some(id)).await?;
            let mut selection = services::Entity::find()
                .filter(services::Column::MerchantId.eq(id))
                .filter(services::Column::DeletedAt.is_null())
                .order_by_asc(services::Column::Id);
            if txn.get_database_backend() != DbBackend::Sqlite {
                selection = selection.lock(LockType::Update);
            }
            let list = selection.all(&txn).await.map_err(internal)?;
            for service in list {
                retire_service(&txn, service.id, now).await?;
            }
            audit_control(&txn, actor, "merchant.delete", id, None, None).await?;
            txn.commit().await.map_err(internal)?;
            Ok(())
        })
        .await
    }
}

pub(super) async fn lock_live_pool(
    txn: &DatabaseTransaction,
    id: i64,
) -> Result<pools::Model, MerchantPoolError> {
    pools::Entity::update_many()
        .filter(pools::Column::Id.eq(id))
        .filter(pools::Column::DeletedAt.is_null())
        .col_expr(pools::Column::Id, Expr::col(pools::Column::Id).into())
        .exec(txn)
        .await
        .map_err(internal)?;
    let mut query = pools::Entity::find_by_id(id).filter(pools::Column::DeletedAt.is_null());
    if txn.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(txn)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::NotFound)
}

pub(super) async fn lock_live_merchant(
    txn: &DatabaseTransaction,
    id: i64,
) -> Result<profiles::Model, MerchantPoolError> {
    profiles::Entity::update_many()
        .filter(profiles::Column::Id.eq(id))
        .filter(profiles::Column::DeletedAt.is_null())
        .col_expr(profiles::Column::Id, Expr::col(profiles::Column::Id).into())
        .exec(txn)
        .await
        .map_err(internal)?;
    let mut query = profiles::Entity::find_by_id(id).filter(profiles::Column::DeletedAt.is_null());
    if txn.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(txn)
        .await
        .map_err(internal)?
        .ok_or(MerchantPoolError::NotFound)
}

async fn ensure_no_active_billing(
    txn: &DatabaseTransaction,
    column: &str,
    id: i64,
) -> Result<(), MerchantPoolError> {
    let mut query = Query::select()
        .column(Alias::new("reservation_key"))
        .from(Alias::new("merchant_billing_reservations"))
        .and_where(Expr::col(Alias::new(column)).eq(id))
        .and_where(Expr::col(Alias::new("status")).eq("reserved"))
        .limit(1)
        .to_owned();
    if txn.get_database_backend() != DbBackend::Sqlite {
        query.lock(LockType::Update);
    }
    if txn
        .query_one(txn.get_database_backend().build(&query))
        .await
        .map_err(internal)?
        .is_some()
    {
        return Err(MerchantPoolError::InUse);
    }
    Ok(())
}

async fn ensure_no_active_tasks(
    txn: &DatabaseTransaction,
    pool: Option<i64>,
    merchant: Option<i64>,
) -> Result<(), MerchantPoolError> {
    let mut selection = unresolved_merchant_claims();
    if txn.get_database_backend() != DbBackend::Sqlite {
        selection = selection.lock(LockType::Update);
    }
    let claims = selection.all(txn).await.map_err(internal)?;
    for claim in claims {
        let value = claim
            .merchant_snapshot
            .ok_or(MerchantPoolError::Unavailable)?;
        let terms: af_domain::MerchantBillingSnapshot = serde_json::from_value(
            value
                .get("terms")
                .cloned()
                .ok_or(MerchantPoolError::Unavailable)?,
        )
        .map_err(internal)?;
        if pool.is_some_and(|id| terms.pool_id() != id)
            || merchant.is_some_and(|id| terms.merchant_id() != id)
        {
            continue;
        }
        // A preflight failure can leave an unattempted bound claim. It cannot
        // contact an upstream without a new billing admission, now tombstoned.
        if claim.state == 1 && claim.attempt_key.is_none() {
            continue;
        }
        if claim.state != 3 {
            return Err(MerchantPoolError::InUse);
        }
        // An accepted claim keeps its original provider response; polling updates
        // async_tasks, so its current terminal state takes precedence.
        let task = async_tasks::Entity::find()
            .filter(async_tasks::Column::TaskKey.eq(claim.task_key))
            .one(txn)
            .await
            .map_err(internal)?;
        let terminal = task.map_or_else(
            || claim.status.is_some_and(|status| matches!(status, 4 | 5)),
            |task| task.terminal_at.is_some() && matches!(task.status, 4 | 5),
        );
        if !terminal {
            return Err(MerchantPoolError::InUse);
        }
    }
    let mut services_query = services::Entity::find();
    if let Some(id) = pool {
        services_query = services_query.filter(services::Column::PoolId.eq(id));
    }
    if let Some(id) = merchant {
        services_query = services_query.filter(services::Column::MerchantId.eq(id));
    }
    if txn.get_database_backend() != DbBackend::Sqlite {
        services_query = services_query.lock(LockType::Update);
    }
    let service_ids: Vec<i64> = services_query
        .select_only()
        .column(services::Column::Id)
        .into_tuple()
        .all(txn)
        .await
        .map_err(internal)?;
    let mut revisions_query =
        revisions::Entity::find().filter(revisions::Column::ServiceId.is_in(service_ids));
    if txn.get_database_backend() != DbBackend::Sqlite {
        revisions_query = revisions_query.lock(LockType::Update);
    }
    let channels: Vec<i64> = revisions_query
        .select_only()
        .column(revisions::Column::ChannelId)
        .into_tuple()
        .all(txn)
        .await
        .map_err(internal)?;
    let tasks_query = async_tasks::Entity::find()
        .filter(async_tasks::Column::ChannelId.is_in(channels))
        .filter(async_tasks::Column::TerminalAt.is_null());
    if tasks_query.one(txn).await.map_err(internal)?.is_some() {
        return Err(MerchantPoolError::InUse);
    }
    Ok(())
}

pub(super) fn unresolved_merchant_claims() -> sea_orm::Select<async_task_submission_claims::Entity>
{
    let matching_task = Query::select()
        .expr(Expr::val(1_i16))
        .from(async_tasks::Entity)
        .and_where(
            Expr::col((async_tasks::Entity, async_tasks::Column::TaskKey)).equals((
                async_task_submission_claims::Entity,
                async_task_submission_claims::Column::TaskKey,
            )),
        )
        .to_owned();
    let terminal_task = matching_task
        .clone()
        .and_where(Expr::col((async_tasks::Entity, async_tasks::Column::TerminalAt)).is_not_null())
        .and_where(
            Expr::col((async_tasks::Entity, async_tasks::Column::Status)).is_in([4_i16, 5_i16]),
        )
        .to_owned();
    // Keep the locking query on claims only. Locking a joined task would invert
    // the task settlement order while merchant deletion holds the finance account.
    async_task_submission_claims::Entity::find()
        .filter(async_task_submission_claims::Column::MerchantSnapshot.is_not_null())
        .filter(
            async_task_submission_claims::Column::State
                .ne(1_i16)
                .or(async_task_submission_claims::Column::AttemptKey.is_not_null()),
        )
        .filter(
            async_task_submission_claims::Column::State
                .ne(3_i16)
                .or(Expr::exists(terminal_task).not().and(
                    Expr::exists(matching_task).or(
                        async_task_submission_claims::Column::Status
                            .is_null()
                            .or(async_task_submission_claims::Column::Status
                                .is_not_in([4_i16, 5_i16])),
                    ),
                )),
        )
}

async fn retire_service(
    txn: &DatabaseTransaction,
    id: i64,
    now: DatabaseTimestamp,
) -> Result<(), MerchantPoolError> {
    let mut service = lock_service(txn, id).await?.into_active_model();
    service.deleted_at = Set(Some(now));
    service.status = Set("suspended".to_owned());
    service.platform_suspended = Set(true);
    service.updated_at = Set(now);
    service.update(txn).await.map_err(internal)?;
    let mut history = revisions::Entity::find()
        .filter(revisions::Column::ServiceId.eq(id))
        .order_by_asc(revisions::Column::Id);
    if txn.get_database_backend() != DbBackend::Sqlite {
        history = history.lock(LockType::Update);
    }
    let history = history.all(txn).await.map_err(internal)?;
    for revision in history {
        let channel = ChannelId::new(revision.channel_id).map_err(internal)?;
        channels::Entity::update_many()
            .filter(channels::Column::Id.eq(channel.get()))
            .col_expr(
                channels::Column::Status,
                Expr::value(Status::Disabled.code()),
            )
            .col_expr(channels::Column::DeletedAt, Expr::value(now))
            .col_expr(channels::Column::UpdatedAt, Expr::value(now))
            .exec(txn)
            .await
            .map_err(internal)?;
        credentials::Entity::update_many()
            .filter(credentials::Column::ChannelId.eq(channel.get()))
            .col_expr(
                credentials::Column::Status,
                Expr::value(Status::Disabled.code()),
            )
            .col_expr(credentials::Column::Schedulable, Expr::value(false))
            .col_expr(credentials::Column::DeletedAt, Expr::value(now))
            .col_expr(credentials::Column::UpdatedAt, Expr::value(now))
            .exec(txn)
            .await
            .map_err(internal)?;
        delete_channel_abilities(txn, channel, now)
            .await
            .map_err(internal)?;
        channel_groups::Entity::delete_many()
            .filter(channel_groups::Column::ChannelId.eq(channel.get()))
            .exec(txn)
            .await
            .map_err(internal)?;
        channel_models::Entity::delete_many()
            .filter(channel_models::Column::ChannelId.eq(channel.get()))
            .exec(txn)
            .await
            .map_err(internal)?;
    }
    merchant_service_prices::Entity::update_many()
        .filter(merchant_service_prices::Column::ServiceId.eq(id))
        .filter(merchant_service_prices::Column::Status.eq("scheduled"))
        .col_expr(
            merchant_service_prices::Column::Status,
            Expr::value("cancelled"),
        )
        .col_expr(
            merchant_service_prices::Column::Reason,
            Expr::value("service_deleted"),
        )
        .col_expr(merchant_service_prices::Column::UpdatedAt, Expr::value(now))
        .exec(txn)
        .await
        .map_err(internal)?;
    merchant_maintenance::Entity::update_many()
        .filter(merchant_maintenance::Column::ServiceId.eq(id))
        .col_expr(merchant_maintenance::Column::Cancelled, Expr::value(true))
        .col_expr(merchant_maintenance::Column::UpdatedAt, Expr::value(now))
        .exec(txn)
        .await
        .map_err(internal)?;
    Ok(())
}
