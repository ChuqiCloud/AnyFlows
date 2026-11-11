//! Strictly bounded opt-in active channel probe monitoring persistence.
use std::time::Duration;

use af_domain::{ChannelId, CredentialId};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
    entity::prelude::*,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{
        channel_models, channel_monitor_events as events, channel_monitor_settings as settings,
        channel_monitor_targets as targets, channels, credentials,
    },
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const MIN_INTERVAL_SECS: u32 = 300;
const MAX_INTERVAL_SECS: u32 = 86_400;
const MAX_TIMEOUT_SECS: u32 = 15;
const MIN_ALERT_WINDOW_SECS: u32 = 60;
const MAX_ALERT_WINDOW_SECS: u32 = 3_600;
const DAILY_WINDOW_SECS: u64 = 86_400;
pub const MAX_ACTIVE_CHANNEL_MONITOR_BATCH: usize = 2;
pub const MAX_ACTIVE_CHANNEL_MONITOR_BUDGET: u32 = 288;
pub const MAX_ACTIVE_CHANNEL_MONITOR_EVENTS_PER_TARGET: usize = 100;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ActiveChannelMonitorConfigError {
    #[error("invalid monitoring interval")]
    Interval,
    #[error("invalid daily probe budget")]
    Budget,
    #[error("invalid probe timeout")]
    Timeout,
    #[error("invalid alert duration")]
    AlertWindow,
    #[error("invalid target")]
    Target,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ActiveChannelMonitorError {
    #[error("database query failed")]
    Query,
    #[error("database operation timed out")]
    Timeout,
    #[error("persisted monitor state invariant failed")]
    Invariant,
    #[error("target is not eligible")]
    NotEligible,
    #[error("configuration is invalid")]
    InvalidConfiguration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveChannelMonitorConfig {
    pub enabled: bool,
    pub global_daily_budget: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveChannelMonitorTargetInput {
    pub channel_id: ChannelId,
    pub credential_id: CredentialId,
    pub model: String,
    pub enabled: bool,
    pub interval_secs: u32,
    pub daily_budget: u32,
    pub timeout_secs: u32,
    pub alert_after_secs: u32,
    pub muted_until_epoch_seconds: Option<i64>,
}

impl ActiveChannelMonitorTargetInput {
    pub fn validate(&self) -> Result<(), ActiveChannelMonitorConfigError> {
        if self.model.trim() != self.model
            || self.model.is_empty()
            || self.model.len() > 255
            || self.model.chars().any(char::is_control)
        {
            return Err(ActiveChannelMonitorConfigError::Target);
        }
        if !(MIN_INTERVAL_SECS..=MAX_INTERVAL_SECS).contains(&self.interval_secs) {
            return Err(ActiveChannelMonitorConfigError::Interval);
        }
        if !(1..=MAX_ACTIVE_CHANNEL_MONITOR_BUDGET).contains(&self.daily_budget) {
            return Err(ActiveChannelMonitorConfigError::Budget);
        }
        if !(1..=MAX_TIMEOUT_SECS).contains(&self.timeout_secs) {
            return Err(ActiveChannelMonitorConfigError::Timeout);
        }
        if !(MIN_ALERT_WINDOW_SECS..=MAX_ALERT_WINDOW_SECS).contains(&self.alert_after_secs) {
            return Err(ActiveChannelMonitorConfigError::AlertWindow);
        }
        if self
            .muted_until_epoch_seconds
            .is_some_and(|value| value <= 0)
        {
            return Err(ActiveChannelMonitorConfigError::AlertWindow);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveChannelMonitorTarget {
    pub id: i64,
    pub channel_id: ChannelId,
    pub credential_id: CredentialId,
    pub model: String,
    pub enabled: bool,
    pub interval_secs: u32,
    pub daily_budget: u32,
    pub timeout_secs: u32,
    pub alert_after_secs: u32,
    pub daily_used: u32,
    pub last_attempt_epoch_seconds: Option<i64>,
    pub last_success_epoch_seconds: Option<i64>,
    pub failure_since_epoch_seconds: Option<i64>,
    pub alert_active: bool,
    pub muted_until_epoch_seconds: Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveChannelMonitorEventKind {
    Down,
    Recovered,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveChannelMonitorEvent {
    pub id: i64,
    pub target_id: i64,
    pub kind: ActiveChannelMonitorEventKind,
    pub latency_ms: Option<i64>,
    pub occurred_epoch_seconds: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveChannelProbeLease {
    pub target: ActiveChannelMonitorTarget,
    pub lease_token: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveChannelProbeOutcome {
    Healthy { latency_ms: u64 },
    Unhealthy,
    TimedOut,
}

#[derive(Clone)]
pub struct ActiveChannelMonitorRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl ActiveChannelMonitorRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_TIMEOUT,
        }
    }

    pub async fn get_config(
        &self,
    ) -> Result<ActiveChannelMonitorConfig, ActiveChannelMonitorError> {
        self.bounded(async {
            let setting = settings::Entity::find_by_id(1)
                .one(self.pool.connection())
                .await
                .map_err(map_db)?
                .ok_or(ActiveChannelMonitorError::Invariant)?;
            Ok(ActiveChannelMonitorConfig {
                enabled: setting.enabled,
                global_daily_budget: u32::try_from(setting.global_daily_budget)
                    .map_err(|_| ActiveChannelMonitorError::Invariant)?,
            })
        })
        .await
    }

    pub async fn set_enabled(
        &self,
        enabled: bool,
    ) -> Result<ActiveChannelMonitorConfig, ActiveChannelMonitorError> {
        self.bounded(async {
            let transaction = self.pool.connection().begin().await.map_err(map_db)?;
            let current = lock_settings(&transaction).await?;
            let updated = settings::Entity::update_many()
                .col_expr(settings::Column::Enabled, Expr::value(enabled))
                .col_expr(
                    settings::Column::UpdatedAt,
                    Expr::value(TimeDateTimeWithTimeZone::now_utc()),
                )
                .filter(settings::Column::Id.eq(1))
                .exec(&transaction)
                .await
                .map_err(map_db)?;
            if updated.rows_affected != 1 {
                let _ = transaction.rollback().await;
                return Err(ActiveChannelMonitorError::Invariant);
            }
            transaction.commit().await.map_err(map_db)?;
            Ok(ActiveChannelMonitorConfig {
                enabled,
                global_daily_budget: u32::try_from(current.global_daily_budget)
                    .map_err(|_| ActiveChannelMonitorError::Invariant)?,
            })
        })
        .await
    }

    pub async fn set_config(
        &self,
        config: ActiveChannelMonitorConfig,
    ) -> Result<ActiveChannelMonitorConfig, ActiveChannelMonitorError> {
        if config.global_daily_budget > MAX_ACTIVE_CHANNEL_MONITOR_BUDGET {
            return Err(ActiveChannelMonitorError::InvalidConfiguration);
        }
        self.bounded(async {
            let transaction = self.pool.connection().begin().await.map_err(map_db)?;
            let _ = lock_settings(&transaction).await?;
            let updated = settings::Entity::update_many()
                .col_expr(settings::Column::Enabled, Expr::value(config.enabled))
                .col_expr(
                    settings::Column::GlobalDailyBudget,
                    Expr::value(config.global_daily_budget as i32),
                )
                .col_expr(
                    settings::Column::UpdatedAt,
                    Expr::value(TimeDateTimeWithTimeZone::now_utc()),
                )
                .filter(settings::Column::Id.eq(1))
                .exec(&transaction)
                .await
                .map_err(map_db)?;
            if updated.rows_affected != 1 {
                let _ = transaction.rollback().await;
                return Err(ActiveChannelMonitorError::Invariant);
            }
            transaction.commit().await.map_err(map_db)?;
            Ok(config)
        })
        .await
    }

    pub async fn list_targets(
        &self,
        limit: usize,
    ) -> Result<Vec<ActiveChannelMonitorTarget>, ActiveChannelMonitorError> {
        if limit == 0 || limit > 256 {
            return Err(ActiveChannelMonitorError::InvalidConfiguration);
        }
        self.bounded(async {
            let rows = targets::Entity::find()
                .filter(targets::Column::DeleteAfterLease.eq(false))
                .order_by_asc(targets::Column::Id)
                .limit(limit as u64)
                .all(self.pool.connection())
                .await
                .map_err(map_db)?;
            rows.into_iter().map(convert_target).collect()
        })
        .await
    }

    pub async fn upsert_target(
        &self,
        input: ActiveChannelMonitorTargetInput,
    ) -> Result<ActiveChannelMonitorTarget, ActiveChannelMonitorError> {
        input
            .validate()
            .map_err(|_| ActiveChannelMonitorError::InvalidConfiguration)?;
        self.bounded(async {
            validate_target_scope(
                self.pool.connection(),
                input.channel_id,
                input.credential_id,
                &input.model,
            )
            .await?;
            let existing = targets::Entity::find()
                .filter(targets::Column::ChannelId.eq(input.channel_id.get()))
                .filter(targets::Column::CredentialId.eq(input.credential_id.get()))
                .filter(targets::Column::Model.eq(&input.model))
                .one(self.pool.connection())
                .await
                .map_err(map_db)?;
            let now = TimeDateTimeWithTimeZone::now_utc();
            let muted_until = input
                .muted_until_epoch_seconds
                .map(from_epoch)
                .transpose()?;
            let row = if let Some(existing) = existing {
                let mut active: targets::ActiveModel = existing.into();
                active.enabled = Set(input.enabled);
                active.interval_secs = Set(input.interval_secs as i32);
                active.daily_budget = Set(input.daily_budget as i32);
                active.timeout_secs = Set(input.timeout_secs as i32);
                active.alert_after_secs = Set(input.alert_after_secs as i32);
                active.muted_until = Set(muted_until);
                active.delete_after_lease = Set(false);
                active.updated_at = Set(now);
                active
                    .update(self.pool.connection())
                    .await
                    .map_err(map_db)?
            } else {
                if targets::Entity::find()
                    .count(self.pool.connection())
                    .await
                    .map_err(map_db)?
                    >= 256
                {
                    return Err(ActiveChannelMonitorError::InvalidConfiguration);
                }
                targets::ActiveModel {
                    channel_id: Set(input.channel_id.get()),
                    credential_id: Set(input.credential_id.get()),
                    model: Set(input.model),
                    enabled: Set(input.enabled),
                    interval_secs: Set(input.interval_secs as i32),
                    daily_budget: Set(input.daily_budget as i32),
                    timeout_secs: Set(input.timeout_secs as i32),
                    alert_after_secs: Set(input.alert_after_secs as i32),
                    daily_used: Set(0),
                    budget_window_started_at: Set(None),
                    last_attempt_at: Set(None),
                    last_success_at: Set(None),
                    failure_since: Set(None),
                    alert_active: Set(false),
                    muted_until: Set(muted_until),
                    delete_after_lease: Set(false),
                    lease_token: Set(None),
                    lease_until: Set(None),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(self.pool.connection())
                .await
                .map_err(map_db)?
            };
            convert_target(row)
        })
        .await
    }

    pub async fn delete_target(&self, target_id: i64) -> Result<bool, ActiveChannelMonitorError> {
        self.bounded(async {
            let transaction = self.pool.connection().begin().await.map_err(map_db)?;
            let now = TimeDateTimeWithTimeZone::now_utc();
            let Some(row) = targets::Entity::find_by_id(target_id)
                .one(&transaction)
                .await
                .map_err(map_db)?
            else {
                transaction.rollback().await.map_err(map_db)?;
                return Ok(false);
            };
            if row.lease_until.is_some_and(|until| until > now) {
                let result = targets::Entity::update_many()
                    .col_expr(targets::Column::Enabled, Expr::value(false))
                    .col_expr(targets::Column::DeleteAfterLease, Expr::value(true))
                    .col_expr(targets::Column::UpdatedAt, Expr::value(now))
                    .filter(targets::Column::Id.eq(target_id))
                    .exec(&transaction)
                    .await
                    .map_err(map_db)?;
                transaction.commit().await.map_err(map_db)?;
                return Ok(result.rows_affected == 1);
            }
            let result = targets::Entity::delete_many()
                .filter(targets::Column::Id.eq(target_id))
                .exec(&transaction)
                .await
                .map_err(map_db)?;
            transaction.commit().await.map_err(map_db)?;
            Ok(result.rows_affected > 0)
        })
        .await
    }

    /// Atomic claim enforces cross-node lease, minimum interval and rolling daily budget before any upstream call.
    pub async fn claim_due_targets(
        &self,
        lease_token: &str,
        limit: usize,
        now_epoch_seconds: i64,
    ) -> Result<Vec<ActiveChannelProbeLease>, ActiveChannelMonitorError> {
        if lease_token.is_empty()
            || lease_token.len() > 64
            || limit == 0
            || limit > MAX_ACTIVE_CHANNEL_MONITOR_BATCH
        {
            return Err(ActiveChannelMonitorError::InvalidConfiguration);
        }
        self.bounded(async {
            let now = from_epoch(now_epoch_seconds)?;
            let transaction = self.pool.connection().begin().await.map_err(map_db)?;
            let setting = lock_settings(&transaction).await?;
            if !setting.enabled {
                transaction.rollback().await.map_err(map_db)?;
                return Ok(Vec::new());
            }
            let (global_used, global_window) =
                if setting.global_budget_window_started_at.is_none_or(|start| {
                    start
                        .unix_timestamp()
                        .saturating_add(DAILY_WINDOW_SECS as i64)
                        <= now.unix_timestamp()
                }) {
                    (0, Some(now))
                } else {
                    (
                        u32::try_from(setting.global_daily_used)
                            .map_err(|_| ActiveChannelMonitorError::Invariant)?,
                        setting.global_budget_window_started_at,
                    )
                };
            let global_budget = u32::try_from(setting.global_daily_budget)
                .map_err(|_| ActiveChannelMonitorError::Invariant)?;
            if global_budget > MAX_ACTIVE_CHANNEL_MONITOR_BUDGET {
                let _ = transaction.rollback().await;
                return Err(ActiveChannelMonitorError::Invariant);
            }
            if global_used >= global_budget {
                transaction.rollback().await.map_err(map_db)?;
                return Ok(Vec::new());
            }
            let in_flight = targets::Entity::find()
                .filter(targets::Column::LeaseUntil.gt(now))
                .count(&transaction)
                .await
                .map_err(map_db)?;
            let available = MAX_ACTIVE_CHANNEL_MONITOR_BATCH
                .saturating_sub(usize::try_from(in_flight).unwrap_or(usize::MAX));
            if available == 0 {
                transaction.rollback().await.map_err(map_db)?;
                return Ok(Vec::new());
            }
            let rows = targets::Entity::find()
                .filter(targets::Column::Enabled.eq(true))
                .filter(targets::Column::DeleteAfterLease.eq(false))
                .order_by_asc(targets::Column::Id)
                .limit(256)
                .all(&transaction)
                .await
                .map_err(map_db)?;
            let mut claimed = Vec::new();
            let lease_until = add_seconds(now, MAX_TIMEOUT_SECS + 30)?;
            for row in rows {
                if claimed.len() >= limit.min(available)
                    || global_used + claimed.len() as u32 >= global_budget
                {
                    break;
                }
                if row.lease_until.is_some_and(|until| until > now) {
                    continue;
                }
                let (daily_used, window_start) =
                    if row.budget_window_started_at.is_none_or(|start| {
                        start
                            .unix_timestamp()
                            .saturating_add(DAILY_WINDOW_SECS as i64)
                            <= now.unix_timestamp()
                    }) {
                        (0, Some(now))
                    } else {
                        (
                            u32::try_from(row.daily_used)
                                .map_err(|_| ActiveChannelMonitorError::Invariant)?,
                            row.budget_window_started_at,
                        )
                    };
                if daily_used
                    >= u32::try_from(row.daily_budget)
                        .map_err(|_| ActiveChannelMonitorError::Invariant)?
                {
                    continue;
                }
                if row.last_attempt_at.is_some_and(|last| {
                    last.unix_timestamp()
                        .saturating_add(i64::from(row.interval_secs))
                        > now.unix_timestamp()
                }) {
                    continue;
                }
                let target = convert_target(row.clone())?;
                match validate_target_scope(
                    &transaction,
                    target.channel_id,
                    target.credential_id,
                    &target.model,
                )
                .await
                {
                    Ok(()) => {}
                    Err(ActiveChannelMonitorError::NotEligible) => continue,
                    Err(error) => return Err(error),
                }
                let result = targets::Entity::update_many()
                    .col_expr(
                        targets::Column::DailyUsed,
                        Expr::value((daily_used + 1) as i32),
                    )
                    .col_expr(
                        targets::Column::BudgetWindowStartedAt,
                        Expr::value(window_start),
                    )
                    .col_expr(targets::Column::LastAttemptAt, Expr::value(Some(now)))
                    .col_expr(
                        targets::Column::LeaseToken,
                        Expr::value(Some(lease_token.to_owned())),
                    )
                    .col_expr(targets::Column::LeaseUntil, Expr::value(Some(lease_until)))
                    .col_expr(targets::Column::UpdatedAt, Expr::value(now))
                    .filter(targets::Column::Id.eq(row.id))
                    .filter(targets::Column::Enabled.eq(true))
                    .filter(
                        targets::Column::LeaseUntil
                            .is_null()
                            .or(targets::Column::LeaseUntil.lte(now)),
                    )
                    .exec(&transaction)
                    .await
                    .map_err(map_db)?;
                if result.rows_affected != 1 {
                    continue;
                }
                let refreshed = targets::Entity::find_by_id(row.id)
                    .one(&transaction)
                    .await
                    .map_err(map_db)?
                    .ok_or(ActiveChannelMonitorError::Invariant)?;
                let target = convert_target(refreshed)?;
                claimed.push(ActiveChannelProbeLease {
                    target,
                    lease_token: lease_token.to_owned(),
                });
            }
            if !claimed.is_empty() {
                let used = global_used + claimed.len() as u32;
                let changed = settings::Entity::update_many()
                    .col_expr(settings::Column::GlobalDailyUsed, Expr::value(used as i32))
                    .col_expr(
                        settings::Column::GlobalBudgetWindowStartedAt,
                        Expr::value(global_window),
                    )
                    .col_expr(settings::Column::UpdatedAt, Expr::value(now))
                    .filter(settings::Column::Id.eq(1))
                    .filter(settings::Column::Enabled.eq(true))
                    .exec(&transaction)
                    .await
                    .map_err(map_db)?;
                if changed.rows_affected != 1 {
                    let _ = transaction.rollback().await;
                    return Err(ActiveChannelMonitorError::NotEligible);
                }
            }
            transaction.commit().await.map_err(map_db)?;
            Ok(claimed)
        })
        .await
    }

    pub async fn complete_probe(
        &self,
        lease: ActiveChannelProbeLease,
        outcome: ActiveChannelProbeOutcome,
        now_epoch_seconds: i64,
    ) -> Result<(), ActiveChannelMonitorError> {
        self.bounded(async {
            let now = from_epoch(now_epoch_seconds)?;
            let transaction = self.pool.connection().begin().await.map_err(map_db)?;
            let _ = lock_settings(&transaction).await?;
            let row = targets::Entity::find_by_id(lease.target.id)
                .one(&transaction)
                .await
                .map_err(map_db)?
                .ok_or(ActiveChannelMonitorError::NotEligible)?;
            if row.lease_token.as_deref() != Some(lease.lease_token.as_str()) {
                let _ = transaction.rollback().await;
                return Err(ActiveChannelMonitorError::NotEligible);
            }
            let mut active: targets::ActiveModel = row.clone().into();
            active.lease_token = Set(None);
            active.lease_until = Set(None);
            active.updated_at = Set(now);
            match outcome {
                ActiveChannelProbeOutcome::Healthy { latency_ms } => {
                    active.last_success_at = Set(Some(now));
                    active.failure_since = Set(None);
                    if row.alert_active {
                        active.alert_active = Set(false);
                        let event_id =
                            insert_event(&transaction, row.id, "recovered", Some(latency_ms), now)
                                .await?;
                        insert_admin_monitor_notifications(
                            &transaction,
                            row.id,
                            event_id,
                            "recovered",
                            now,
                        )
                        .await?;
                    }
                }
                ActiveChannelProbeOutcome::Unhealthy | ActiveChannelProbeOutcome::TimedOut => {
                    let failure_since = row.failure_since.unwrap_or(now);
                    active.failure_since = Set(Some(failure_since));
                    if !row.alert_active
                        && failure_since
                            .unix_timestamp()
                            .saturating_add(i64::from(row.alert_after_secs))
                            <= now.unix_timestamp()
                    {
                        active.alert_active = Set(true);
                        let event_id =
                            insert_event(&transaction, row.id, "down", None, now).await?;
                        if !row.muted_until.is_some_and(|until| until > now) {
                            insert_admin_monitor_notifications(
                                &transaction,
                                row.id,
                                event_id,
                                "down",
                                now,
                            )
                            .await?;
                        }
                    }
                }
            }
            active.update(&transaction).await.map_err(map_db)?;
            if row.delete_after_lease {
                targets::Entity::delete_by_id(row.id)
                    .exec(&transaction)
                    .await
                    .map_err(map_db)?;
            }
            transaction.commit().await.map_err(map_db)?;
            Ok(())
        })
        .await
    }

    pub async fn events(
        &self,
        target_id: i64,
        limit: usize,
    ) -> Result<Vec<ActiveChannelMonitorEvent>, ActiveChannelMonitorError> {
        if limit == 0 || limit > 100 {
            return Err(ActiveChannelMonitorError::InvalidConfiguration);
        }
        self.bounded(async {
            if targets::Entity::find_by_id(target_id)
                .one(self.pool.connection())
                .await
                .map_err(map_db)?
                .is_none()
            {
                return Err(ActiveChannelMonitorError::NotEligible);
            }
            events::Entity::find()
                .filter(events::Column::TargetId.eq(target_id))
                .order_by_desc(events::Column::OccurredAt)
                .order_by_desc(events::Column::Id)
                .limit(limit as u64)
                .all(self.pool.connection())
                .await
                .map_err(map_db)?
                .into_iter()
                .map(|row| {
                    Ok(ActiveChannelMonitorEvent {
                        id: row.id,
                        target_id: row.target_id,
                        kind: match row.event_kind.as_str() {
                            "down" => ActiveChannelMonitorEventKind::Down,
                            "recovered" => ActiveChannelMonitorEventKind::Recovered,
                            _ => return Err(ActiveChannelMonitorError::Invariant),
                        },
                        latency_ms: row.latency_ms,
                        occurred_epoch_seconds: row.occurred_at.unix_timestamp(),
                    })
                })
                .collect()
        })
        .await
    }

    async fn bounded<T, F>(&self, operation: F) -> Result<T, ActiveChannelMonitorError>
    where
        T: Send,
        F: std::future::Future<Output = Result<T, ActiveChannelMonitorError>> + Send,
    {
        match timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(ActiveChannelMonitorError::Timeout),
        }
    }
}

pub(super) async fn insert_event<C: ConnectionTrait>(
    db: &C,
    target_id: i64,
    kind: &str,
    latency: Option<u64>,
    at: TimeDateTimeWithTimeZone,
) -> Result<i64, ActiveChannelMonitorError> {
    let latency = latency.map(|value| i64::try_from(value).unwrap_or(i64::MAX));
    let row = events::ActiveModel {
        target_id: Set(target_id),
        event_kind: Set(kind.to_owned()),
        latency_ms: Set(latency),
        occurred_at: Set(at),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(map_db)?;
    let keep_ids = events::Entity::find()
        .filter(events::Column::TargetId.eq(target_id))
        .order_by_desc(events::Column::OccurredAt)
        .order_by_desc(events::Column::Id)
        .limit(MAX_ACTIVE_CHANNEL_MONITOR_EVENTS_PER_TARGET as u64)
        .all(db)
        .await
        .map_err(map_db)?
        .into_iter()
        .map(|event| event.id)
        .collect::<Vec<_>>();
    events::Entity::delete_many()
        .filter(events::Column::TargetId.eq(target_id))
        .filter(events::Column::Id.is_not_in(keep_ids))
        .exec(db)
        .await
        .map_err(map_db)?;
    Ok(row.id)
}

async fn insert_admin_monitor_notifications<C: ConnectionTrait>(
    db: &C,
    target_id: i64,
    event_id: i64,
    event_kind: &str,
    occurred_at: TimeDateTimeWithTimeZone,
) -> Result<(), ActiveChannelMonitorError> {
    use crate::entity::users;
    let admins = users::Entity::find()
        .filter(users::Column::Role.eq(1_i16))
        .filter(users::Column::Status.eq(af_domain::Status::Enabled.code()))
        .filter(users::Column::DeletedAt.is_null())
        .all(db)
        .await
        .map_err(map_db)?;
    for admin in admins {
        crate::notification::insert_channel_monitor_update(
            db,
            admin.id,
            target_id,
            event_id,
            event_kind,
            occurred_at,
        )
        .await
        .map_err(map_db)?;
    }
    Ok(())
}

async fn validate_target_scope<C: ConnectionTrait>(
    db: &C,
    channel: ChannelId,
    credential: CredentialId,
    model: &str,
) -> Result<(), ActiveChannelMonitorError> {
    let channel_row = channels::Entity::find_by_id(channel.get())
        .one(db)
        .await
        .map_err(map_db)?
        .ok_or(ActiveChannelMonitorError::NotEligible)?;
    if channel_row.status != af_domain::Status::Enabled.code() || channel_row.deleted_at.is_some() {
        return Err(ActiveChannelMonitorError::NotEligible);
    }
    if channel_models::Entity::find_by_id((channel.get(), model.to_owned()))
        .one(db)
        .await
        .map_err(map_db)?
        .is_none()
    {
        return Err(ActiveChannelMonitorError::NotEligible);
    }
    let channel_type = channel_row
        .r#type
        .parse::<af_domain::ChannelType>()
        .map_err(|_| ActiveChannelMonitorError::NotEligible)?;
    let protocol = channel_row
        .protocol
        .parse::<af_domain::Protocol>()
        .map_err(|_| ActiveChannelMonitorError::NotEligible)?;
    if !channel_type.supports_probe(protocol) || !supports_low_cost_probe_protocol(protocol) {
        return Err(ActiveChannelMonitorError::NotEligible);
    }
    let channel_settings = channel_row.settings.clone().into_inner();
    if crate::channel_opencode_full_compatibility(channel_type, &channel_settings)
        .map_err(|_| ActiveChannelMonitorError::NotEligible)?
    {
        return Err(ActiveChannelMonitorError::NotEligible);
    }
    let profiles = crate::client_profiles::runtime_profiles(db, &[channel.get()])
        .await
        .map_err(|_| ActiveChannelMonitorError::NotEligible)?;
    let profile = match profiles.get(&channel.get()) {
        Some(None) => return Err(ActiveChannelMonitorError::NotEligible),
        Some(Some(profile)) => Some(profile),
        None => None,
    };
    if profile.is_some_and(|profile| !profile.definition.matches_channel(channel_type, protocol)) {
        return Err(ActiveChannelMonitorError::NotEligible);
    }
    let compatible = credentials::Entity::find_by_id(credential.get())
        .filter(credentials::Column::ChannelId.eq(channel.get()))
        .filter(credentials::Column::Status.eq(af_domain::Status::Enabled.code()))
        .filter(credentials::Column::Schedulable.eq(true))
        .filter(credentials::Column::OauthTokenPending.eq(false))
        .filter(credentials::Column::DeletedAt.is_null())
        .filter(crate::credential_probe::compatible_probe_credentials(
            channel_type,
            profile,
        ))
        .one(db)
        .await
        .map_err(map_db)?;
    if compatible.is_none() {
        return Err(ActiveChannelMonitorError::NotEligible);
    }
    Ok(())
}

fn supports_low_cost_probe_protocol(protocol: af_domain::Protocol) -> bool {
    matches!(
        protocol,
        af_domain::Protocol::OpenAiChat
            | af_domain::Protocol::OpenAiResponses
            | af_domain::Protocol::OpenAiEmbeddings
            | af_domain::Protocol::Anthropic
            | af_domain::Protocol::Gemini
            | af_domain::Protocol::JinaRerank
            | af_domain::Protocol::CohereRerank
    )
}

fn convert_target(
    row: targets::Model,
) -> Result<ActiveChannelMonitorTarget, ActiveChannelMonitorError> {
    Ok(ActiveChannelMonitorTarget {
        id: row.id,
        channel_id: ChannelId::new(row.channel_id)
            .map_err(|_| ActiveChannelMonitorError::Invariant)?,
        credential_id: CredentialId::new(row.credential_id)
            .map_err(|_| ActiveChannelMonitorError::Invariant)?,
        model: row.model,
        enabled: row.enabled,
        interval_secs: u32::try_from(row.interval_secs)
            .map_err(|_| ActiveChannelMonitorError::Invariant)?,
        daily_budget: u32::try_from(row.daily_budget)
            .map_err(|_| ActiveChannelMonitorError::Invariant)?,
        timeout_secs: u32::try_from(row.timeout_secs)
            .map_err(|_| ActiveChannelMonitorError::Invariant)?,
        alert_after_secs: u32::try_from(row.alert_after_secs)
            .map_err(|_| ActiveChannelMonitorError::Invariant)?,
        daily_used: u32::try_from(row.daily_used)
            .map_err(|_| ActiveChannelMonitorError::Invariant)?,
        last_attempt_epoch_seconds: row.last_attempt_at.map(|value| value.unix_timestamp()),
        last_success_epoch_seconds: row.last_success_at.map(|value| value.unix_timestamp()),
        failure_since_epoch_seconds: row.failure_since.map(|value| value.unix_timestamp()),
        alert_active: row.alert_active,
        muted_until_epoch_seconds: row.muted_until.map(|value| value.unix_timestamp()),
    })
}
fn from_epoch(value: i64) -> Result<TimeDateTimeWithTimeZone, ActiveChannelMonitorError> {
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| ActiveChannelMonitorError::InvalidConfiguration)
}
fn add_seconds(
    at: TimeDateTimeWithTimeZone,
    seconds: u32,
) -> Result<TimeDateTimeWithTimeZone, ActiveChannelMonitorError> {
    TimeDateTimeWithTimeZone::from_unix_timestamp(
        at.unix_timestamp()
            .checked_add(i64::from(seconds))
            .ok_or(ActiveChannelMonitorError::Invariant)?,
    )
    .map_err(|_| ActiveChannelMonitorError::Invariant)
}
fn map_db(_: DbErr) -> ActiveChannelMonitorError {
    ActiveChannelMonitorError::Query
}

async fn lock_settings(
    transaction: &DatabaseTransaction,
) -> Result<settings::Model, ActiveChannelMonitorError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = settings::Entity::update_many()
            .filter(settings::Column::Id.eq(1))
            .col_expr(
                settings::Column::Enabled,
                Expr::col(settings::Column::Enabled).into(),
            )
            .exec(transaction)
            .await
            .map_err(map_db)?;
        if result.rows_affected != 1 {
            return Err(ActiveChannelMonitorError::Invariant);
        }
    }
    let mut query = settings::Entity::find_by_id(1);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(map_db)?
        .ok_or(ActiveChannelMonitorError::Invariant)
}
