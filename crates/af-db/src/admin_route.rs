use std::{
    collections::{HashMap, HashSet},
    fmt,
    time::Duration,
};

use af_domain::{
    ChannelId, CredentialId, RouteChannelId, RouteId, RouteMode, RouteStrategy,
    validate_route_model_pattern,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::Expr,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{channels, credentials, route_channels, routes},
    smart_route_runtime::validate_smart_route_model_mapping,
};

/// 管理端路由列表允许返回的最大记录数。
pub const MAX_ADMIN_ROUTE_PAGE_SIZE: usize = 100;
/// 单条路由允许绑定的候选上限，与调度层候选上限保持一致。
pub const MAX_ADMIN_ROUTE_CHANNELS: usize = 64;
/// 路由名称允许的最大 UTF-8 字节数。
pub const MAX_ADMIN_ROUTE_NAME_BYTES: usize = 128;
/// 模型映射对象编码后的最大字节数。
pub const MAX_ADMIN_ROUTE_MAPPING_BYTES: usize = 16 * 1024;

/// 已校验的路由候选配置及运行时统计。
#[derive(Clone)]
pub struct AdminRouteChannelRecord {
    id: RouteChannelId,
    channel_id: ChannelId,
    credential_id: CredentialId,
    priority: i32,
    weight: i32,
    enabled: bool,
    success_count: i64,
    fail_count: i64,
    total_latency: i64,
    cooldown_level: i16,
    cooldown_until: Option<TimeDateTimeWithTimeZone>,
    last_selected_at: Option<TimeDateTimeWithTimeZone>,
    last_failure_at: Option<TimeDateTimeWithTimeZone>,
}

impl AdminRouteChannelRecord {
    /// 返回候选记录标识。
    #[must_use]
    pub const fn id(&self) -> RouteChannelId {
        self.id
    }
    /// 返回绑定的渠道标识。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }
    /// 返回绑定的凭据标识。
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.credential_id
    }
    /// 返回候选优先级。
    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }
    /// 返回候选权重。
    #[must_use]
    pub const fn weight(&self) -> i32 {
        self.weight
    }
    /// 返回候选是否参与路由。
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    /// 返回成功次数。
    #[must_use]
    pub const fn success_count(&self) -> i64 {
        self.success_count
    }
    /// 返回失败次数。
    #[must_use]
    pub const fn fail_count(&self) -> i64 {
        self.fail_count
    }
    /// 返回累计延迟（毫秒）。
    #[must_use]
    pub const fn total_latency(&self) -> i64 {
        self.total_latency
    }
    /// 返回当前冷却级别。
    #[must_use]
    pub const fn cooldown_level(&self) -> i16 {
        self.cooldown_level
    }
    /// 返回冷却截止时间。
    #[must_use]
    pub const fn cooldown_until(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.cooldown_until
    }
    /// 返回最近一次被选中的时间。
    #[must_use]
    pub const fn last_selected_at(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.last_selected_at
    }
    /// 返回最近一次失败时间。
    #[must_use]
    pub const fn last_failure_at(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.last_failure_at
    }
}

impl fmt::Debug for AdminRouteChannelRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRouteChannelRecord(<redacted>)")
    }
}

/// 已校验的管理端路由快照。
pub struct AdminRouteRecord {
    route_id: RouteId,
    name: String,
    model_pattern: String,
    mode: RouteMode,
    strategy: RouteStrategy,
    model_mapping: serde_json::Value,
    enabled: bool,
    channels: Vec<AdminRouteChannelRecord>,
}

impl AdminRouteRecord {
    /// 返回路由标识。
    #[must_use]
    pub const fn route_id(&self) -> RouteId {
        self.route_id
    }
    /// 返回路由名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// 返回模型匹配表达式。
    #[must_use]
    pub fn model_pattern(&self) -> &str {
        &self.model_pattern
    }
    /// 返回路由模式。
    #[must_use]
    pub const fn mode(&self) -> RouteMode {
        self.mode
    }
    /// 返回候选策略。
    #[must_use]
    pub const fn strategy(&self) -> RouteStrategy {
        self.strategy
    }
    /// 返回模型映射对象。
    #[must_use]
    pub fn model_mapping(&self) -> &serde_json::Value {
        &self.model_mapping
    }
    /// 返回是否启用。
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    /// 返回有序候选列表。
    #[must_use]
    pub fn channels(&self) -> &[AdminRouteChannelRecord] {
        &self.channels
    }
}

impl fmt::Debug for AdminRouteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRouteRecord(<redacted>)")
    }
}

/// 路由候选写入字段；敏感凭据内容不在该契约中出现。
pub struct AdminRouteChannelWriteRecord {
    channel_id: ChannelId,
    credential_id: CredentialId,
    priority: i32,
    weight: i32,
    enabled: bool,
}

impl AdminRouteChannelWriteRecord {
    /// 组装已经完成正数 ID 校验的候选配置。
    #[must_use]
    pub const fn new(
        channel_id: ChannelId,
        credential_id: CredentialId,
        priority: i32,
        weight: i32,
        enabled: bool,
    ) -> Self {
        Self {
            channel_id,
            credential_id,
            priority,
            weight,
            enabled,
        }
    }
}

impl fmt::Debug for AdminRouteChannelWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRouteChannelWriteRecord(<redacted>)")
    }
}

/// 管理端路由创建或完整更新字段。
pub struct AdminRouteWriteRecord {
    name: String,
    model_pattern: String,
    mode: RouteMode,
    strategy: RouteStrategy,
    model_mapping: serde_json::Value,
    enabled: bool,
    channels: Vec<AdminRouteChannelWriteRecord>,
}

impl AdminRouteWriteRecord {
    /// 组装已经由应用层校验的路由写入字段。
    #[allow(clippy::too_many_arguments, reason = "字段与管理端路由契约一一对应")]
    #[must_use]
    pub fn new(
        name: String,
        model_pattern: String,
        mode: RouteMode,
        strategy: RouteStrategy,
        model_mapping: serde_json::Value,
        enabled: bool,
        channels: Vec<AdminRouteChannelWriteRecord>,
    ) -> Self {
        Self {
            name,
            model_pattern,
            mode,
            strategy,
            model_mapping,
            enabled,
            channels,
        }
    }
}

impl fmt::Debug for AdminRouteWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRouteWriteRecord(<redacted>)")
    }
}

/// 一页有界路由结果。
pub struct AdminRoutePageRecord {
    routes: Vec<AdminRouteRecord>,
    next_cursor: Option<RouteId>,
}

impl AdminRoutePageRecord {
    /// 消费页面并返回路由与下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<AdminRouteRecord>, Option<RouteId>) {
        (self.routes, self.next_cursor)
    }
}

impl fmt::Debug for AdminRoutePageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRoutePageRecord(<redacted>)")
    }
}

/// 路由详情查询结果。
pub enum AdminRouteLookupOutcome {
    /// 找到未软删除路由。
    Found(AdminRouteRecord),
    /// 路由不存在或已经软删除。
    NotFound,
}

impl fmt::Debug for AdminRouteLookupOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found(_) => formatter.write_str("AdminRouteLookupOutcome::Found(<redacted>)"),
            Self::NotFound => formatter.write_str("AdminRouteLookupOutcome::NotFound"),
        }
    }
}

/// 路由更新结果。
pub enum AdminRouteMutationOutcome {
    /// 已更新并返回最新快照。
    Mutated(AdminRouteRecord),
    /// 路由不存在或已经软删除。
    NotFound,
}

impl fmt::Debug for AdminRouteMutationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mutated(_) => {
                formatter.write_str("AdminRouteMutationOutcome::Mutated(<redacted>)")
            }
            Self::NotFound => formatter.write_str("AdminRouteMutationOutcome::NotFound"),
        }
    }
}

/// 路由删除结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminRouteDeleteOutcome {
    /// 路由墓碑与候选已在同一事务内写入。
    Deleted,
    /// 路由不存在或已经软删除。
    NotFound,
}

/// 管理路由仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminRouteRepositoryConfigError {
    /// 零超时无法形成有效查询截止时间。
    #[error("管理路由查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 管理路由仓储内部错误；不携带模型名、URL 或凭据材料。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminRouteRepositoryError {
    /// 路由名称与当前有效路由冲突。
    #[error("管理路由名称冲突")]
    Conflict,
    /// 渠道或凭据不存在、已删除或归属不一致。
    #[error("管理路由候选引用无效")]
    InvalidReference,
    /// 数据库查询或事务提交失败。
    #[error("管理路由数据库操作失败")]
    Query,
    /// 查询超过配置硬截止时间。
    #[error("管理路由数据库操作超时")]
    Timeout,
    /// 持久化状态违反路由不变量。
    #[error("管理路由持久化状态损坏")]
    Invariant,
}

/// 管理端路由规则与候选的数据库仓储。
#[derive(Clone)]
pub struct AdminRouteRepository {
    pub(super) pool: DatabasePool,
    pub(super) lookup_timeout: Duration,
}

impl AdminRouteRepository {
    /// 使用共享连接池和查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, AdminRouteRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(AdminRouteRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按稳定路由 ID 游标读取一页未软删除路由。
    pub async fn list(
        &self,
        after: Option<RouteId>,
        limit: usize,
    ) -> Result<AdminRoutePageRecord, AdminRouteRepositoryError> {
        if !(1..=MAX_ADMIN_ROUTE_PAGE_SIZE).contains(&limit) {
            return Err(record_internal_error(AdminRouteRepositoryError::Invariant));
        }
        let operation = async {
            let mut query = routes::Entity::find()
                .filter(routes::Column::DeletedAt.is_null())
                .order_by_asc(routes::Column::Id)
                .limit((limit + 1) as u64);
            if let Some(after) = after {
                query = query.filter(routes::Column::Id.gt(after.get()));
            }
            query
                .all(self.pool.connection())
                .await
                .map_err(|_| AdminRouteRepositoryError::Query)
        }
        .with_subscriber(NoSubscriber::default());
        let mut models = match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error)?,
            Err(_) => return Err(record_internal_error(AdminRouteRepositoryError::Timeout)),
        };
        let has_more = models.len() > limit;
        if has_more {
            models.pop();
        }
        let mut records = Vec::with_capacity(models.len());
        for model in models {
            let route_id = RouteId::new(model.id).map_err(|_| internal_invariant())?;
            let channels = self.load_channels(self.pool.connection(), route_id).await?;
            records.push(AdminRouteRecord::try_from_models(model, channels)?);
        }
        let next_cursor = has_more
            .then(|| records.last().map(|record| record.route_id))
            .flatten();
        Ok(AdminRoutePageRecord {
            routes: records,
            next_cursor,
        })
    }

    /// 读取一个未软删除路由及其候选快照。
    pub async fn get(
        &self,
        route_id: RouteId,
    ) -> Result<AdminRouteLookupOutcome, AdminRouteRepositoryError> {
        let operation = async {
            routes::Entity::find_by_id(route_id.get())
                .filter(routes::Column::DeletedAt.is_null())
                .one(self.pool.connection())
                .await
                .map_err(|_| AdminRouteRepositoryError::Query)
        }
        .with_subscriber(NoSubscriber::default());
        let model = match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error)?,
            Err(_) => return Err(record_internal_error(AdminRouteRepositoryError::Timeout)),
        };
        let Some(model) = model else {
            return Ok(AdminRouteLookupOutcome::NotFound);
        };
        let channels = self.load_channels(self.pool.connection(), route_id).await?;
        Ok(AdminRouteLookupOutcome::Found(
            AdminRouteRecord::try_from_models(model, channels)?,
        ))
    }

    /// 在一个事务内创建路由规则和全部候选。
    pub async fn create(
        &self,
        record: AdminRouteWriteRecord,
    ) -> Result<AdminRouteRecord, AdminRouteRepositoryError> {
        validate_write_record(&record)?;
        match timeout(self.lookup_timeout, self.create_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminRouteRepositoryError::Timeout)),
        }
    }

    /// 在一个事务内完整更新路由字段并差量维护候选，保留未变更候选的统计。
    pub async fn update(
        &self,
        route_id: RouteId,
        record: AdminRouteWriteRecord,
    ) -> Result<AdminRouteMutationOutcome, AdminRouteRepositoryError> {
        validate_write_record(&record)?;
        match timeout(self.lookup_timeout, self.update_inner(route_id, record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminRouteRepositoryError::Timeout)),
        }
    }

    /// 软删除路由并清理其候选，避免墓碑继续参与运行时加载。
    pub async fn delete(
        &self,
        route_id: RouteId,
    ) -> Result<AdminRouteDeleteOutcome, AdminRouteRepositoryError> {
        match timeout(self.lookup_timeout, self.delete_inner(route_id)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminRouteRepositoryError::Timeout)),
        }
    }

    async fn create_inner(
        &self,
        record: AdminRouteWriteRecord,
    ) -> Result<AdminRouteRecord, AdminRouteRepositoryError> {
        let transaction = begin_transaction(self).await?;
        ensure_name_available(&transaction, &record.name, None).await?;
        validate_channel_references(&transaction, &record.channels).await?;
        let inserted = routes::ActiveModel {
            name: Set(record.name),
            model_pattern: Set(record.model_pattern),
            route_mode: Set(record.mode.code()),
            strategy: Set(record.strategy.code()),
            model_mapping: Set(record.model_mapping),
            enabled: Set(record.enabled),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
        insert_route_channels(&transaction, inserted.id, &record.channels).await?;
        let snapshot = fetch_route_snapshot(
            &transaction,
            RouteId::new(inserted.id).map_err(|_| internal_invariant())?,
        )
        .await?;
        commit_transaction(transaction).await?;
        Ok(snapshot)
    }

    async fn update_inner(
        &self,
        route_id: RouteId,
        record: AdminRouteWriteRecord,
    ) -> Result<AdminRouteMutationOutcome, AdminRouteRepositoryError> {
        let transaction = begin_transaction(self).await?;
        if !active_route_exists(&transaction, route_id).await? {
            return Ok(AdminRouteMutationOutcome::NotFound);
        }
        ensure_name_available(&transaction, &record.name, Some(route_id)).await?;
        validate_channel_references(&transaction, &record.channels).await?;
        routes::Entity::update_many()
            .filter(routes::Column::Id.eq(route_id.get()))
            .filter(routes::Column::DeletedAt.is_null())
            .col_expr(routes::Column::Name, Expr::value(record.name))
            .col_expr(
                routes::Column::ModelPattern,
                Expr::value(record.model_pattern),
            )
            .col_expr(routes::Column::RouteMode, Expr::value(record.mode.code()))
            .col_expr(
                routes::Column::Strategy,
                Expr::value(record.strategy.code()),
            )
            .col_expr(
                routes::Column::ModelMapping,
                Expr::value(record.model_mapping),
            )
            .col_expr(routes::Column::Enabled, Expr::value(record.enabled))
            .col_expr(
                routes::Column::UpdatedAt,
                Expr::value(TimeDateTimeWithTimeZone::now_utc()),
            )
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        sync_route_channels(&transaction, route_id, &record.channels).await?;
        let snapshot = fetch_route_snapshot(&transaction, route_id).await?;
        commit_transaction(transaction).await?;
        Ok(AdminRouteMutationOutcome::Mutated(snapshot))
    }

    async fn delete_inner(
        &self,
        route_id: RouteId,
    ) -> Result<AdminRouteDeleteOutcome, AdminRouteRepositoryError> {
        let transaction = begin_transaction(self).await?;
        if !active_route_exists(&transaction, route_id).await? {
            return Ok(AdminRouteDeleteOutcome::NotFound);
        }
        routes::Entity::update_many()
            .filter(routes::Column::Id.eq(route_id.get()))
            .filter(routes::Column::DeletedAt.is_null())
            .col_expr(
                routes::Column::DeletedAt,
                Expr::value(Some(TimeDateTimeWithTimeZone::now_utc())),
            )
            .col_expr(
                routes::Column::UpdatedAt,
                Expr::value(TimeDateTimeWithTimeZone::now_utc()),
            )
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        route_channels::Entity::delete_many()
            .filter(route_channels::Column::RouteId.eq(route_id.get()))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        commit_transaction(transaction).await?;
        Ok(AdminRouteDeleteOutcome::Deleted)
    }

    async fn load_channels<C: ConnectionTrait + Send + Sync>(
        &self,
        database: &C,
        route_id: RouteId,
    ) -> Result<Vec<route_channels::Model>, AdminRouteRepositoryError> {
        route_channels::Entity::find()
            .filter(route_channels::Column::RouteId.eq(route_id.get()))
            .order_by_asc(route_channels::Column::Priority)
            .order_by_asc(route_channels::Column::Id)
            .all(database)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(AdminRouteRepositoryError::Query))
    }
}

impl fmt::Debug for AdminRouteRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminRouteRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

impl AdminRouteRecord {
    fn try_from_models(
        model: routes::Model,
        channel_models: Vec<route_channels::Model>,
    ) -> Result<Self, AdminRouteRepositoryError> {
        let route_id = RouteId::new(model.id).map_err(|_| internal_invariant())?;
        let mode = RouteMode::try_from(model.route_mode).map_err(|_| internal_invariant())?;
        let strategy = RouteStrategy::try_from(model.strategy).map_err(|_| internal_invariant())?;
        if model.name.is_empty()
            || model.name.len() > MAX_ADMIN_ROUTE_NAME_BYTES
            || model.name.chars().any(char::is_control)
            || validate_route_model_pattern(&model.model_pattern).is_err()
            || model.model_pattern.trim() != model.model_pattern
            || (mode == RouteMode::ExplicitGroup && model.model_pattern.starts_with("re:"))
            || !validate_smart_route_model_mapping(&model.model_mapping)
            || channel_models.len() > MAX_ADMIN_ROUTE_CHANNELS
        {
            return Err(internal_invariant());
        }
        let mut channels = Vec::with_capacity(channel_models.len());
        let mut seen = HashSet::with_capacity(channel_models.len());
        for channel in channel_models {
            if channel.route_id != route_id.get()
                || channel.priority < 0
                || channel.weight < 0
                || channel.success_count < 0
                || channel.fail_count < 0
                || channel.total_latency < 0
                || !(0..=3).contains(&channel.cooldown_level)
            {
                return Err(internal_invariant());
            }
            let key = (channel.channel_id, channel.credential_id);
            if !seen.insert(key) {
                return Err(internal_invariant());
            }
            channels.push(AdminRouteChannelRecord {
                id: RouteChannelId::new(channel.id).map_err(|_| internal_invariant())?,
                channel_id: ChannelId::new(channel.channel_id).map_err(|_| internal_invariant())?,
                credential_id: CredentialId::new(channel.credential_id)
                    .map_err(|_| internal_invariant())?,
                priority: channel.priority,
                weight: channel.weight,
                enabled: channel.enabled,
                success_count: channel.success_count,
                fail_count: channel.fail_count,
                total_latency: channel.total_latency,
                cooldown_level: channel.cooldown_level,
                cooldown_until: channel.cooldown_until,
                last_selected_at: channel.last_selected_at,
                last_failure_at: channel.last_failure_at,
            });
        }
        Ok(Self {
            route_id,
            name: model.name,
            model_pattern: model.model_pattern,
            mode,
            strategy,
            model_mapping: model.model_mapping,
            enabled: model.enabled,
            channels,
        })
    }
}

fn validate_write_record(record: &AdminRouteWriteRecord) -> Result<(), AdminRouteRepositoryError> {
    if record.name.is_empty()
        || record.name.len() > MAX_ADMIN_ROUTE_NAME_BYTES
        || record.name.chars().any(char::is_control)
        || validate_route_model_pattern(&record.model_pattern).is_err()
        || record.model_pattern.trim() != record.model_pattern
        || (record.mode == RouteMode::ExplicitGroup && record.model_pattern.starts_with("re:"))
        || !validate_smart_route_model_mapping(&record.model_mapping)
        || record.channels.len() > MAX_ADMIN_ROUTE_CHANNELS
    {
        return Err(AdminRouteRepositoryError::Invariant);
    }
    let mut seen = HashSet::with_capacity(record.channels.len());
    for channel in &record.channels {
        if channel.priority < 0
            || channel.weight < 0
            || !seen.insert((channel.channel_id.get(), channel.credential_id.get()))
        {
            return Err(AdminRouteRepositoryError::Invariant);
        }
    }
    Ok(())
}

async fn begin_transaction(
    repository: &AdminRouteRepository,
) -> Result<DatabaseTransaction, AdminRouteRepositoryError> {
    repository
        .pool
        .connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminRouteRepositoryError::Query))
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), AdminRouteRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminRouteRepositoryError::Query))
}

async fn fetch_route_snapshot(
    transaction: &DatabaseTransaction,
    route_id: RouteId,
) -> Result<AdminRouteRecord, AdminRouteRepositoryError> {
    let model = routes::Entity::find_by_id(route_id.get())
        .filter(routes::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminRouteRepositoryError::Query))?
        .ok_or_else(internal_invariant)?;
    let channels = route_channels::Entity::find()
        .filter(route_channels::Column::RouteId.eq(route_id.get()))
        .order_by_asc(route_channels::Column::Priority)
        .order_by_asc(route_channels::Column::Id)
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminRouteRepositoryError::Query))?;
    AdminRouteRecord::try_from_models(model, channels)
}

async fn ensure_name_available(
    transaction: &DatabaseTransaction,
    name: &str,
    except_route_id: Option<RouteId>,
) -> Result<(), AdminRouteRepositoryError> {
    let mut query = routes::Entity::find()
        .filter(routes::Column::Name.eq(name))
        .filter(routes::Column::DeletedAt.is_null());
    if let Some(route_id) = except_route_id {
        query = query.filter(routes::Column::Id.ne(route_id.get()));
    }
    if query
        .select_only()
        .column(routes::Column::Id)
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminRouteRepositoryError::Query))?
        .is_some()
    {
        Err(AdminRouteRepositoryError::Conflict)
    } else {
        Ok(())
    }
}

async fn active_route_exists(
    transaction: &DatabaseTransaction,
    route_id: RouteId,
) -> Result<bool, AdminRouteRepositoryError> {
    Ok(routes::Entity::find_by_id(route_id.get())
        .filter(routes::Column::DeletedAt.is_null())
        .select_only()
        .column(routes::Column::Id)
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminRouteRepositoryError::Query))?
        .is_some())
}

async fn validate_channel_references(
    transaction: &DatabaseTransaction,
    desired: &[AdminRouteChannelWriteRecord],
) -> Result<(), AdminRouteRepositoryError> {
    if desired.is_empty() {
        return Ok(());
    }
    let channel_ids = desired
        .iter()
        .map(|item| item.channel_id.get())
        .collect::<Vec<_>>();
    let credential_ids = desired
        .iter()
        .map(|item| item.credential_id.get())
        .collect::<Vec<_>>();
    let channels = channels::Entity::find()
        .filter(channels::Column::Id.is_in(channel_ids))
        .filter(channels::Column::DeletedAt.is_null())
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminRouteRepositoryError::Query))?;
    let credentials = credentials::Entity::find()
        .filter(credentials::Column::Id.is_in(credential_ids))
        .filter(credentials::Column::DeletedAt.is_null())
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminRouteRepositoryError::Query))?;
    let channel_set = channels
        .into_iter()
        .map(|row| row.id)
        .collect::<HashSet<_>>();
    let credential_channels = credentials
        .into_iter()
        .map(|row| (row.id, row.channel_id))
        .collect::<HashMap<_, _>>();
    if desired.iter().any(|item| {
        !channel_set.contains(&item.channel_id.get())
            || credential_channels.get(&item.credential_id.get()) != Some(&item.channel_id.get())
    }) {
        return Err(AdminRouteRepositoryError::InvalidReference);
    }
    Ok(())
}

async fn insert_route_channels(
    transaction: &DatabaseTransaction,
    route_id: i64,
    desired: &[AdminRouteChannelWriteRecord],
) -> Result<(), AdminRouteRepositoryError> {
    if desired.is_empty() {
        return Ok(());
    }
    let rows = desired
        .iter()
        .map(|item| route_channels::ActiveModel {
            route_id: Set(route_id),
            channel_id: Set(item.channel_id.get()),
            credential_id: Set(item.credential_id.get()),
            priority: Set(item.priority),
            weight: Set(item.weight),
            enabled: Set(item.enabled),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    route_channels::Entity::insert_many(rows)
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)
        .map(|_| ())
}

async fn sync_route_channels(
    transaction: &DatabaseTransaction,
    route_id: RouteId,
    desired: &[AdminRouteChannelWriteRecord],
) -> Result<(), AdminRouteRepositoryError> {
    let existing = route_channels::Entity::find()
        .filter(route_channels::Column::RouteId.eq(route_id.get()))
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminRouteRepositoryError::Query))?;
    let mut existing_by_key = existing
        .into_iter()
        .map(|row| ((row.channel_id, row.credential_id), row))
        .collect::<HashMap<_, _>>();
    for item in desired {
        let key = (item.channel_id.get(), item.credential_id.get());
        if let Some(row) = existing_by_key.remove(&key) {
            route_channels::Entity::update_many()
                .filter(route_channels::Column::Id.eq(row.id))
                .col_expr(route_channels::Column::Priority, Expr::value(item.priority))
                .col_expr(route_channels::Column::Weight, Expr::value(item.weight))
                .col_expr(route_channels::Column::Enabled, Expr::value(item.enabled))
                .col_expr(
                    route_channels::Column::UpdatedAt,
                    Expr::value(TimeDateTimeWithTimeZone::now_utc()),
                )
                .exec(transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_write_db_error)?;
        } else {
            insert_route_channels(transaction, route_id.get(), std::slice::from_ref(item)).await?;
        }
    }
    let stale_ids = existing_by_key
        .values()
        .map(|row| row.id)
        .collect::<Vec<_>>();
    if !stale_ids.is_empty() {
        route_channels::Entity::delete_many()
            .filter(route_channels::Column::Id.is_in(stale_ids))
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
    }
    Ok(())
}

fn map_write_db_error(error: sea_orm::DbErr) -> AdminRouteRepositoryError {
    let rendered = error.to_string();
    if rendered.contains("uq_route_channels") {
        return AdminRouteRepositoryError::Conflict;
    }
    if rendered.contains("FOREIGN KEY") || rendered.contains("foreign key") {
        return AdminRouteRepositoryError::InvalidReference;
    }
    record_internal_error(AdminRouteRepositoryError::Query)
}

fn internal_invariant() -> AdminRouteRepositoryError {
    record_internal_error(AdminRouteRepositoryError::Invariant)
}

fn record_internal_error(error: AdminRouteRepositoryError) -> AdminRouteRepositoryError {
    let error_kind = match error {
        AdminRouteRepositoryError::Conflict => "admin_route_conflict",
        AdminRouteRepositoryError::InvalidReference => "admin_route_invalid_reference",
        AdminRouteRepositoryError::Query => "admin_route_query",
        AdminRouteRepositoryError::Timeout => "admin_route_timeout",
        AdminRouteRepositoryError::Invariant => "admin_route_invariant",
    };
    tracing::error!(target: "af_db::admin_route", error_kind, "管理路由仓储发生内部错误");
    error
}
