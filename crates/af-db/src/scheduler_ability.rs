use std::{fmt, time::Duration};

use af_domain::{ChannelId, GroupId, Status};
use sea_orm::{
    ColumnTrait, EntityTrait, JoinType, QueryFilter, QueryOrder, QuerySelect, RelationTrait,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{abilities, channels, groups},
    model_price::is_valid_model_name,
};

const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(5);
/// 单次调度查询允许返回的候选上限，与 relay 有界候选状态机保持一致。
pub const MAX_SCHEDULER_ABILITY_ENTRIES: usize = 64;

/// 已通过数据库读取边界校验的调度能力候选。
#[derive(Clone, Eq, PartialEq)]
pub struct SchedulerAbilityRecord {
    group_id: GroupId,
    model: String,
    channel_id: ChannelId,
    priority: i32,
    weight: u32,
}

impl SchedulerAbilityRecord {
    /// 返回能力项所属的有效分组。
    #[must_use]
    pub const fn group_id(&self) -> GroupId {
        self.group_id
    }

    /// 返回 Canonical 模型名；调用方不得直接写入日志或响应。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回该能力项指向的渠道。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回优先级；数值越大越先参与调度。
    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }

    /// 返回非负权重；基础概率由调度策略另行叠加。
    #[must_use]
    pub const fn weight(&self) -> u32 {
        self.weight
    }

    fn try_from_models(
        ability: abilities::Model,
        channel: channels::Model,
        expected_group_id: GroupId,
        expected_model: &str,
    ) -> Result<Self, SchedulerAbilityRepositoryError> {
        let channel_status = Status::try_from(channel.status)
            .map_err(|_| SchedulerAbilityRepositoryError::Invariant)?;
        if ability.group_id != expected_group_id.get()
            || ability.model != expected_model
            || ability.channel_id != channel.id
            || !ability.enabled
            || !channel_status.is_enabled()
            || channel.deleted_at.is_some()
            || !is_valid_model_name(&ability.model)
        {
            return Err(SchedulerAbilityRepositoryError::Invariant);
        }
        let group_id = GroupId::new(ability.group_id)
            .map_err(|_| SchedulerAbilityRepositoryError::Invariant)?;
        let channel_id = ChannelId::new(ability.channel_id)
            .map_err(|_| SchedulerAbilityRepositoryError::Invariant)?;
        let weight = u32::try_from(ability.weight)
            .map_err(|_| SchedulerAbilityRepositoryError::Invariant)?;
        Ok(Self {
            group_id,
            model: ability.model,
            channel_id,
            priority: ability.priority,
            weight,
        })
    }
}

impl fmt::Debug for SchedulerAbilityRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerAbilityRecord")
            .field("group_id", &self.group_id)
            .field("channel_id", &self.channel_id)
            .field("priority", &self.priority)
            .field("weight", &self.weight)
            .finish()
    }
}

/// 调度能力读取失败；错误不携带模型名、渠道名或数据库诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SchedulerAbilityRepositoryError {
    /// 查询截止时间配置为零。
    #[error("调度能力查询超时必须大于零")]
    InvalidConfiguration,
    /// 查询模型名不是受限 Canonical 名称。
    #[error("调度能力查询模型名无效")]
    InvalidModel,
    /// 获取连接或读取能力列表失败。
    #[error("读取调度能力失败")]
    Query,
    /// 能力读取超过硬截止时间。
    #[error("读取调度能力超时")]
    Timeout,
    /// 行数、标识、权重、模型名或渠道状态违反持久化不变量。
    #[error("调度能力持久化状态损坏")]
    Invariant,
}

/// 按 `(group_id, model)` 有界读取可调度能力的数据库仓储。
#[derive(Clone)]
pub struct SchedulerAbilityRepository {
    pool: DatabasePool,
    load_timeout: Duration,
}

impl SchedulerAbilityRepository {
    /// 使用默认五秒截止时间创建仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            load_timeout: DEFAULT_LOAD_TIMEOUT,
        }
    }

    /// 使用显式非零截止时间创建仓储。
    pub fn with_load_timeout(
        pool: DatabasePool,
        load_timeout: Duration,
    ) -> Result<Self, SchedulerAbilityRepositoryError> {
        if load_timeout.is_zero() {
            return Err(SchedulerAbilityRepositoryError::InvalidConfiguration);
        }
        Ok(Self { pool, load_timeout })
    }

    /// 读取当前分组与模型下启用、渠道启用且未软删除的能力候选。
    pub async fn load(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<Vec<SchedulerAbilityRecord>, SchedulerAbilityRepositoryError> {
        if !is_valid_model_name(model) {
            return Err(SchedulerAbilityRepositoryError::InvalidModel);
        }
        let operation = async {
            let limit = query_limit()?;
            let rows = abilities::Entity::find()
                .find_also_related(channels::Entity)
                .join(JoinType::InnerJoin, abilities::Relation::Group.def())
                .filter(abilities::Column::GroupId.eq(group_id.get()))
                .filter(abilities::Column::Model.eq(model))
                .filter(abilities::Column::Enabled.eq(true))
                .filter(channels::Column::Status.eq(Status::Enabled.code()))
                .filter(channels::Column::DeletedAt.is_null())
                .filter(groups::Column::DeletedAt.is_null())
                .order_by_desc(abilities::Column::Priority)
                .order_by_asc(abilities::Column::ChannelId)
                .limit(limit)
                .all(self.pool.connection())
                .await
                .map_err(|_| SchedulerAbilityRepositoryError::Query)?;
            if rows.len() > MAX_SCHEDULER_ABILITY_ENTRIES {
                return Err(SchedulerAbilityRepositoryError::Invariant);
            }
            rows.into_iter()
                .map(|(ability, channel)| {
                    let channel = channel.ok_or(SchedulerAbilityRepositoryError::Invariant)?;
                    SchedulerAbilityRecord::try_from_models(ability, channel, group_id, model)
                })
                .collect()
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.load_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                SchedulerAbilityRepositoryError::Timeout,
            )),
        }
    }
}

impl fmt::Debug for SchedulerAbilityRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerAbilityRepository")
            .field("load_timeout", &self.load_timeout)
            .finish_non_exhaustive()
    }
}

fn query_limit() -> Result<u64, SchedulerAbilityRepositoryError> {
    u64::try_from(MAX_SCHEDULER_ABILITY_ENTRIES)
        .ok()
        .and_then(|limit| limit.checked_add(1))
        .ok_or(SchedulerAbilityRepositoryError::Invariant)
}

fn record_internal_error(
    error: SchedulerAbilityRepositoryError,
) -> SchedulerAbilityRepositoryError {
    let error_kind = match error {
        SchedulerAbilityRepositoryError::InvalidConfiguration
        | SchedulerAbilityRepositoryError::InvalidModel => return error,
        SchedulerAbilityRepositoryError::Query => "scheduler_ability_query",
        SchedulerAbilityRepositoryError::Timeout => "scheduler_ability_timeout",
        SchedulerAbilityRepositoryError::Invariant => "scheduler_ability_invariant",
    };
    tracing::error!(
        target: "af_db::scheduler_ability",
        error_kind,
        "调度能力仓储发生内部错误"
    );
    error
}
