use std::{collections::BTreeSet, fmt, time::Duration};

use af_domain::{
    ChannelId, CredentialId, MAX_MODEL_NAME_BYTES, RouteChannelId, RouteId, RouteMode,
    RouteStrategy, route_model_pattern_matches, validate_route_model_pattern,
};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, MAX_ADMIN_ROUTE_CHANNELS, MAX_ADMIN_ROUTE_MAPPING_BYTES,
    entity::{route_channels, routes},
};

/// 单次运行时读取允许扫描的启用规则上限。
pub const MAX_SMART_ROUTE_RUNTIME_RULES: usize = 256;

/// 已校验的智能路由凭据候选。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct SmartRouteRuntimeCandidate {
    route_channel_id: RouteChannelId,
    channel_id: ChannelId,
    credential_id: CredentialId,
    priority: i32,
    weight: u32,
    last_selected_at_millis: Option<i64>,
}

impl SmartRouteRuntimeCandidate {
    /// 构造已经完成标识和非负调度字段校验的运行时候选。
    pub fn new(
        route_channel_id: RouteChannelId,
        channel_id: ChannelId,
        credential_id: CredentialId,
        priority: i32,
        weight: u32,
        last_selected_at_millis: Option<i64>,
    ) -> Result<Self, SmartRouteRuntimeRepositoryError> {
        if priority < 0 || last_selected_at_millis.is_some_and(|value| value < 0) {
            return Err(invariant());
        }
        Ok(Self {
            route_channel_id,
            channel_id,
            credential_id,
            priority,
            weight,
            last_selected_at_millis,
        })
    }

    /// 返回路由候选记录标识，供运行时反馈精确关联。
    #[must_use]
    pub const fn route_channel_id(self) -> RouteChannelId {
        self.route_channel_id
    }

    /// 返回绑定渠道。
    #[must_use]
    pub const fn channel_id(self) -> ChannelId {
        self.channel_id
    }

    /// 返回绑定凭据。
    #[must_use]
    pub const fn credential_id(self) -> CredentialId {
        self.credential_id
    }

    /// 返回规则内凭据优先级。
    #[must_use]
    pub const fn priority(self) -> i32 {
        self.priority
    }

    /// 返回规则内凭据权重。
    #[must_use]
    pub const fn weight(self) -> u32 {
        self.weight
    }

    /// 返回最近一次真实尝试的 Unix 毫秒时间，仅供顺序轮询排序。
    #[must_use]
    pub const fn last_selected_at_millis(self) -> Option<i64> {
        self.last_selected_at_millis
    }
}

impl fmt::Debug for SmartRouteRuntimeCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SmartRouteRuntimeCandidate")
            .field("route_channel_id", &self.route_channel_id)
            .field("channel_id", &self.channel_id)
            .field("credential_id", &self.credential_id)
            .field("priority", &self.priority)
            .field("weight", &self.weight)
            .field(
                "has_last_selected_at",
                &self.last_selected_at_millis.is_some(),
            )
            .finish()
    }
}

/// 已按固定优先级匹配并完成模型映射的智能路由规则。
pub struct SmartRouteRuntimeRule {
    route_id: RouteId,
    mode: RouteMode,
    strategy: RouteStrategy,
    routed_model: String,
    candidates: Vec<SmartRouteRuntimeCandidate>,
}

impl SmartRouteRuntimeRule {
    /// 构造用于测试边界或其他只读来源的已校验规则快照。
    pub fn new(
        route_id: RouteId,
        mode: RouteMode,
        strategy: RouteStrategy,
        routed_model: String,
        candidates: Vec<SmartRouteRuntimeCandidate>,
    ) -> Result<Self, SmartRouteRuntimeRepositoryError> {
        if !valid_model(&routed_model) || candidates.len() > MAX_ADMIN_ROUTE_CHANNELS {
            return Err(invariant());
        }
        Ok(Self {
            route_id,
            mode,
            strategy,
            routed_model,
            candidates,
        })
    }

    /// 返回规则标识。
    #[must_use]
    pub const fn route_id(&self) -> RouteId {
        self.route_id
    }

    /// 返回规则匹配模式。
    #[must_use]
    pub const fn mode(&self) -> RouteMode {
        self.mode
    }

    /// 返回规则候选策略。
    #[must_use]
    pub const fn strategy(&self) -> RouteStrategy {
        self.strategy
    }

    /// 返回规则映射后的 Canonical 路由模型。
    #[must_use]
    pub fn routed_model(&self) -> &str {
        &self.routed_model
    }

    /// 返回启用且已校验的凭据候选。
    #[must_use]
    pub fn candidates(&self) -> &[SmartRouteRuntimeCandidate] {
        &self.candidates
    }
}

impl fmt::Debug for SmartRouteRuntimeRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SmartRouteRuntimeRule")
            .field("route_id", &self.route_id)
            .field("mode", &self.mode)
            .field("strategy", &self.strategy)
            .field("routed_model", &"<已脱敏>")
            .field("candidate_count", &self.candidates.len())
            .finish()
    }
}

/// 智能路由运行时仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SmartRouteRuntimeRepositoryConfigError {
    /// 零超时无法形成有界数据库读取。
    #[error("智能路由运行时查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 智能路由运行时读取错误，不携带模型名或规则正文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SmartRouteRuntimeRepositoryError {
    /// 数据库查询失败。
    #[error("智能路由运行时查询失败")]
    Query,
    /// 查询超过硬截止时间。
    #[error("智能路由运行时查询超时")]
    Timeout,
    /// 持久化规则违反运行时不变量。
    #[error("智能路由运行时规则损坏")]
    Invariant,
}

/// 一次真实 Relay 尝试产生的路由候选统计增量。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SmartRouteAttemptFeedback {
    route_channel_id: RouteChannelId,
    success: bool,
    elapsed_millis: i64,
}

impl SmartRouteAttemptFeedback {
    /// 使用已截断到非负 `i64` 的毫秒耗时构造反馈。
    pub fn new(
        route_channel_id: RouteChannelId,
        success: bool,
        elapsed_millis: i64,
    ) -> Result<Self, SmartRouteRuntimeRepositoryError> {
        if elapsed_millis < 0 {
            return Err(invariant());
        }
        Ok(Self {
            route_channel_id,
            success,
            elapsed_millis,
        })
    }
}

/// 面向生产调度的有界智能路由只读仓储。
#[derive(Clone)]
pub struct SmartRouteRuntimeRepository {
    pool: DatabasePool,
    lookup_timeout: Duration,
}

impl SmartRouteRuntimeRepository {
    /// 使用共享连接池和硬截止时间创建仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, SmartRouteRuntimeRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(SmartRouteRuntimeRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按固定匹配等级返回请求模型命中的全部启用规则。
    pub async fn matching_rules(
        &self,
        requested_model: &str,
    ) -> Result<Vec<SmartRouteRuntimeRule>, SmartRouteRuntimeRepositoryError> {
        if !valid_model(requested_model) {
            return Err(record_error(SmartRouteRuntimeRepositoryError::Invariant));
        }
        let operation = async {
            routes::Entity::find()
                .filter(routes::Column::Enabled.eq(true))
                .filter(routes::Column::DeletedAt.is_null())
                .order_by_asc(routes::Column::Id)
                .limit((MAX_SMART_ROUTE_RUNTIME_RULES + 1) as u64)
                .all(self.pool.connection())
                .await
                .map_err(|_| SmartRouteRuntimeRepositoryError::Query)
        }
        .with_subscriber(NoSubscriber::default());
        let rows = match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result.map_err(record_error)?,
            Err(_) => return Err(record_error(SmartRouteRuntimeRepositoryError::Timeout)),
        };
        if rows.len() > MAX_SMART_ROUTE_RUNTIME_RULES {
            return Err(record_error(SmartRouteRuntimeRepositoryError::Invariant));
        }

        let mut matched = Vec::new();
        for row in rows {
            let Some(rank) = match_rank(row.route_mode, &row.model_pattern, requested_model)?
            else {
                continue;
            };
            let route_id = RouteId::new(row.id).map_err(|_| invariant())?;
            let mode = RouteMode::try_from(row.route_mode).map_err(|_| invariant())?;
            let strategy = RouteStrategy::try_from(row.strategy).map_err(|_| invariant())?;
            let routed_model =
                resolve_routed_model(requested_model, &row.model_pattern, &row.model_mapping)?;
            let candidates = self.load_candidates(route_id).await?;
            matched.push((
                rank,
                SmartRouteRuntimeRule::new(route_id, mode, strategy, routed_model, candidates)?,
            ));
        }
        matched.sort_unstable_by(|(left_rank, left), (right_rank, right)| {
            right_rank
                .cmp(left_rank)
                .then_with(|| left.route_id().cmp(&right.route_id()))
        });
        Ok(matched.into_iter().map(|(_, rule)| rule).collect())
    }

    async fn load_candidates(
        &self,
        route_id: RouteId,
    ) -> Result<Vec<SmartRouteRuntimeCandidate>, SmartRouteRuntimeRepositoryError> {
        let operation = async {
            route_channels::Entity::find()
                .filter(route_channels::Column::RouteId.eq(route_id.get()))
                .filter(route_channels::Column::Enabled.eq(true))
                .order_by_desc(route_channels::Column::Priority)
                .order_by_asc(route_channels::Column::Id)
                .limit((MAX_ADMIN_ROUTE_CHANNELS + 1) as u64)
                .all(self.pool.connection())
                .await
                .map_err(|_| SmartRouteRuntimeRepositoryError::Query)
        }
        .with_subscriber(NoSubscriber::default());
        let rows = match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result.map_err(record_error)?,
            Err(_) => return Err(record_error(SmartRouteRuntimeRepositoryError::Timeout)),
        };
        if rows.len() > MAX_ADMIN_ROUTE_CHANNELS {
            return Err(invariant());
        }
        let mut seen = BTreeSet::new();
        rows.into_iter()
            .map(|row| {
                let key = (row.channel_id, row.credential_id);
                if row.route_id != route_id.get()
                    || row.priority < 0
                    || row.weight < 0
                    || !seen.insert(key)
                {
                    return Err(invariant());
                }
                SmartRouteRuntimeCandidate::new(
                    RouteChannelId::new(row.id).map_err(|_| invariant())?,
                    ChannelId::new(row.channel_id).map_err(|_| invariant())?,
                    CredentialId::new(row.credential_id).map_err(|_| invariant())?,
                    row.priority,
                    u32::try_from(row.weight).map_err(|_| invariant())?,
                    row.last_selected_at
                        .map(|value| i64::try_from(value.unix_timestamp_nanos() / 1_000_000))
                        .transpose()
                        .map_err(|_| invariant())?,
                )
            })
            .collect()
    }

    /// 原子记录一批真实尝试；任一计数溢出或目标缺失时整批失败。
    pub async fn record_attempts(
        &self,
        feedback: &[SmartRouteAttemptFeedback],
    ) -> Result<(), SmartRouteRuntimeRepositoryError> {
        if feedback.len() > MAX_ADMIN_ROUTE_CHANNELS {
            return Err(invariant());
        }
        let mut seen = BTreeSet::new();
        if feedback
            .iter()
            .any(|item| !seen.insert(item.route_channel_id))
        {
            return Err(invariant());
        }
        let operation = async {
            let transaction = self
                .pool
                .connection()
                .begin()
                .await
                .map_err(|_| SmartRouteRuntimeRepositoryError::Query)?;
            let now = TimeDateTimeWithTimeZone::now_utc();
            for item in feedback {
                let count_column = if item.success {
                    route_channels::Column::SuccessCount
                } else {
                    route_channels::Column::FailCount
                };
                let mut update = route_channels::Entity::update_many()
                    .filter(route_channels::Column::Id.eq(item.route_channel_id.get()))
                    .filter(count_column.lt(i64::MAX))
                    .filter(
                        route_channels::Column::TotalLatency.lte(i64::MAX - item.elapsed_millis),
                    )
                    .col_expr(count_column, Expr::col(count_column).add(1_i64))
                    .col_expr(
                        route_channels::Column::TotalLatency,
                        Expr::col(route_channels::Column::TotalLatency).add(item.elapsed_millis),
                    )
                    .col_expr(route_channels::Column::LastSelectedAt, Expr::value(now))
                    .col_expr(route_channels::Column::UpdatedAt, Expr::value(now));
                if !item.success {
                    update =
                        update.col_expr(route_channels::Column::LastFailureAt, Expr::value(now));
                }
                if update
                    .exec(&transaction)
                    .await
                    .map_err(|_| SmartRouteRuntimeRepositoryError::Query)?
                    .rows_affected
                    != 1
                {
                    return Err(SmartRouteRuntimeRepositoryError::Invariant);
                }
            }
            transaction
                .commit()
                .await
                .map_err(|_| SmartRouteRuntimeRepositoryError::Query)
        }
        .with_subscriber(NoSubscriber::default());
        match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result.map_err(record_error),
            Err(_) => Err(record_error(SmartRouteRuntimeRepositoryError::Timeout)),
        }
    }
}

impl fmt::Debug for SmartRouteRuntimeRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SmartRouteRuntimeRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

fn match_rank(
    mode_code: i16,
    pattern: &str,
    requested_model: &str,
) -> Result<Option<u8>, SmartRouteRuntimeRepositoryError> {
    if pattern.trim() != pattern {
        return Err(invariant());
    }
    let matches = route_model_pattern_matches(pattern, requested_model).map_err(|_| invariant())?;
    match RouteMode::try_from(mode_code).map_err(|_| invariant())? {
        RouteMode::ExplicitGroup => {
            if pattern.starts_with("re:") {
                return Err(invariant());
            }
            Ok(matches.then_some(3))
        }
        RouteMode::Pattern if !pattern.starts_with("re:") => Ok(matches.then_some(2)),
        RouteMode::Pattern => Ok(matches.then_some(1)),
    }
}

fn resolve_routed_model(
    requested_model: &str,
    route_pattern: &str,
    mapping: &serde_json::Value,
) -> Result<String, SmartRouteRuntimeRepositoryError> {
    if !validate_smart_route_model_mapping(mapping) {
        return Err(invariant());
    }
    let object = mapping.as_object().ok_or_else(invariant)?;
    let routed = object
        .get(requested_model)
        .or_else(|| object.get(route_pattern))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(requested_model);
    if !valid_model(routed) {
        return Err(invariant());
    }
    Ok(routed.to_owned())
}

/// 校验智能路由映射的有界模式键和 Canonical 模型值。
#[must_use]
pub fn validate_smart_route_model_mapping(mapping: &serde_json::Value) -> bool {
    if serde_json::to_vec(mapping)
        .map(|encoded| encoded.len() > MAX_ADMIN_ROUTE_MAPPING_BYTES)
        .unwrap_or(true)
    {
        return false;
    }
    mapping.as_object().is_some_and(|object| {
        object.iter().all(|(source, target)| {
            valid_mapping_source(source) && target.as_str().is_some_and(valid_model)
        })
    })
}

fn valid_mapping_source(value: &str) -> bool {
    if value.starts_with("re:") {
        validate_route_model_pattern(value).is_ok()
    } else {
        valid_model(value)
    }
}

fn valid_model(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MODEL_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn invariant() -> SmartRouteRuntimeRepositoryError {
    record_error(SmartRouteRuntimeRepositoryError::Invariant)
}

fn record_error(error: SmartRouteRuntimeRepositoryError) -> SmartRouteRuntimeRepositoryError {
    let error_kind = match error {
        SmartRouteRuntimeRepositoryError::Query => "smart_route_runtime_query",
        SmartRouteRuntimeRepositoryError::Timeout => "smart_route_runtime_timeout",
        SmartRouteRuntimeRepositoryError::Invariant => "smart_route_runtime_invariant",
    };
    tracing::error!(target: "af_db::smart_route_runtime", error_kind, "智能路由运行时仓储失败");
    error
}
