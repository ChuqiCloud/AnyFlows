use std::{collections::BTreeSet, fmt, time::Duration};

use af_domain::GroupId;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{group_model_ratios, groups},
};

const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(5);
/// 单次快照允许加载的有效分组上限。
pub const MAX_GROUP_PRICING_ENTRIES: usize = 100_000;
/// 单次快照允许加载的分组间附加倍率上限。
pub const MAX_GROUP_MODEL_RATIO_ENTRIES: usize = 200_000;

/// 已校验的高峰倍率窗口，时间使用从午夜开始的整秒数。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct GroupPeakPricingRecord {
    ratio_micros: i64,
    start_second: u32,
    end_second: u32,
}

impl GroupPeakPricingRecord {
    /// 返回高峰时段额外应用的百万分比倍率。
    #[must_use]
    pub const fn ratio_micros(self) -> i64 {
        self.ratio_micros
    }

    /// 返回高峰窗口起点，单位为从午夜开始的整秒数。
    #[must_use]
    pub const fn start_second(self) -> u32 {
        self.start_second
    }

    /// 返回高峰窗口终点，单位为从午夜开始的整秒数。
    #[must_use]
    pub const fn end_second(self) -> u32 {
        self.end_second
    }
}

impl fmt::Debug for GroupPeakPricingRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GroupPeakPricingRecord(<redacted>)")
    }
}

/// 已通过持久化边界校验的单个有效分组计费配置。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct GroupPricingRecord {
    group_id: GroupId,
    ratio_micros: i64,
    peak: Option<GroupPeakPricingRecord>,
}

impl GroupPricingRecord {
    /// 返回有效分组标识。
    #[must_use]
    pub const fn group_id(self) -> GroupId {
        self.group_id
    }

    /// 返回分组基础倍率的百万分比整数值。
    #[must_use]
    pub const fn ratio_micros(self) -> i64 {
        self.ratio_micros
    }

    /// 返回可选高峰倍率窗口。
    #[must_use]
    pub const fn peak(self) -> Option<GroupPeakPricingRecord> {
        self.peak
    }

    fn try_from_model(model: groups::Model) -> Result<Self, GroupPricingRepositoryError> {
        let group_id =
            GroupId::new(model.id).map_err(|_| GroupPricingRepositoryError::Invariant)?;
        if model.ratio_micros < 0 {
            return Err(GroupPricingRepositoryError::Invariant);
        }
        let peak = match (model.peak_ratio_micros, model.peak_start, model.peak_end) {
            (None, None, None) => None,
            (Some(ratio_micros), Some(start), Some(end)) if ratio_micros >= 0 => {
                let start_second = second_of_day(start);
                let end_second = second_of_day(end);
                if start_second == end_second {
                    return Err(GroupPricingRepositoryError::Invariant);
                }
                Some(GroupPeakPricingRecord {
                    ratio_micros,
                    start_second,
                    end_second,
                })
            }
            _ => return Err(GroupPricingRepositoryError::Invariant),
        };
        Ok(Self {
            group_id,
            ratio_micros: model.ratio_micros,
            peak,
        })
    }
}

impl fmt::Debug for GroupPricingRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GroupPricingRecord")
            .field("group_id", &self.group_id)
            .field("has_peak", &self.peak.is_some())
            .finish_non_exhaustive()
    }
}

/// 已校验的来源分组到实际计费分组附加倍率。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct GroupModelRatioRecord {
    source_group_id: GroupId,
    target_group_id: GroupId,
    ratio_micros: i64,
}

impl GroupModelRatioRecord {
    /// 返回用户原始所属分组。
    #[must_use]
    pub const fn source_group_id(self) -> GroupId {
        self.source_group_id
    }

    /// 返回请求最终实际使用的计费分组。
    #[must_use]
    pub const fn target_group_id(self) -> GroupId {
        self.target_group_id
    }

    /// 返回附加倍率的百万分比整数值。
    #[must_use]
    pub const fn ratio_micros(self) -> i64 {
        self.ratio_micros
    }

    fn try_from_model(
        model: group_model_ratios::Model,
        active_groups: &BTreeSet<GroupId>,
    ) -> Result<Self, GroupPricingRepositoryError> {
        let source_group_id = GroupId::new(model.source_group_id)
            .map_err(|_| GroupPricingRepositoryError::Invariant)?;
        let target_group_id = GroupId::new(model.target_group_id)
            .map_err(|_| GroupPricingRepositoryError::Invariant)?;
        if model.ratio_micros < 0
            || !active_groups.contains(&source_group_id)
            || !active_groups.contains(&target_group_id)
        {
            return Err(GroupPricingRepositoryError::Invariant);
        }
        Ok(Self {
            source_group_id,
            target_group_id,
            ratio_micros: model.ratio_micros,
        })
    }
}

impl fmt::Debug for GroupModelRatioRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GroupModelRatioRecord")
            .field("source_group_id", &self.source_group_id)
            .field("target_group_id", &self.target_group_id)
            .finish_non_exhaustive()
    }
}

/// 一次完整读取后得到的有效分组与附加倍率目录。
#[derive(Clone, Eq, PartialEq)]
pub struct GroupPricingCatalog {
    groups: Vec<GroupPricingRecord>,
    group_model_ratios: Vec<GroupModelRatioRecord>,
}

impl GroupPricingCatalog {
    /// 返回按分组标识稳定排序的有效分组配置。
    #[must_use]
    pub fn groups(&self) -> &[GroupPricingRecord] {
        &self.groups
    }

    /// 返回按来源、目标分组稳定排序的附加倍率。
    #[must_use]
    pub fn group_model_ratios(&self) -> &[GroupModelRatioRecord] {
        &self.group_model_ratios
    }
}

impl fmt::Debug for GroupPricingCatalog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GroupPricingCatalog")
            .field("group_count", &self.groups.len())
            .field("group_model_ratio_count", &self.group_model_ratios.len())
            .finish()
    }
}

/// 分组计费目录读取错误；不携带分组标识或倍率值。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum GroupPricingRepositoryError {
    /// 查询截止时间配置为零。
    #[error("分组计费查询超时必须大于零")]
    InvalidConfiguration,
    /// 获取连接或读取完整目录失败。
    #[error("读取分组计费目录失败")]
    Query,
    /// 完整目录读取超过硬截止时间。
    #[error("读取分组计费目录超时")]
    Timeout,
    /// 行数、分组引用、倍率或高峰窗口违反持久化不变量。
    #[error("分组计费持久化状态损坏")]
    Invariant,
}

/// 有界读取完整分组计费目录的数据库仓储。
#[derive(Clone)]
pub struct GroupPricingRepository {
    pool: DatabasePool,
    load_timeout: Duration,
}

impl GroupPricingRepository {
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
    ) -> Result<Self, GroupPricingRepositoryError> {
        if load_timeout.is_zero() {
            return Err(GroupPricingRepositoryError::InvalidConfiguration);
        }
        Ok(Self { pool, load_timeout })
    }

    /// 读取一个按主键稳定排序且内部引用完整的分组计费目录。
    pub async fn load_snapshot(&self) -> Result<GroupPricingCatalog, GroupPricingRepositoryError> {
        let operation = async {
            let group_limit = query_limit(MAX_GROUP_PRICING_ENTRIES)?;
            let ratio_limit = query_limit(MAX_GROUP_MODEL_RATIO_ENTRIES)?;
            let group_models = groups::Entity::find()
                .filter(groups::Column::DeletedAt.is_null())
                .order_by_asc(groups::Column::Id)
                .limit(group_limit)
                .all(self.pool.connection())
                .await
                .map_err(|_| GroupPricingRepositoryError::Query)?;
            if group_models.len() > MAX_GROUP_PRICING_ENTRIES {
                return Err(GroupPricingRepositoryError::Invariant);
            }
            let groups = group_models
                .into_iter()
                .map(GroupPricingRecord::try_from_model)
                .collect::<Result<Vec<_>, _>>()?;
            let active_groups = groups
                .iter()
                .map(|record| record.group_id())
                .collect::<BTreeSet<_>>();

            let ratio_models = group_model_ratios::Entity::find()
                .order_by_asc(group_model_ratios::Column::SourceGroupId)
                .order_by_asc(group_model_ratios::Column::TargetGroupId)
                .limit(ratio_limit)
                .all(self.pool.connection())
                .await
                .map_err(|_| GroupPricingRepositoryError::Query)?;
            if ratio_models.len() > MAX_GROUP_MODEL_RATIO_ENTRIES {
                return Err(GroupPricingRepositoryError::Invariant);
            }
            let group_model_ratios = ratio_models
                .into_iter()
                .map(|model| GroupModelRatioRecord::try_from_model(model, &active_groups))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(GroupPricingCatalog {
                groups,
                group_model_ratios,
            })
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.load_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(GroupPricingRepositoryError::Timeout)),
        }
    }
}

impl fmt::Debug for GroupPricingRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GroupPricingRepository")
            .field("load_timeout", &self.load_timeout)
            .finish_non_exhaustive()
    }
}

fn query_limit(maximum: usize) -> Result<u64, GroupPricingRepositoryError> {
    u64::try_from(maximum)
        .ok()
        .and_then(|limit| limit.checked_add(1))
        .ok_or(GroupPricingRepositoryError::Invariant)
}

fn second_of_day(time: sea_orm::entity::prelude::TimeTime) -> u32 {
    u32::from(time.hour()) * 3_600 + u32::from(time.minute()) * 60 + u32::from(time.second())
}

fn record_internal_error(error: GroupPricingRepositoryError) -> GroupPricingRepositoryError {
    let error_kind = match error {
        GroupPricingRepositoryError::InvalidConfiguration => return error,
        GroupPricingRepositoryError::Query => "group_pricing_query",
        GroupPricingRepositoryError::Timeout => "group_pricing_timeout",
        GroupPricingRepositoryError::Invariant => "group_pricing_invariant",
    };
    tracing::error!(
        target: "af_db::group_pricing",
        error_kind,
        "分组计费仓储发生内部错误"
    );
    error
}
