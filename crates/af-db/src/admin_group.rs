use std::{fmt, time::Duration};

use af_domain::{GroupId, SubscriptionCycle, SubscriptionWindow};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, entity::groups};

/// 单页分组查询允许返回的最大记录数。
pub const MAX_ADMIN_GROUP_PAGE_SIZE: usize = 100;
/// 分组开关 JSON 编码后的最大字节数。
pub const MAX_ADMIN_GROUP_FLAGS_BYTES: usize = 16 * 1024;

/// 已校验的分组高峰倍率窗口。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AdminGroupPeakRecord {
    ratio_micros: i64,
    start_second: u32,
    end_second: u32,
}

impl AdminGroupPeakRecord {
    /// 返回高峰倍率的百万分比整数值。
    #[must_use]
    pub const fn ratio_micros(self) -> i64 {
        self.ratio_micros
    }

    /// 返回高峰窗口起点，单位为午夜起整秒数。
    #[must_use]
    pub const fn start_second(self) -> u32 {
        self.start_second
    }

    /// 返回高峰窗口终点，单位为午夜起整秒数。
    #[must_use]
    pub const fn end_second(self) -> u32 {
        self.end_second
    }
}

impl fmt::Debug for AdminGroupPeakRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminGroupPeakRecord(<redacted>)")
    }
}

/// 管理端可读取的单个 UTC 日历额度窗口快照。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AdminGroupWindowRecord {
    usage: i64,
    started_at: i64,
    resets_at: i64,
}

impl AdminGroupWindowRecord {
    /// 返回当前窗口内已结算的共享额度。
    #[must_use]
    pub const fn usage(self) -> i64 {
        self.usage
    }

    /// 返回当前窗口的 UTC Unix 秒起点。
    #[must_use]
    pub const fn started_at(self) -> i64 {
        self.started_at
    }

    /// 返回当前窗口的 UTC Unix 秒重置时间。
    #[must_use]
    pub const fn resets_at(self) -> i64 {
        self.resets_at
    }
}

impl fmt::Debug for AdminGroupWindowRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminGroupWindowRecord(<redacted>)")
    }
}

/// 管理端可读取的已校验分组快照。
pub struct AdminGroupRecord {
    group_id: GroupId,
    name: String,
    display_name: String,
    ratio_micros: i64,
    peak: Option<AdminGroupPeakRecord>,
    is_exclusive: bool,
    daily_limit: Option<i64>,
    weekly_limit: Option<i64>,
    monthly_limit: Option<i64>,
    daily_window: AdminGroupWindowRecord,
    weekly_window: AdminGroupWindowRecord,
    monthly_window: AdminGroupWindowRecord,
    rpm_limit: Option<i32>,
    fallback_group_id: Option<GroupId>,
    flags: serde_json::Value,
}

impl AdminGroupRecord {
    /// 返回分组主键。
    #[must_use]
    pub const fn group_id(&self) -> GroupId {
        self.group_id
    }

    /// 返回稳定分组名。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回分组显示名。
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 返回基础倍率的百万分比整数值。
    #[must_use]
    pub const fn ratio_micros(&self) -> i64 {
        self.ratio_micros
    }

    /// 返回可选高峰倍率窗口。
    #[must_use]
    pub const fn peak(&self) -> Option<AdminGroupPeakRecord> {
        self.peak
    }

    /// 返回是否为独占分组。
    #[must_use]
    pub const fn is_exclusive(&self) -> bool {
        self.is_exclusive
    }

    /// 返回每日额度上限。
    #[must_use]
    pub const fn daily_limit(&self) -> Option<i64> {
        self.daily_limit
    }

    /// 返回每周额度上限。
    #[must_use]
    pub const fn weekly_limit(&self) -> Option<i64> {
        self.weekly_limit
    }

    /// 返回每月额度上限。
    #[must_use]
    pub const fn monthly_limit(&self) -> Option<i64> {
        self.monthly_limit
    }

    /// 返回按 UTC 自然日累计的共享额度窗口。
    #[must_use]
    pub const fn daily_window(&self) -> AdminGroupWindowRecord {
        self.daily_window
    }

    /// 返回按 UTC 周一开始自然周累计的共享额度窗口。
    #[must_use]
    pub const fn weekly_window(&self) -> AdminGroupWindowRecord {
        self.weekly_window
    }

    /// 返回按 UTC 自然月累计的共享额度窗口。
    #[must_use]
    pub const fn monthly_window(&self) -> AdminGroupWindowRecord {
        self.monthly_window
    }

    /// 返回分组级 RPM 限制。
    #[must_use]
    pub const fn rpm_limit(&self) -> Option<i32> {
        self.rpm_limit
    }

    /// 返回客户端限制降级目标。
    #[must_use]
    pub const fn fallback_group_id(&self) -> Option<GroupId> {
        self.fallback_group_id
    }

    /// 返回已校验的分组开关对象。
    #[must_use]
    pub const fn flags(&self) -> &serde_json::Value {
        &self.flags
    }

    pub(super) fn try_from_model(model: groups::Model) -> Result<Self, AdminGroupRepositoryError> {
        let now = TimeDateTimeWithTimeZone::now_utc();
        let group_id = GroupId::new(model.id).map_err(|_| internal_invariant())?;
        let valid_name = valid_text(&model.name, 64);
        let valid_display_name = valid_text(&model.display_name, 128);
        let peak = match (model.peak_ratio_micros, model.peak_start, model.peak_end) {
            (None, None, None) => None,
            (Some(ratio_micros), Some(start), Some(end)) if ratio_micros >= 0 => {
                let start_second = second_of_day(start);
                let end_second = second_of_day(end);
                if start_second == end_second {
                    return Err(internal_invariant());
                }
                Some(AdminGroupPeakRecord {
                    ratio_micros,
                    start_second,
                    end_second,
                })
            }
            _ => return Err(internal_invariant()),
        };
        let fallback_group_id = model
            .fallback_group_id
            .map(GroupId::new)
            .transpose()
            .map_err(|_| internal_invariant())?;
        let valid_flags = model.flags.is_object()
            && serde_json::to_vec(&model.flags)
                .is_ok_and(|encoded| encoded.len() <= MAX_ADMIN_GROUP_FLAGS_BYTES);
        let daily_window = normalize_window_record(
            SubscriptionCycle::Daily,
            model.daily_usage,
            model.daily_window_start,
            now,
        )?;
        let weekly_window = normalize_window_record(
            SubscriptionCycle::Weekly,
            model.weekly_usage,
            model.weekly_window_start,
            now,
        )?;
        let monthly_window = normalize_window_record(
            SubscriptionCycle::Monthly,
            model.monthly_usage,
            model.monthly_window_start,
            now,
        )?;
        if !valid_name
            || !valid_display_name
            || model.ratio_micros < 0
            || [model.daily_limit, model.weekly_limit, model.monthly_limit]
                .into_iter()
                .flatten()
                .any(|value| value < 0)
            || model.rpm_limit.is_some_and(|value| value < 0)
            || fallback_group_id == Some(group_id)
            || !valid_flags
        {
            return Err(internal_invariant());
        }
        Ok(Self {
            group_id,
            name: model.name,
            display_name: model.display_name,
            ratio_micros: model.ratio_micros,
            peak,
            is_exclusive: model.is_exclusive,
            daily_limit: model.daily_limit,
            weekly_limit: model.weekly_limit,
            monthly_limit: model.monthly_limit,
            daily_window,
            weekly_window,
            monthly_window,
            rpm_limit: model.rpm_limit,
            fallback_group_id,
            flags: model.flags,
        })
    }
}

fn normalize_window_record(
    cycle: SubscriptionCycle,
    usage: i64,
    started_at: TimeDateTimeWithTimeZone,
    now: TimeDateTimeWithTimeZone,
) -> Result<AdminGroupWindowRecord, AdminGroupRepositoryError> {
    if usage < 0 || started_at.unix_timestamp() < 0 || started_at > now {
        return Err(internal_invariant());
    }
    let now_seconds = u64::try_from(now.unix_timestamp()).map_err(|_| internal_invariant())?;
    let observed_seconds =
        u64::try_from(started_at.unix_timestamp()).map_err(|_| internal_invariant())?;
    let current =
        SubscriptionWindow::initial(cycle, now_seconds).map_err(|_| internal_invariant())?;
    let observed =
        SubscriptionWindow::initial(cycle, observed_seconds).map_err(|_| internal_invariant())?;
    let observed_start = TimeDateTimeWithTimeZone::from_unix_timestamp(
        i64::try_from(observed.started_at()).map_err(|_| internal_invariant())?,
    )
    .map_err(|_| internal_invariant())?;
    let noncanonical = observed_start != started_at;
    if noncanonical && usage != 0 {
        return Err(internal_invariant());
    }
    let current_started_at =
        i64::try_from(current.started_at()).map_err(|_| internal_invariant())?;
    let observed_started_at =
        i64::try_from(observed.started_at()).map_err(|_| internal_invariant())?;
    if observed_started_at > current_started_at {
        return Err(internal_invariant());
    }
    let stale = noncanonical || observed_started_at < current_started_at;
    Ok(AdminGroupWindowRecord {
        usage: if stale { 0 } else { usage },
        started_at: current_started_at,
        resets_at: i64::try_from(current.ends_at()).map_err(|_| internal_invariant())?,
    })
}

impl fmt::Debug for AdminGroupRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminGroupRecord(<redacted>)")
    }
}

/// 一页有界分组结果。
pub struct AdminGroupPageRecord {
    groups: Vec<AdminGroupRecord>,
    next_cursor: Option<GroupId>,
}

impl AdminGroupPageRecord {
    /// 消费页面并返回分组记录和下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<AdminGroupRecord>, Option<GroupId>) {
        (self.groups, self.next_cursor)
    }
}

impl fmt::Debug for AdminGroupPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminGroupPageRecord(<redacted>)")
    }
}

/// 分组详情查询结果。
pub enum AdminGroupLookupOutcome {
    /// 找到当前未软删除分组。
    Found(Box<AdminGroupRecord>),
    /// 分组不存在或已经软删除。
    NotFound,
}

impl fmt::Debug for AdminGroupLookupOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found(_) => formatter.write_str("AdminGroupLookupOutcome::Found(<redacted>)"),
            Self::NotFound => formatter.write_str("AdminGroupLookupOutcome::NotFound"),
        }
    }
}

/// 管理分组仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminGroupRepositoryConfigError {
    /// 零超时无法形成有效的数据库查询截止时间。
    #[error("管理分组查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 管理分组仓储内部错误；不携带分组字段或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminGroupRepositoryError {
    /// 分组名与当前有效分组冲突。
    #[error("管理分组名称冲突")]
    Conflict,
    /// 回退分组不存在、已软删除或引用自身。
    #[error("管理分组引用无效")]
    InvalidReference,
    /// 分组仍被有效用户或有效令牌引用，不能安全删除。
    #[error("管理分组仍在使用")]
    InUse,
    /// 获取连接或执行查询失败。
    #[error("管理分组数据库查询失败")]
    Query,
    /// 查询超过配置的硬截止时间。
    #[error("管理分组数据库查询超时")]
    Timeout,
    /// 查询输入或持久化结果违反不变量。
    #[error("管理分组持久化状态损坏")]
    Invariant,
}

/// 管理端分组列表与详情共用的只读仓储。
#[derive(Clone)]
pub struct AdminGroupRepository {
    pub(super) pool: DatabasePool,
    pub(super) lookup_timeout: Duration,
}

impl AdminGroupRepository {
    /// 使用共享数据库连接池和单次查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, AdminGroupRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(AdminGroupRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按单调分组 ID 游标读取一页未软删除分组。
    pub async fn list(
        &self,
        after: Option<GroupId>,
        limit: usize,
    ) -> Result<AdminGroupPageRecord, AdminGroupRepositoryError> {
        if !(1..=MAX_ADMIN_GROUP_PAGE_SIZE).contains(&limit) {
            return Err(record_internal_error(AdminGroupRepositoryError::Invariant));
        }
        let operation = async {
            let mut query = groups::Entity::find()
                .filter(groups::Column::DeletedAt.is_null())
                .order_by_asc(groups::Column::Id)
                .limit((limit + 1) as u64);
            if let Some(after) = after {
                query = query.filter(groups::Column::Id.gt(after.get()));
            }
            query
                .all(self.pool.connection())
                .await
                .map_err(|_| AdminGroupRepositoryError::Query)
        }
        .with_subscriber(NoSubscriber::default());
        let mut models = match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error)?,
            Err(_) => return Err(record_internal_error(AdminGroupRepositoryError::Timeout)),
        };
        let has_more = models.len() > limit;
        if has_more {
            models.truncate(limit);
        }
        let groups = models
            .into_iter()
            .map(AdminGroupRecord::try_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| groups.last().map(AdminGroupRecord::group_id))
            .flatten();
        Ok(AdminGroupPageRecord {
            groups,
            next_cursor,
        })
    }

    /// 按稳定分组 ID 查询当前未软删除分组。
    pub async fn get(
        &self,
        group_id: GroupId,
    ) -> Result<AdminGroupLookupOutcome, AdminGroupRepositoryError> {
        let operation = groups::Entity::find()
            .filter(groups::Column::Id.eq(group_id.get()))
            .filter(groups::Column::DeletedAt.is_null())
            .limit(2)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default());
        let mut models = match timeout(self.lookup_timeout, operation).await {
            Ok(Ok(models)) => models,
            Ok(Err(_)) => return Err(record_internal_error(AdminGroupRepositoryError::Query)),
            Err(_) => return Err(record_internal_error(AdminGroupRepositoryError::Timeout)),
        };
        match models.len() {
            0 => Ok(AdminGroupLookupOutcome::NotFound),
            1 => Ok(AdminGroupLookupOutcome::Found(Box::new(
                AdminGroupRecord::try_from_model(models.pop().ok_or_else(internal_invariant)?)?,
            ))),
            _ => Err(internal_invariant()),
        }
    }
}

impl fmt::Debug for AdminGroupRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminGroupRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}

fn second_of_day(time: sea_orm::entity::prelude::TimeTime) -> u32 {
    u32::from(time.hour()) * 3_600 + u32::from(time.minute()) * 60 + u32::from(time.second())
}

fn internal_invariant() -> AdminGroupRepositoryError {
    record_internal_error(AdminGroupRepositoryError::Invariant)
}

pub(super) fn record_internal_error(error: AdminGroupRepositoryError) -> AdminGroupRepositoryError {
    let error_kind = match error {
        AdminGroupRepositoryError::Conflict => "admin_group_conflict",
        AdminGroupRepositoryError::InvalidReference => "admin_group_invalid_reference",
        AdminGroupRepositoryError::InUse => "admin_group_in_use",
        AdminGroupRepositoryError::Query => "admin_group_query",
        AdminGroupRepositoryError::Timeout => "admin_group_timeout",
        AdminGroupRepositoryError::Invariant => "admin_group_invariant",
    };
    tracing::error!(
        target: "af_db::admin_group",
        error_kind,
        "管理分组仓储发生内部错误"
    );
    error
}
