use std::{fmt, time::Duration};

use af_domain::{ChannelId, GroupId, Status};
use sea_orm::{
    ColumnTrait, EntityTrait, JoinType, QueryFilter, QueryOrder, QuerySelect, RelationTrait,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, SchedulerCatalogSubject,
    entity::{abilities, channels, groups},
    model_price::is_valid_model_name,
};

const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(5);
/// 单次调度查询允许返回的候选上限，与 relay 有界候选状态机保持一致。
pub const MAX_SCHEDULER_ABILITY_ENTRIES: usize = 64;
/// 一次全量渠道索引重建允许读取的能力总数，防止损坏数据造成无界内存占用。
pub const MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES: usize = 200_000;

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
    ) -> Result<Self, SchedulerAbilityRepositoryError> {
        let channel_status = Status::try_from(channel.status)
            .map_err(|_| SchedulerAbilityRepositoryError::Invariant)?;
        if ability.channel_id != channel.id
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
                    let record = SchedulerAbilityRecord::try_from_models(ability, channel)?;
                    if record.group_id() != group_id || record.model() != model {
                        return Err(SchedulerAbilityRepositoryError::Invariant);
                    }
                    Ok(record)
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

    /// 全量读取所有有效能力，供进程内渠道索引原子重建。
    ///
    /// 单个 `(group_id, model)` 仍受 64 个候选硬上限约束，但不同键之间不会共享
    /// 全局行数上限；调用方必须先完整构建新快照，成功后再替换旧快照。
    pub async fn load_all(
        &self,
    ) -> Result<Vec<SchedulerAbilityRecord>, SchedulerAbilityRepositoryError> {
        self.load_snapshot(None).await
    }

    /// 按渠道或分组主体读取当前全部有效能力，供事件驱动投影做局部重建。
    pub async fn load_subject(
        &self,
        subject: SchedulerCatalogSubject,
    ) -> Result<Vec<SchedulerAbilityRecord>, SchedulerAbilityRepositoryError> {
        self.load_snapshot(Some(subject)).await
    }

    async fn load_snapshot(
        &self,
        subject: Option<SchedulerCatalogSubject>,
    ) -> Result<Vec<SchedulerAbilityRecord>, SchedulerAbilityRepositoryError> {
        let operation = async {
            let limit = snapshot_query_limit()?;
            let mut query = abilities::Entity::find()
                .find_also_related(channels::Entity)
                .join(JoinType::InnerJoin, abilities::Relation::Group.def())
                .filter(abilities::Column::Enabled.eq(true))
                .filter(channels::Column::Status.eq(Status::Enabled.code()))
                .filter(channels::Column::DeletedAt.is_null())
                .filter(groups::Column::DeletedAt.is_null());
            query = match subject {
                Some(SchedulerCatalogSubject::Channel(channel_id)) => {
                    query.filter(abilities::Column::ChannelId.eq(channel_id.get()))
                }
                Some(SchedulerCatalogSubject::Group(group_id)) => {
                    query.filter(abilities::Column::GroupId.eq(group_id.get()))
                }
                None => query,
            };
            let rows = query
                .order_by_asc(abilities::Column::GroupId)
                .order_by_asc(abilities::Column::Model)
                .order_by_desc(abilities::Column::Priority)
                .order_by_asc(abilities::Column::ChannelId)
                .limit(limit)
                .all(self.pool.connection())
                .await
                .map_err(|_| SchedulerAbilityRepositoryError::Query)?;
            if rows.len() > MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES {
                return Err(SchedulerAbilityRepositoryError::Invariant);
            }

            let mut records = Vec::with_capacity(rows.len());
            let mut current_group_id = None;
            let mut current_model = None::<String>;
            let mut current_count = 0_usize;
            for (ability, channel) in rows {
                let channel = channel.ok_or(SchedulerAbilityRepositoryError::Invariant)?;
                let record = SchedulerAbilityRecord::try_from_models(ability, channel)?;
                if subject.is_some_and(|subject| !record.matches_subject(subject)) {
                    return Err(SchedulerAbilityRepositoryError::Invariant);
                }
                if current_group_id == Some(record.group_id())
                    && current_model.as_deref() == Some(record.model())
                {
                    current_count = current_count
                        .checked_add(1)
                        .ok_or(SchedulerAbilityRepositoryError::Invariant)?;
                } else {
                    current_group_id = Some(record.group_id());
                    current_model = Some(record.model().to_owned());
                    current_count = 1;
                }
                if current_count > MAX_SCHEDULER_ABILITY_ENTRIES {
                    return Err(SchedulerAbilityRepositoryError::Invariant);
                }
                records.push(record);
            }
            Ok(records)
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

impl SchedulerAbilityRecord {
    pub(crate) fn from_projection_parts(
        group_id: GroupId,
        model: String,
        channel_id: ChannelId,
        priority: i32,
        weight: u32,
    ) -> Result<Self, SchedulerAbilityRepositoryError> {
        if !is_valid_model_name(&model) {
            return Err(SchedulerAbilityRepositoryError::Invariant);
        }
        Ok(Self {
            group_id,
            model,
            channel_id,
            priority,
            weight,
        })
    }

    fn matches_subject(&self, subject: SchedulerCatalogSubject) -> bool {
        match subject {
            SchedulerCatalogSubject::Channel(channel_id) => self.channel_id == channel_id,
            SchedulerCatalogSubject::Group(group_id) => self.group_id == group_id,
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

fn snapshot_query_limit() -> Result<u64, SchedulerAbilityRepositoryError> {
    u64::try_from(MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES)
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
