//! Pool routes retain ordinary token ownership, but never expose an arbitrary channel selector.
use super::{
    UserRouteError, UserRouteMemberWrite, UserRouteRepository, group_access, internal, lock_owner,
    lock_route, owned_route, sync_members,
};
use crate::{MerchantPoolRepository, entity::user_routes};
use af_domain::{GroupId, MAX_USER_ROUTE_MEMBERS, MAX_USER_ROUTES, UserId, UserRouteStrategy};
use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(super) mod scope {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "merchant_pool_routes")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub route_id: i64,
        pub pool_id: i64,
        #[sea_orm(column_type = "JsonBinary")]
        pub members: Json,
        #[sea_orm(column_type = "JsonBinary")]
        pub enrolled_merchants: Json,
        #[sea_orm(column_type = "JsonBinary")]
        pub excluded_merchant_ids: Json,
        pub allow_pool_fallback: bool,
        pub accept_new_merchants: bool,
        pub updated_at_millis: i64,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

mod attempts {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "merchant_pool_attempts")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub pool_id: i64,
        pub route_id: i64,
        pub request_id: String,
        pub merchant_id: i64,
        pub service_id: i64,
        pub model: String,
        pub protocol: String,
        pub outcome: String,
        pub streaming: bool,
        pub latency_ms: i64,
        pub ttft_ms: Option<i64>,
        pub completed_at_millis: i64,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantRouteMember {
    pub merchant_id: i64,
    pub priority: i32,
    pub weight: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_selected_at_millis: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantRouteWrite {
    pub name: String,
    pub pool_id: i64,
    pub strategy: UserRouteStrategy,
    pub enabled: bool,
    pub max_merchant_attempts: u32,
    pub max_price_ratio_micros: Option<i64>,
    pub members: Vec<MerchantRouteMember>,
    #[serde(default)]
    pub excluded_merchant_ids: Vec<i64>,
    pub allow_pool_fallback: bool,
    pub accept_new_merchants: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MerchantRouteRecord {
    pub id: i64,
    #[serde(flatten)]
    pub configuration: MerchantRouteWrite,
}

#[derive(Clone, Debug)]
pub struct MerchantRouteScope {
    pub pool_id: i64,
    pub members: Vec<MerchantRouteMember>,
    pub enrolled_merchants: Vec<i64>,
    pub excluded_merchant_ids: Vec<i64>,
    pub allow_pool_fallback: bool,
    pub accept_new_merchants: bool,
}

impl MerchantRouteWrite {
    pub fn validate(&self) -> Result<(), UserRouteError> {
        let mut seen = BTreeSet::new();
        if self.pool_id <= 0
            || self.name.is_empty()
            || self.name.len() > 128
            || self.name.trim() != self.name
            || self.name.chars().any(char::is_control)
            || !(1..=16).contains(&self.max_merchant_attempts)
            || self.max_price_ratio_micros.is_some_and(|n| n < 0)
            || self.members.is_empty()
            || self.members.len() > MAX_USER_ROUTE_MEMBERS
            || self.excluded_merchant_ids.len() > 128
            || self.excluded_merchant_ids.iter().any(|id| *id <= 0)
            || self
                .excluded_merchant_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.excluded_merchant_ids.len()
            || self
                .members
                .iter()
                .any(|m| self.excluded_merchant_ids.contains(&m.merchant_id))
            || (self.accept_new_merchants && !self.allow_pool_fallback)
            || self.members.iter().any(|m| {
                m.merchant_id <= 0
                    || m.priority < 0
                    || m.weight == 0
                    || m.weight > 10_000
                    || m.last_selected_at_millis.is_some()
                    || !seen.insert(m.merchant_id)
            })
        {
            return Err(UserRouteError::InvalidInput);
        }
        Ok(())
    }
}

pub(super) async fn route_scope<C: ConnectionTrait>(
    connection: &C,
    route_id: i64,
) -> Result<Option<MerchantRouteScope>, UserRouteError> {
    scope::Entity::find_by_id(route_id)
        .one(connection)
        .await
        .map_err(internal)?
        .map(|row| {
            Ok(MerchantRouteScope {
                pool_id: row.pool_id,
                members: serde_json::from_value(row.members)
                    .map_err(|_| UserRouteError::Internal)?,
                enrolled_merchants: serde_json::from_value(row.enrolled_merchants)
                    .map_err(|_| UserRouteError::Internal)?,
                excluded_merchant_ids: serde_json::from_value(row.excluded_merchant_ids)
                    .map_err(|_| UserRouteError::Internal)?,
                allow_pool_fallback: row.allow_pool_fallback,
                accept_new_merchants: row.accept_new_merchants,
            })
        })
        .transpose()
}

async fn enrolled<C: ConnectionTrait>(
    connection: &C,
    pool_id: i64,
) -> Result<Vec<i64>, UserRouteError> {
    let query = Query::select()
        .distinct()
        .column(Alias::new("merchant_id"))
        .from(Alias::new("merchant_services"))
        .and_where(Expr::col(Alias::new("pool_id")).eq(pool_id))
        .and_where(Expr::col(Alias::new("published_revision_id")).is_not_null())
        .and_where(Expr::col(Alias::new("status")).eq("published"))
        .order_by(Alias::new("merchant_id"), sea_orm::sea_query::Order::Asc)
        .to_owned();
    connection
        .query_all(connection.get_database_backend().build(&query))
        .await
        .map_err(internal)?
        .into_iter()
        .map(|r| r.try_get("", "merchant_id").map_err(internal))
        .collect()
}

impl UserRouteRepository {
    pub async fn merchant_scope(
        &self,
        route_id: i64,
    ) -> Result<Option<MerchantRouteScope>, UserRouteError> {
        self.bounded(route_scope(self.pool.connection(), route_id))
            .await
    }

    pub async fn merchant_routes(
        &self,
        owner: UserId,
    ) -> Result<Vec<MerchantRouteRecord>, UserRouteError> {
        self.bounded(async {
            super::ensure_owner(self.pool.connection(), owner).await?;
            let rows = user_routes::Entity::find()
                .filter(user_routes::Column::OwnerUserId.eq(owner.get()))
                .filter(user_routes::Column::DeletedAt.is_null())
                .order_by_asc(user_routes::Column::Id)
                .limit(MAX_USER_ROUTES as u64)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            let mut result = Vec::new();
            for row in rows {
                if let Some(scope) = route_scope(self.pool.connection(), row.id).await? {
                    result.push(route_record(row, scope)?);
                }
            }
            Ok(result)
        })
        .await
    }

    pub async fn merchant_route(
        &self,
        owner: UserId,
        id: i64,
    ) -> Result<MerchantRouteRecord, UserRouteError> {
        self.bounded(async {
            super::ensure_owner(self.pool.connection(), owner).await?;
            let row = owned_route(self.pool.connection(), owner, id).await?;
            let scope = route_scope(self.pool.connection(), id)
                .await?
                .ok_or(UserRouteError::NotFound)?;
            route_record(row, scope)
        })
        .await
    }

    pub async fn write_merchant_route(
        &self,
        owner: UserId,
        id: Option<i64>,
        write: MerchantRouteWrite,
    ) -> Result<MerchantRouteRecord, UserRouteError> {
        write.validate()?;
        let pool = MerchantPoolRepository::new(self.pool.clone(), self.lookup_timeout)
            .get_pool(write.pool_id)
            .await
            .map_err(|_| UserRouteError::Unavailable)?;
        let pool_group = GroupId::new(pool.group_id).map_err(|_| UserRouteError::Internal)?;
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let default = lock_owner(&txn, owner).await?;
            if !pool.enabled || !group_access(&txn, owner, default, pool_group).await? {
                return Err(UserRouteError::Unavailable);
            }
            let current_merchants = enrolled(&txn, write.pool_id).await?;
            if write
                .members
                .iter()
                .any(|m| !current_merchants.contains(&m.merchant_id))
            {
                return Err(UserRouteError::Unavailable);
            }
            let now = TimeDateTimeWithTimeZone::now_utc();
            let row = if let Some(id) = id {
                owned_route(&txn, owner, id).await?;
                let row = lock_route(&txn, id).await?;
                if route_scope(&txn, id).await?.is_none() {
                    return Err(UserRouteError::NotFound);
                }
                let mut active: user_routes::ActiveModel = row.into();
                active.name = Set(write.name.clone());
                active.strategy = Set(write.strategy.as_str().to_owned());
                active.enabled = Set(write.enabled);
                active.max_group_attempts = Set(write.max_merchant_attempts as i32);
                active.max_price_ratio_micros = Set(write.max_price_ratio_micros);
                active.updated_at = Set(now);
                active.update(&txn).await.map_err(internal)?
            } else {
                let count = user_routes::Entity::find()
                    .filter(user_routes::Column::OwnerUserId.eq(owner.get()))
                    .filter(user_routes::Column::DeletedAt.is_null())
                    .count(&txn)
                    .await
                    .map_err(internal)?;
                if count >= MAX_USER_ROUTES as u64 {
                    return Err(UserRouteError::LimitReached);
                }
                user_routes::ActiveModel {
                    owner_user_id: Set(owner.get()),
                    name: Set(write.name.clone()),
                    strategy: Set(write.strategy.as_str().to_owned()),
                    enabled: Set(write.enabled),
                    max_group_attempts: Set(write.max_merchant_attempts as i32),
                    max_price_ratio_micros: Set(write.max_price_ratio_micros),
                    created_at: Set(now),
                    updated_at: Set(now),
                    deleted_at: Set(None),
                    ..Default::default()
                }
                .insert(&txn)
                .await
                .map_err(internal)?
            };
            let previous_scope = route_scope(&txn, row.id).await?;
            let enrolled_merchants =
                enrollment_snapshot(previous_scope.as_ref(), &write, current_merchants);
            sync_members(
                &txn,
                row.id,
                &[UserRouteMemberWrite {
                    group_id: pool_group,
                    priority: 0,
                    weight: 1,
                }],
            )
            .await?;
            scope::Entity::delete_by_id(row.id)
                .exec(&txn)
                .await
                .map_err(internal)?;
            scope::ActiveModel {
                route_id: Set(row.id),
                pool_id: Set(write.pool_id),
                members: Set(
                    serde_json::to_value(&write.members).map_err(|_| UserRouteError::Internal)?
                ),
                enrolled_merchants: Set(serde_json::to_value(enrolled_merchants)
                    .map_err(|_| UserRouteError::Internal)?),
                excluded_merchant_ids: Set(serde_json::to_value(&write.excluded_merchant_ids)
                    .map_err(|_| UserRouteError::Internal)?),
                allow_pool_fallback: Set(write.allow_pool_fallback),
                accept_new_merchants: Set(write.accept_new_merchants),
                updated_at_millis: Set(now.unix_timestamp_nanos() as i64 / 1_000_000),
            }
            .insert(&txn)
            .await
            .map_err(internal)?;
            let record = MerchantRouteRecord {
                id: row.id,
                configuration: write,
            };
            txn.commit().await.map_err(internal)?;
            Ok(record)
        })
        .await
    }

    /// User lock and route lock serialize round robin claims across nodes.
    pub async fn claim_merchant_round_robin(
        &self,
        owner: UserId,
        route_id: i64,
        eligible: &[i64],
    ) -> Result<i64, UserRouteError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_owner(&txn, owner).await?;
            let row = lock_route(&txn, route_id).await?;
            if row.owner_user_id != owner.get() || !row.enabled {
                return Err(UserRouteError::Unavailable);
            }
            let state = scope::Entity::find_by_id(route_id)
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(UserRouteError::Unavailable)?;
            let mut members: Vec<MerchantRouteMember> =
                serde_json::from_value(state.members.clone())
                    .map_err(|_| UserRouteError::Internal)?;
            let selected = members
                .iter()
                .filter(|m| eligible.contains(&m.merchant_id))
                .min_by_key(|m| (m.last_selected_at_millis, m.priority, m.merchant_id))
                .map(|m| m.merchant_id)
                .or_else(|| eligible.first().copied())
                .ok_or(UserRouteError::Unavailable)?;
            let now = super::runtime::now_millis()?;
            let timestamp = members
                .iter()
                .filter_map(|m| m.last_selected_at_millis)
                .max()
                .unwrap_or(0)
                .saturating_add(1)
                .max(now);
            if let Some(member) = members.iter_mut().find(|m| m.merchant_id == selected) {
                member.last_selected_at_millis = Some(timestamp);
            } else {
                members.push(MerchantRouteMember {
                    merchant_id: selected,
                    priority: i32::MAX,
                    weight: 1,
                    last_selected_at_millis: Some(timestamp),
                });
            }
            let members = serde_json::to_value(members).map_err(|_| UserRouteError::Internal)?;
            let mut state: scope::ActiveModel = state.into();
            state.members = Set(members);
            state.updated_at_millis = Set(now);
            state.update(&txn).await.map_err(internal)?;
            txn.commit().await.map_err(internal)?;
            Ok(selected)
        })
        .await
    }
}

/// Editing a name, budget or priority does not consent to new data recipients.
fn enrollment_snapshot(
    previous: Option<&MerchantRouteScope>,
    write: &MerchantRouteWrite,
    current: Vec<i64>,
) -> Vec<i64> {
    match previous {
        Some(previous)
            if previous.pool_id == write.pool_id
                && previous.allow_pool_fallback
                && write.allow_pool_fallback
                && !previous.accept_new_merchants
                && !write.accept_new_merchants =>
        {
            previous.enrolled_merchants.clone()
        }
        _ => current,
    }
}

#[cfg(test)]
mod enrollment_tests {
    use super::*;

    fn configuration() -> MerchantRouteWrite {
        MerchantRouteWrite {
            name: "Renamed route".to_owned(),
            pool_id: 1,
            strategy: UserRouteStrategy::Ordered,
            enabled: true,
            max_merchant_attempts: 3,
            max_price_ratio_micros: Some(1_000_000),
            members: vec![MerchantRouteMember {
                merchant_id: 10,
                priority: 0,
                weight: 1,
                last_selected_at_millis: None,
            }],
            excluded_merchant_ids: Vec::new(),
            allow_pool_fallback: true,
            accept_new_merchants: false,
        }
    }

    #[test]
    fn editing_an_existing_fixed_pool_route_does_not_enroll_new_merchants() {
        let previous = MerchantRouteScope {
            pool_id: 1,
            members: Vec::new(),
            enrolled_merchants: vec![10, 20],
            excluded_merchant_ids: Vec::new(),
            allow_pool_fallback: true,
            accept_new_merchants: false,
        };
        let write = configuration();
        assert_eq!(
            enrollment_snapshot(Some(&previous), &write, vec![10, 20, 30]),
            vec![10, 20]
        );
        assert_eq!(
            enrollment_snapshot(None, &write, vec![10, 20, 30]),
            vec![10, 20, 30]
        );
    }

    #[test]
    fn explicitly_switching_fallback_policy_captures_the_current_pool() {
        let mut previous = MerchantRouteScope {
            pool_id: 1,
            members: Vec::new(),
            enrolled_merchants: vec![10],
            excluded_merchant_ids: Vec::new(),
            allow_pool_fallback: false,
            accept_new_merchants: false,
        };
        let write = configuration();
        assert_eq!(
            enrollment_snapshot(Some(&previous), &write, vec![10, 20]),
            vec![10, 20]
        );
        previous.allow_pool_fallback = true;
        previous.accept_new_merchants = true;
        assert_eq!(
            enrollment_snapshot(Some(&previous), &write, vec![10, 20]),
            vec![10, 20]
        );
    }
}

fn route_record(
    row: user_routes::Model,
    scope: MerchantRouteScope,
) -> Result<MerchantRouteRecord, UserRouteError> {
    let members = scope
        .members
        .into_iter()
        .map(|mut m| {
            m.last_selected_at_millis = None;
            m
        })
        .collect();
    Ok(MerchantRouteRecord {
        id: row.id,
        configuration: MerchantRouteWrite {
            name: row.name,
            pool_id: scope.pool_id,
            strategy: UserRouteStrategy::parse(&row.strategy).ok_or(UserRouteError::Internal)?,
            enabled: row.enabled,
            max_merchant_attempts: u32::try_from(row.max_group_attempts)
                .map_err(|_| UserRouteError::Internal)?,
            max_price_ratio_micros: row.max_price_ratio_micros,
            members,
            excluded_merchant_ids: scope.excluded_merchant_ids,
            allow_pool_fallback: scope.allow_pool_fallback,
            accept_new_merchants: scope.accept_new_merchants,
        },
    })
}

#[derive(Clone, Debug)]
pub struct MerchantAttemptWrite {
    pub id: String,
    pub pool_id: i64,
    pub route_id: i64,
    pub request_id: String,
    pub merchant_id: i64,
    pub service_id: i64,
    pub model: String,
    pub protocol: String,
    pub outcome: String,
    pub latency_ms: i64,
    pub ttft_ms: Option<i64>,
    pub streaming: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MerchantServiceStatistics {
    pub service_id: i64,
    pub merchant_id: i64,
    pub sample_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub success_rate: Option<f64>,
    pub ttft_p50_ms: Option<i64>,
    pub ttft_p95_ms: Option<i64>,
    pub latency_p50_ms: Option<i64>,
    pub latency_p95_ms: Option<i64>,
    pub status: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MerchantConsumerAttempt {
    pub merchant_id: i64,
    pub merchant_name: String,
    pub service_id: i64,
    pub service_name: String,
    pub request_id: String,
    pub outcome: String,
    pub latency_ms: i64,
    pub completed_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MerchantTimelineBucket {
    pub started_at: i64,
    pub ended_at: i64,
    pub success_count: u64,
    pub failure_count: u64,
    pub sample_count: u64,
    pub status: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct MerchantServiceTimeline {
    pub service_id: i64,
    pub merchant_id: i64,
    pub buckets: Vec<MerchantTimelineBucket>,
}

impl UserRouteRepository {
    pub async fn merchant_timeline(
        &self,
        pool_id: i64,
        model: Option<&str>,
        window_seconds: u32,
        protocol: Option<&str>,
        streaming: Option<bool>,
    ) -> Result<Vec<MerchantServiceTimeline>, UserRouteError> {
        if ![900, 3600, 86400, 604800].contains(&window_seconds)
            || model.is_some_and(|m| m.len() > 255)
        {
            return Err(UserRouteError::InvalidInput);
        }
        self.bounded(async {
            let end = super::runtime::now_millis()? / 1000;
            let start = end - i64::from(window_seconds);
            let mut query = attempts::Entity::find()
                .filter(attempts::Column::PoolId.eq(pool_id))
                .filter(attempts::Column::CompletedAtMillis.gte(start * 1000))
                .filter(attempts::Column::CompletedAtMillis.lte(end * 1000))
                .filter(attempts::Column::Outcome.ne("ignored"));
            if let Some(model) = model.filter(|m| !m.is_empty()) {
                query = query.filter(attempts::Column::Model.eq(model));
            }
            if let Some(protocol) = protocol.filter(|p| !p.is_empty()) {
                query = query.filter(attempts::Column::Protocol.eq(protocol));
            }
            if let Some(streaming) = streaming {
                query = query.filter(attempts::Column::Streaming.eq(streaming));
            }
            let rows = query
                .order_by_desc(attempts::Column::CompletedAtMillis)
                .limit(100_001)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            if rows.len() > 100_000 {
                return Err(UserRouteError::LimitReached);
            }
            let mut groups: BTreeMap<(i64, i64), Vec<MerchantTimelineBucket>> = BTreeMap::new();
            for row in rows {
                let buckets = groups
                    .entry((row.service_id, row.merchant_id))
                    .or_insert_with(|| {
                        (0..24)
                            .map(|i| MerchantTimelineBucket {
                                started_at: start + i * i64::from(window_seconds) / 24,
                                ended_at: start + (i + 1) * i64::from(window_seconds) / 24,
                                success_count: 0,
                                failure_count: 0,
                                sample_count: 0,
                                status: "unknown".to_owned(),
                            })
                            .collect()
                    });
                let index = usize::try_from(
                    (row.completed_at_millis / 1000 - start) * 24 / i64::from(window_seconds),
                )
                .unwrap_or(23)
                .min(23);
                let bucket = &mut buckets[index];
                bucket.sample_count += 1;
                if row.outcome == "success" {
                    bucket.success_count += 1;
                } else {
                    bucket.failure_count += 1;
                }
                bucket.status = if bucket.sample_count < 5 {
                    "unknown"
                } else if bucket.failure_count * 10 > bucket.sample_count {
                    "degraded"
                } else {
                    "normal"
                }
                .to_owned();
            }
            Ok(groups
                .into_iter()
                .map(
                    |((service_id, merchant_id), buckets)| MerchantServiceTimeline {
                        service_id,
                        merchant_id,
                        buckets,
                    },
                )
                .collect())
        })
        .await
    }

    pub async fn merchant_attempts(
        &self,
        owner: UserId,
        request_id: &str,
    ) -> Result<Vec<MerchantConsumerAttempt>, UserRouteError> {
        if request_id.is_empty() || request_id.len() > 128 {
            return Err(UserRouteError::InvalidInput);
        }
        self.bounded(async {
            super::ensure_owner(self.pool.connection(), owner).await?;
            let owned = Query::select()
                .column(user_routes::Column::Id)
                .from(user_routes::Entity)
                .and_where(Expr::col(user_routes::Column::OwnerUserId).eq(owner.get()))
                .to_owned();
            let rows = attempts::Entity::find()
                .filter(Expr::col(attempts::Column::RouteId).in_subquery(owned))
                .filter(attempts::Column::RequestId.eq(request_id))
                .order_by_asc(attempts::Column::CompletedAtMillis)
                .order_by_asc(attempts::Column::Id)
                .limit(32)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            let mut result = Vec::new();
            for row in rows {
                let merchant =
                    crate::entity::merchant_profiles::Entity::find_by_id(row.merchant_id)
                        .one(self.pool.connection())
                        .await
                        .map_err(internal)?;
                let service = crate::entity::merchant_services::Entity::find_by_id(row.service_id)
                    .one(self.pool.connection())
                    .await
                    .map_err(internal)?;
                result.push(MerchantConsumerAttempt {
                    merchant_id: row.merchant_id,
                    merchant_name: merchant
                        .map_or_else(|| format!("#{}", row.merchant_id), |m| m.name),
                    service_id: row.service_id,
                    service_name: service
                        .map_or_else(|| format!("#{}", row.service_id), |s| s.name),
                    request_id: row.request_id,
                    outcome: row.outcome,
                    latency_ms: row.latency_ms,
                    completed_at: row.completed_at_millis / 1000,
                });
            }
            Ok(result)
        })
        .await
    }

    pub async fn record_merchant_attempt(
        &self,
        write: MerchantAttemptWrite,
    ) -> Result<(), UserRouteError> {
        if write.id.len() != 36
            || write.request_id.is_empty()
            || write.request_id.len() > 128
            || write.model.len() > 255
            || write.latency_ms < 0
            || !matches!(write.outcome.as_str(), "success" | "failure" | "ignored")
        {
            return Err(UserRouteError::InvalidInput);
        }
        self.bounded(async {
            let now = super::runtime::now_millis()?;
            attempts::Entity::insert(attempts::ActiveModel {
                id: Set(write.id),
                pool_id: Set(write.pool_id),
                route_id: Set(write.route_id),
                request_id: Set(write.request_id),
                merchant_id: Set(write.merchant_id),
                service_id: Set(write.service_id),
                model: Set(write.model),
                protocol: Set(write.protocol),
                outcome: Set(write.outcome),
                streaming: Set(write.streaming),
                latency_ms: Set(write.latency_ms),
                ttft_ms: Set(write.ttft_ms),
                completed_at_millis: Set(now),
            })
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(attempts::Column::Id)
                    .do_nothing()
                    .to_owned(),
            )
            .do_nothing()
            .exec(self.pool.connection())
            .await
            .map_err(internal)?;
            Ok(())
        })
        .await
    }

    pub async fn merchant_statistics(
        &self,
        pool_id: i64,
        model: Option<&str>,
        window_seconds: u32,
    ) -> Result<Vec<MerchantServiceStatistics>, UserRouteError> {
        self.merchant_statistics_filtered(pool_id, model, window_seconds, None, None)
            .await
    }

    pub async fn merchant_statistics_filtered(
        &self,
        pool_id: i64,
        model: Option<&str>,
        window_seconds: u32,
        protocol: Option<&str>,
        streaming: Option<bool>,
    ) -> Result<Vec<MerchantServiceStatistics>, UserRouteError> {
        if ![900, 3600, 86400, 604800].contains(&window_seconds)
            || model.is_some_and(|m| m.len() > 255)
        {
            return Err(UserRouteError::InvalidInput);
        }
        self.bounded(async {
            let now = super::runtime::now_millis()?;
            let mut query = attempts::Entity::find()
                .filter(attempts::Column::PoolId.eq(pool_id))
                .filter(
                    attempts::Column::CompletedAtMillis.gte(now - i64::from(window_seconds) * 1000),
                )
                .filter(attempts::Column::Outcome.ne("ignored"));
            if let Some(model) = model.filter(|m| !m.is_empty()) {
                query = query.filter(attempts::Column::Model.eq(model));
            }
            if let Some(protocol) = protocol.filter(|p| !p.is_empty()) {
                query = query.filter(attempts::Column::Protocol.eq(protocol));
            }
            if let Some(streaming) = streaming {
                query = query.filter(attempts::Column::Streaming.eq(streaming));
            }
            let rows = query
                .order_by_desc(attempts::Column::CompletedAtMillis)
                .limit(100_001)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            if rows.len() > 100_000 {
                return Err(UserRouteError::LimitReached);
            }
            let mut groups: BTreeMap<(i64, i64), Vec<attempts::Model>> = BTreeMap::new();
            for row in rows {
                groups
                    .entry((row.service_id, row.merchant_id))
                    .or_default()
                    .push(row);
            }
            Ok(groups
                .into_iter()
                .map(|((service_id, merchant_id), rows)| {
                    let success_count =
                        rows.iter().filter(|r| r.outcome == "success").count() as u64;
                    let sample_count = rows.len() as u64;
                    let failure_count = sample_count - success_count;
                    let mut latency = rows
                        .iter()
                        .filter(|r| r.outcome == "success")
                        .map(|r| r.latency_ms)
                        .collect::<Vec<_>>();
                    let mut ttft = rows
                        .iter()
                        .filter(|r| r.outcome == "success")
                        .filter_map(|r| r.ttft_ms)
                        .collect::<Vec<_>>();
                    latency.sort_unstable();
                    ttft.sort_unstable();
                    let updated_at = rows
                        .iter()
                        .map(|r| r.completed_at_millis)
                        .max()
                        .unwrap_or(now)
                        / 1000;
                    MerchantServiceStatistics {
                        service_id,
                        merchant_id,
                        sample_count,
                        success_count,
                        failure_count,
                        success_rate: Some(success_count as f64 * 100.0 / sample_count as f64),
                        ttft_p50_ms: percentile(&ttft, 50),
                        ttft_p95_ms: percentile(&ttft, 95),
                        latency_p50_ms: percentile(&latency, 50),
                        latency_p95_ms: percentile(&latency, 95),
                        status: if now / 1000 - updated_at > 900 || sample_count < 20 {
                            "unknown"
                        } else if failure_count * 10 > sample_count {
                            "degraded"
                        } else {
                            "normal"
                        }
                        .to_owned(),
                        updated_at,
                    }
                })
                .collect())
        })
        .await
    }
}

fn percentile(values: &[i64], percent: usize) -> Option<i64> {
    if values.is_empty() {
        None
    } else {
        Some(values[(values.len() * percent).div_ceil(100).saturating_sub(1)])
    }
}
