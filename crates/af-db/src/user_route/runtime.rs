use af_domain::{GroupId, MAX_MODEL_NAME_BYTES, MAX_USER_ROUTE_MEMBERS};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};

use crate::entity::{user_route_health, user_route_members, user_routes};

use super::{UserRouteError, UserRouteRepository, internal, lock_route};

const HEALTH_WINDOW_MILLIS: i64 = 15 * 60 * 1_000;

/// 按路由、实体分组及模型聚合的近期健康数据。
#[derive(Clone, Debug)]
pub struct UserRouteModelHealth {
    pub group_id: GroupId,
    pub success_count: u64,
    pub fail_count: u64,
    pub total_latency_ms: u64,
    pub cooldown_until_millis: Option<i64>,
    pub updated_at_millis: i64,
}

impl UserRouteRepository {
    /// 仅读取当前滚动窗口内的反馈；过期窗口按无样本处理。
    pub async fn load_model_health(
        &self,
        route_id: i64,
        model: &str,
    ) -> Result<Vec<UserRouteModelHealth>, UserRouteError> {
        validate_model(model)?;
        self.bounded(async {
            let now = now_millis()?;
            let rows = user_route_health::Entity::find()
                .filter(user_route_health::Column::RouteId.eq(route_id))
                .filter(user_route_health::Column::Model.eq(model))
                .filter(
                    user_route_health::Column::WindowStartedAtMillis.gt(now - HEALTH_WINDOW_MILLIS),
                )
                .order_by_asc(user_route_health::Column::GroupId)
                .limit((MAX_USER_ROUTE_MEMBERS + 1) as u64)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            if rows.len() > MAX_USER_ROUTE_MEMBERS {
                return Err(UserRouteError::Internal);
            }
            rows.into_iter()
                .map(|row| {
                    if row.success_count < 0 || row.fail_count < 0 || row.total_latency_ms < 0 {
                        return Err(UserRouteError::Internal);
                    }
                    Ok(UserRouteModelHealth {
                        group_id: GroupId::new(row.group_id)
                            .map_err(|_| UserRouteError::Internal)?,
                        success_count: u64::try_from(row.success_count)
                            .map_err(|_| UserRouteError::Internal)?,
                        fail_count: u64::try_from(row.fail_count)
                            .map_err(|_| UserRouteError::Internal)?,
                        total_latency_ms: u64::try_from(row.total_latency_ms)
                            .map_err(|_| UserRouteError::Internal)?,
                        cooldown_until_millis: row.cooldown_until_millis,
                        updated_at_millis: row.updated_at_millis,
                    })
                })
                .collect()
        })
        .await
    }

    /// 原子分配轮询首选，避免并发请求读取相同首选成员。
    pub async fn claim_round_robin(
        &self,
        route_id: i64,
        eligible_group_ids: &[GroupId],
    ) -> Result<GroupId, UserRouteError> {
        if eligible_group_ids.is_empty() || eligible_group_ids.len() > MAX_USER_ROUTE_MEMBERS {
            return Err(UserRouteError::Unavailable);
        }
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_route(&txn, route_id).await?;
            let route = user_routes::Entity::find_by_id(route_id)
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(UserRouteError::Unavailable)?;
            if !route.enabled {
                return Err(UserRouteError::Unavailable);
            }
            let rows = user_route_members::Entity::find()
                .filter(user_route_members::Column::RouteId.eq(route_id))
                .filter(
                    user_route_members::Column::GroupId
                        .is_in(eligible_group_ids.iter().map(|id| id.get())),
                )
                .all(&txn)
                .await
                .map_err(internal)?;
            let selected = rows
                .iter()
                .min_by_key(|row| (row.last_selected_at_millis, row.priority, row.group_id))
                .ok_or(UserRouteError::Unavailable)?;
            let timestamp = rows
                .iter()
                .filter_map(|row| row.last_selected_at_millis)
                .max()
                .unwrap_or(0)
                .saturating_add(1)
                .max(now_millis()?);
            user_route_members::Entity::update_many()
                .filter(user_route_members::Column::RouteId.eq(route_id))
                .filter(user_route_members::Column::GroupId.eq(selected.group_id))
                .col_expr(
                    user_route_members::Column::LastSelectedAtMillis,
                    Expr::value(timestamp),
                )
                .exec(&txn)
                .await
                .map_err(internal)?;
            let group = GroupId::new(selected.group_id).map_err(|_| UserRouteError::Internal)?;
            txn.commit().await.map_err(internal)?;
            Ok(group)
        })
        .await
    }

    /// 仅记录真实选择，不因候选规划更新其他成员。
    pub async fn record_selection(
        &self,
        route_id: i64,
        group_id: GroupId,
    ) -> Result<(), UserRouteError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_route(&txn, route_id).await?;
            let route = user_routes::Entity::find_by_id(route_id)
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(UserRouteError::Unavailable)?;
            if !route.enabled {
                return Err(UserRouteError::Unavailable);
            }
            // 轮询首选已由 claim 原子预约；迟到的 selection 不得改变预约顺序。
            if route.strategy == af_domain::UserRouteStrategy::RoundRobin.as_str() {
                return txn.commit().await.map_err(internal);
            }
            let rows = user_route_members::Entity::find()
                .filter(user_route_members::Column::RouteId.eq(route_id))
                .all(&txn)
                .await
                .map_err(internal)?;
            if !rows.iter().any(|row| row.group_id == group_id.get()) {
                return Err(UserRouteError::Unavailable);
            }
            let timestamp = rows
                .iter()
                .filter_map(|row| row.last_selected_at_millis)
                .max()
                .unwrap_or(0)
                .saturating_add(1)
                .max(now_millis()?);
            user_route_members::Entity::update_many()
                .filter(user_route_members::Column::RouteId.eq(route_id))
                .filter(user_route_members::Column::GroupId.eq(group_id.get()))
                .col_expr(
                    user_route_members::Column::LastSelectedAtMillis,
                    Expr::value(timestamp),
                )
                .exec(&txn)
                .await
                .map_err(internal)?;
            txn.commit().await.map_err(internal)
        })
        .await
    }

    /// 参数和策略错误不损害分组稳定评分；真实可重试错误进入冷却。
    pub async fn record_attempt(
        &self,
        route_id: i64,
        group_id: GroupId,
        model: &str,
        success: bool,
        latency_ms: u64,
        retryable_failure: bool,
    ) -> Result<(), UserRouteError> {
        validate_model(model)?;
        let latency_ms = i64::try_from(latency_ms).unwrap_or(i64::MAX);
        if !success && !retryable_failure {
            return Ok(());
        }
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            // 路由锁同时串行化首次插入与窗口重置，避免跨进程丢失增量。
            match lock_route(&txn, route_id).await {
                Err(UserRouteError::Unavailable) => return Ok(()),
                result => result?,
            };
            if user_route_members::Entity::find_by_id((route_id, group_id.get()))
                .one(&txn)
                .await
                .map_err(internal)?
                .is_none()
            {
                return Ok(());
            }
            let now = now_millis()?;
            let previous =
                user_route_health::Entity::find_by_id((route_id, group_id.get(), model.to_owned()))
                    .one(&txn)
                    .await
                    .map_err(internal)?;
            let exists = previous.is_some();
            let mut row = previous.unwrap_or_else(|| user_route_health::Model {
                route_id,
                group_id: group_id.get(),
                model: model.to_owned(),
                success_count: 0,
                fail_count: 0,
                total_latency_ms: 0,
                consecutive_failures: 0,
                cooldown_until_millis: None,
                window_started_at_millis: now,
                updated_at_millis: now,
            });
            if row.window_started_at_millis <= now - HEALTH_WINDOW_MILLIS {
                row.success_count = 0;
                row.fail_count = 0;
                row.total_latency_ms = 0;
                row.consecutive_failures = 0;
                row.cooldown_until_millis = None;
                row.window_started_at_millis = now;
            }
            if success {
                row.success_count = row.success_count.saturating_add(1);
                row.total_latency_ms = row.total_latency_ms.saturating_add(latency_ms);
                row.consecutive_failures = 0;
                row.cooldown_until_millis = None;
            } else {
                row.fail_count = row.fail_count.saturating_add(1);
                row.consecutive_failures = row.consecutive_failures.saturating_add(1);
                if row.consecutive_failures >= 3 {
                    row.cooldown_until_millis = Some(now.saturating_add(30_000));
                }
            }
            row.updated_at_millis = now;
            // Set 每列，确保更新后的 Model 被持久化。
            let active = user_route_health::ActiveModel {
                route_id: Set(row.route_id),
                group_id: Set(row.group_id),
                model: Set(row.model),
                success_count: Set(row.success_count),
                fail_count: Set(row.fail_count),
                total_latency_ms: Set(row.total_latency_ms),
                consecutive_failures: Set(row.consecutive_failures),
                cooldown_until_millis: Set(row.cooldown_until_millis),
                window_started_at_millis: Set(row.window_started_at_millis),
                updated_at_millis: Set(row.updated_at_millis),
            };
            if exists {
                active.update(&txn).await.map_err(internal)?;
            } else {
                active.insert(&txn).await.map_err(internal)?;
            }
            txn.commit().await.map_err(internal)
        })
        .await
    }
}

fn now_millis() -> Result<i64, UserRouteError> {
    i64::try_from(TimeDateTimeWithTimeZone::now_utc().unix_timestamp_nanos() / 1_000_000)
        .map_err(|_| UserRouteError::Internal)
}

fn validate_model(model: &str) -> Result<(), UserRouteError> {
    if model.is_empty()
        || model.len() > MAX_MODEL_NAME_BYTES
        || model.trim() != model
        || model.chars().any(char::is_control)
    {
        return Err(UserRouteError::InvalidInput);
    }
    Ok(())
}
