use super::*;

pub struct UpstreamUsageStoredSnapshot {
    pub value: Option<Value>,
    pub last_success_at: Option<i64>,
    pub last_checked_at: i64,
    pub failures: i32,
}

pub struct UpstreamUsageLease {
    pub cache_key: String,
    pub identity: String,
    pub owner: String,
}

impl UpstreamUsageRepository {
    pub async fn snapshot(
        &self,
        target: &UpstreamUsageTarget,
    ) -> Result<Option<UpstreamUsageStoredSnapshot>, UpstreamUsageError> {
        self.bounded(async {
            let q = snapshot_query(&target.cache_key);
            let row = self
                .pool
                .connection()
                .query_one(self.pool.connection().get_database_backend().build(&q))
                .await
                .map_err(internal)?;
            let Some(row) = row else { return Ok(None) };
            if row.try_get::<String>("", "identity").map_err(internal)? != target.query_identity {
                return Ok(None);
            }
            let encoded = row
                .try_get::<Option<String>>("", "snapshot_json")
                .map_err(internal)?;
            Ok(Some(UpstreamUsageStoredSnapshot {
                value: encoded
                    .map(|v| serde_json::from_str(&v).map_err(internal))
                    .transpose()?,
                last_success_at: row.try_get("", "last_success_at").map_err(internal)?,
                last_checked_at: row.try_get("", "last_checked_at").map_err(internal)?,
                failures: row.try_get("", "failures").map_err(internal)?,
            }))
        })
        .await
    }

    /// A database lease coalesces manual and periodic queries across all instances.
    pub async fn claim(
        &self,
        target: &UpstreamUsageTarget,
        owner: String,
        now: i64,
    ) -> Result<Option<UpstreamUsageLease>, UpstreamUsageError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let insert = Query::insert()
                .into_table(Alias::new("upstream_usage_snapshots"))
                .columns(["cache_key", "identity"].map(Alias::new))
                .values_panic([
                    target.cache_key.clone().into(),
                    target.query_identity.clone().into(),
                ])
                .on_conflict(
                    OnConflict::column(Alias::new("cache_key"))
                        .do_nothing_on([Alias::new("cache_key")])
                        .to_owned(),
                )
                .to_owned();
            txn.execute(txn.get_database_backend().build(&insert))
                .await
                .map_err(internal)?;
            let lock = Query::update()
                .table(Alias::new("upstream_usage_snapshots"))
                .value(Alias::new("cache_key"), target.cache_key.clone())
                .and_where(Expr::col(Alias::new("cache_key")).eq(&target.cache_key))
                .to_owned();
            txn.execute(txn.get_database_backend().build(&lock))
                .await
                .map_err(internal)?;
            let row = txn
                .query_one(
                    txn.get_database_backend()
                        .build(&snapshot_query(&target.cache_key)),
                )
                .await
                .map_err(internal)?
                .ok_or(UpstreamUsageError::Unavailable)?;
            let same_identity =
                row.try_get::<String>("", "identity").map_err(internal)? == target.query_identity;
            if same_identity
                && (row.try_get::<i64>("", "lease_until").map_err(internal)? > now
                    || row
                        .try_get::<i64>("", "next_refresh_at")
                        .map_err(internal)?
                        > now)
            {
                return Ok(None);
            }
            let mut update = Query::update();
            update
                .table(Alias::new("upstream_usage_snapshots"))
                .value(Alias::new("identity"), target.query_identity.clone())
                .value(Alias::new("lease_owner"), owner.clone())
                .value(Alias::new("lease_until"), now + 60)
                .and_where(Expr::col(Alias::new("cache_key")).eq(&target.cache_key));
            if !same_identity {
                update
                    .value(Alias::new("snapshot_json"), Option::<String>::None)
                    .value(Alias::new("last_success_at"), Option::<i64>::None)
                    .value(Alias::new("last_checked_at"), 0_i64)
                    .value(Alias::new("failures"), 0_i32);
            }
            txn.execute(txn.get_database_backend().build(&update))
                .await
                .map_err(internal)?;
            txn.commit().await.map_err(internal)?;
            Ok(Some(UpstreamUsageLease {
                cache_key: target.cache_key.clone(),
                identity: target.query_identity.clone(),
                owner,
            }))
        })
        .await
    }

    /// Commit only after checking both the observed target and shared query revision.
    #[allow(
        clippy::too_many_arguments,
        reason = "identity, outcome and lease commit atomically"
    )]
    pub async fn complete(
        &self,
        target: &UpstreamUsageTarget,
        lease: UpstreamUsageLease,
        value: Value,
        success: bool,
        failures: i32,
        now: i64,
        exhausted: bool,
        alert: Option<&str>,
    ) -> Result<bool, UpstreamUsageError> {
        self.bounded(async {
            let txn =
                if self.pool.connection().get_database_backend() == sea_orm::DbBackend::Sqlite {
                    self.pool.connection().begin().await
                } else {
                    self.pool
                        .connection()
                        .begin_with_config(Some(sea_orm::IsolationLevel::ReadCommitted), None)
                        .await
                }
                .map_err(internal)?;
            let mut locks = vec![target.credential_id.get(), target.secret_owner_id.get()];
            locks.sort_unstable();
            locks.dedup();
            for id in locks {
                let lock = Query::update()
                    .table(credentials::Entity)
                    .value(credentials::Column::Id, id)
                    .and_where(Expr::col(credentials::Column::Id).eq(id))
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&lock))
                    .await
                    .map_err(internal)?;
            }
            if let Some(profile) = &target.profile {
                let lock = Query::update()
                    .table(Alias::new("upstream_usage_profiles"))
                    .value(Alias::new("id"), profile.id)
                    .and_where(Expr::col(Alias::new("id")).eq(profile.id))
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&lock))
                    .await
                    .map_err(internal)?;
            }
            if load_target(&txn, target.channel_id, target.credential_id)
                .await?
                .identity
                != target.identity
            {
                return Ok(false);
            }
            let retry = if success {
                30
            } else {
                30_i64
                    .saturating_mul(1_i64 << failures.clamp(0, 7))
                    .min(3600)
            };
            let mut update = Query::update();
            update
                .table(Alias::new("upstream_usage_snapshots"))
                .value(Alias::new("snapshot_json"), value.to_string())
                .value(Alias::new("last_checked_at"), now)
                .value(Alias::new("next_refresh_at"), now + retry)
                .value(Alias::new("failures"), if success { 0 } else { failures })
                .value(Alias::new("lease_until"), 0_i64)
                .value(Alias::new("lease_owner"), Option::<String>::None)
                .and_where(Expr::col(Alias::new("cache_key")).eq(&lease.cache_key))
                .and_where(Expr::col(Alias::new("identity")).eq(&lease.identity))
                .and_where(Expr::col(Alias::new("lease_owner")).eq(&lease.owner));
            if success {
                update.value(Alias::new("last_success_at"), now);
            }
            let changed = txn
                .execute(txn.get_database_backend().build(&update))
                .await
                .map_err(internal)?;
            if changed.rows_affected() != 1 {
                return Ok(false);
            }
            let bindings = usage_bindings(
                &txn,
                target.profile.as_ref().map(|p| p.id),
                Some(target.credential_id),
            )
            .await?;
            for (channel, credential) in bindings {
                let id = credential.get();
                let live = match load_target(
                    &txn,
                    channel,
                    CredentialId::new(id).map_err(internal)?,
                )
                .await
                {
                    Ok(live) if live.query_identity == lease.identity => live,
                    Ok(_)
                    | Err(UpstreamUsageError::NotFound | UpstreamUsageError::InvalidInput) => {
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                let config = &live.config;
                let interval = config.refresh_interval_secs.unwrap_or(900);
                let next = now
                    + if success {
                        interval + (id % 31)
                    } else {
                        retry.max(interval.min(300))
                    };
                let insert = Query::insert()
                    .into_table(Alias::new("credential_usage_pauses"))
                    .columns(["credential_id", "channel_id", "next_refresh_at"].map(Alias::new))
                    .values_panic([id.into(), channel.get().into(), next.into()])
                    .on_conflict(
                        OnConflict::column(Alias::new("credential_id"))
                            .update_column(Alias::new("next_refresh_at"))
                            .to_owned(),
                    )
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&insert))
                    .await
                    .map_err(internal)?;
                // Failures never create a pause. Existing pauses expire without requiring a query.
                if success {
                    let paused = exhausted && config.pause_when_exhausted && live.enabled;
                    let update = Query::update()
                        .table(Alias::new("credential_usage_pauses"))
                        .value(
                            Alias::new("pause_until"),
                            paused.then_some(now + interval.clamp(60, 1800)),
                        )
                        .value(
                            Alias::new("pause_identity"),
                            paused.then_some(live.identity),
                        )
                        .and_where(Expr::col(Alias::new("credential_id")).eq(id))
                        .to_owned();
                    txn.execute(txn.get_database_backend().build(&update))
                        .await
                        .map_err(internal)?;
                    crate::scheduler_outbox::enqueue_scheduler_catalog_change(
                        &txn,
                        crate::SchedulerCatalogSubject::Channel(channel),
                        crate::DatabaseTimestamp::now_utc(),
                    )
                    .await
                    .map_err(internal)?;
                }
            }
            if alert.is_some() {
                let alert_claim = Query::update()
                    .table(Alias::new("upstream_usage_snapshots"))
                    .value(Alias::new("last_alert_at"), now)
                    .and_where(Expr::col(Alias::new("cache_key")).eq(&lease.cache_key))
                    .and_where(Expr::col(Alias::new("last_alert_at")).lte(now - 21_600))
                    .to_owned();
                let claimed = txn
                    .execute(txn.get_database_backend().build(&alert_claim))
                    .await
                    .map_err(internal)?
                    .rows_affected()
                    == 1;
                txn.commit().await.map_err(internal)?;
                return Ok(claimed);
            }
            txn.commit().await.map_err(internal)?;
            Ok(false)
        })
        .await
    }

    pub async fn due(
        &self,
        now: i64,
        limit: u64,
    ) -> Result<Vec<(ChannelId, CredentialId)>, UpstreamUsageError> {
        self.bounded(async {
            let query = Query::select()
                .columns([
                    (Alias::new("c"), Alias::new("channel_id")),
                    (Alias::new("c"), Alias::new("id")),
                ])
                .from_as(credentials::Entity, Alias::new("c"))
                .left_join(
                    Alias::new("credential_usage_pauses"),
                    Expr::col((
                        Alias::new("credential_usage_pauses"),
                        Alias::new("credential_id"),
                    ))
                    .equals((Alias::new("c"), Alias::new("id"))),
                )
                .and_where(Expr::col((Alias::new("c"), Alias::new("deleted_at"))).is_null())
                .and_where(
                    Expr::col(Alias::new("next_refresh_at"))
                        .is_null()
                        .or(Expr::col(Alias::new("next_refresh_at")).lte(now)),
                )
                .order_by(Alias::new("next_refresh_at"), sea_orm::Order::Asc)
                .limit(limit.min(64))
                .to_owned();
            let rows = self
                .pool
                .connection()
                .query_all(self.pool.connection().get_database_backend().build(&query))
                .await
                .map_err(internal)?;
            let mut result = Vec::new();
            for row in rows {
                let id = row.try_get::<i64>("", "id").map_err(internal)?;
                let channel = ChannelId::new(row.try_get("", "channel_id").map_err(internal)?)
                    .map_err(internal)?;
                let credential = CredentialId::new(id).map_err(internal)?;
                let target = self.target(channel, credential).await;
                let interval = target
                    .as_ref()
                    .ok()
                    .and_then(|t| t.config.refresh_interval_secs);
                if interval.is_some() && target.is_ok_and(|t| t.enabled) {
                    result.push((channel, credential));
                }
                // A short claim window avoids one disabled/config-off row starving the queue.
                let update = Query::insert()
                    .into_table(Alias::new("credential_usage_pauses"))
                    .columns(["credential_id", "channel_id", "next_refresh_at"].map(Alias::new))
                    .values_panic([
                        id.into(),
                        channel.get().into(),
                        (now + interval.unwrap_or(3600).max(60)).into(),
                    ])
                    .on_conflict(
                        OnConflict::column(Alias::new("credential_id"))
                            .update_column(Alias::new("next_refresh_at"))
                            .to_owned(),
                    )
                    .to_owned();
                self.pool
                    .connection()
                    .execute(self.pool.connection().get_database_backend().build(&update))
                    .await
                    .map_err(internal)?;
            }
            Ok(result)
        })
        .await
    }
}

fn snapshot_query(key: &str) -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .columns(
            [
                "identity",
                "snapshot_json",
                "last_success_at",
                "last_checked_at",
                "next_refresh_at",
                "lease_until",
                "failures",
            ]
            .map(Alias::new),
        )
        .from(Alias::new("upstream_usage_snapshots"))
        .and_where(Expr::col(Alias::new("cache_key")).eq(key))
        .to_owned()
}
