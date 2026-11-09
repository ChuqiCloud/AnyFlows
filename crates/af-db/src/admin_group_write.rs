use std::fmt;

use af_domain::{GroupId, GroupPeakSchedule};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, IsolationLevel, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::{TimeDateTimeWithTimeZone, TimeTime},
    sea_query::{Alias, Condition, Expr, LockType, Query, SelectStatement},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminGroupRecord, AdminGroupRepository, AdminGroupRepositoryError,
    ability_write::{AbilityWriteError, delete_group_abilities},
    admin_group::record_internal_error,
    entity::{
        authentication_settings, channel_groups, group_group_grants, group_key_policies,
        group_model_policies, group_model_ratio_overrides, group_model_ratios, groups, tokens,
        user_group_grants, user_route_health, user_route_members, user_routes, users,
    },
    group_key_access::lock_active_group,
};

/// 已由应用层校验的高峰倍率窗口。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AdminGroupPeakWriteRecord {
    ratio_micros: i64,
    start_second: u32,
    end_second: u32,
}

impl AdminGroupPeakWriteRecord {
    /// 组装使用午夜起整秒表示的高峰窗口。
    #[must_use]
    pub const fn new(ratio_micros: i64, start_second: u32, end_second: u32) -> Self {
        Self {
            ratio_micros,
            start_second,
            end_second,
        }
    }
}

impl fmt::Debug for AdminGroupPeakWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminGroupPeakWriteRecord(<redacted>)")
    }
}

/// 管理端创建或完整更新分组时写入的业务字段。
pub struct AdminGroupWriteRecord {
    name: String,
    display_name: String,
    ratio_micros: i64,
    peak: Option<AdminGroupPeakWriteRecord>,
    peak_schedule: Option<GroupPeakSchedule>,
    is_exclusive: bool,
    daily_limit: Option<i64>,
    weekly_limit: Option<i64>,
    monthly_limit: Option<i64>,
    rpm_limit: Option<i32>,
    fallback_group_id: Option<GroupId>,
    flags: serde_json::Value,
}

impl AdminGroupWriteRecord {
    /// 组装已经由应用层完成公开边界校验的写入记录。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端分组写入契约一一对应"
    )]
    #[must_use]
    pub fn new(
        name: String,
        display_name: String,
        ratio_micros: i64,
        peak: Option<AdminGroupPeakWriteRecord>,
        is_exclusive: bool,
        daily_limit: Option<i64>,
        weekly_limit: Option<i64>,
        monthly_limit: Option<i64>,
        rpm_limit: Option<i32>,
        fallback_group_id: Option<GroupId>,
        flags: serde_json::Value,
    ) -> Self {
        Self {
            name,
            display_name,
            ratio_micros,
            peak,
            peak_schedule: None,
            is_exclusive,
            daily_limit,
            weekly_limit,
            monthly_limit,
            rpm_limit,
            fallback_group_id,
            flags,
        }
    }

    /// 设置新周计划；未设置时更新操作保留数据库现有计划。
    #[must_use]
    pub fn with_peak_schedule(mut self, schedule: Option<GroupPeakSchedule>) -> Self {
        self.peak_schedule = schedule;
        self
    }
}

impl fmt::Debug for AdminGroupWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminGroupWriteRecord(<redacted>)")
    }
}

/// 分组更新结果；不存在和已软删除统一视为未找到。
pub enum AdminGroupMutationOutcome {
    /// 分组已更新，并返回最新管理快照。
    Mutated(Box<AdminGroupRecord>),
    /// 分组不存在或已经软删除。
    NotFound,
}

impl fmt::Debug for AdminGroupMutationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mutated(_) => {
                formatter.write_str("AdminGroupMutationOutcome::Mutated(<redacted>)")
            }
            Self::NotFound => formatter.write_str("AdminGroupMutationOutcome::NotFound"),
        }
    }
}

/// 分组软删除结果；重复删除不会伪装成成功。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminGroupDeleteOutcome {
    /// 分组及可安全解除的直接运行时关系已在同一事务内删除。
    Deleted,
    /// 分组不存在或已经软删除。
    NotFound,
}

impl AdminGroupRepository {
    /// 创建一个未软删除分组并返回管理快照。
    pub async fn create(
        &self,
        record: AdminGroupWriteRecord,
    ) -> Result<AdminGroupRecord, AdminGroupRepositoryError> {
        match timeout(self.lookup_timeout, self.create_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminGroupRepositoryError::Timeout)),
        }
    }

    /// 完整更新一个未软删除分组并返回管理快照。
    pub async fn update(
        &self,
        group_id: GroupId,
        record: AdminGroupWriteRecord,
    ) -> Result<AdminGroupMutationOutcome, AdminGroupRepositoryError> {
        match timeout(self.lookup_timeout, self.update_inner(group_id, record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminGroupRepositoryError::Timeout)),
        }
    }

    /// 软删除分组；有效用户或令牌仍引用时拒绝执行。
    pub async fn delete(
        &self,
        group_id: GroupId,
    ) -> Result<AdminGroupDeleteOutcome, AdminGroupRepositoryError> {
        self.delete_with_replacement(group_id, None).await
    }

    /// 显式迁移用户默认分组并撤销直接绑定 Key，再软删除原分组。
    pub async fn delete_with_replacement(
        &self,
        group_id: GroupId,
        replacement_group_id: Option<GroupId>,
    ) -> Result<AdminGroupDeleteOutcome, AdminGroupRepositoryError> {
        match timeout(
            self.lookup_timeout,
            self.delete_inner(group_id, replacement_group_id),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminGroupRepositoryError::Timeout)),
        }
    }

    async fn create_inner(
        &self,
        record: AdminGroupWriteRecord,
    ) -> Result<AdminGroupRecord, AdminGroupRepositoryError> {
        let transaction = begin_transaction(self).await?;
        ensure_name_available(&transaction, &record.name, None).await?;
        ensure_fallback_exists(&transaction, record.fallback_group_id).await?;
        let (peak_ratio_micros, peak_start, peak_end) = peak_columns(record.peak)?;
        let peak_windows = encode_peak_schedule(record.peak_schedule.as_ref())?;
        let inserted = groups::ActiveModel {
            name: Set(record.name),
            display_name: Set(record.display_name),
            ratio_micros: Set(record.ratio_micros),
            peak_ratio_micros: Set(peak_ratio_micros),
            peak_start: Set(peak_start),
            peak_end: Set(peak_end),
            peak_windows: Set(peak_windows),
            is_exclusive: Set(record.is_exclusive),
            daily_limit: Set(record.daily_limit),
            weekly_limit: Set(record.weekly_limit),
            monthly_limit: Set(record.monthly_limit),
            rpm_limit: Set(record.rpm_limit),
            fallback_group_id: Set(record.fallback_group_id.map(GroupId::get)),
            flags: Set(record.flags),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
        let snapshot = AdminGroupRecord::try_from_model(inserted)?;
        commit_transaction(transaction).await?;
        Ok(snapshot)
    }

    async fn update_inner(
        &self,
        group_id: GroupId,
        record: AdminGroupWriteRecord,
    ) -> Result<AdminGroupMutationOutcome, AdminGroupRepositoryError> {
        if record.fallback_group_id == Some(group_id) {
            return Err(AdminGroupRepositoryError::InvalidReference);
        }
        let transaction = begin_transaction(self).await?;
        if !active_group_exists(&transaction, group_id).await? {
            return Ok(AdminGroupMutationOutcome::NotFound);
        }
        ensure_name_available(&transaction, &record.name, Some(group_id)).await?;
        ensure_fallback_exists(&transaction, record.fallback_group_id).await?;
        let (peak_ratio_micros, peak_start, peak_end) = peak_columns(record.peak)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let mut update = groups::Entity::update_many()
            .filter(groups::Column::Id.eq(group_id.get()))
            .filter(groups::Column::DeletedAt.is_null())
            .col_expr(groups::Column::Name, Expr::value(record.name))
            .col_expr(
                groups::Column::DisplayName,
                Expr::value(record.display_name),
            )
            .col_expr(
                groups::Column::RatioMicros,
                Expr::value(record.ratio_micros),
            )
            .col_expr(
                groups::Column::PeakRatioMicros,
                Expr::value(peak_ratio_micros),
            )
            .col_expr(groups::Column::PeakStart, Expr::value(peak_start))
            .col_expr(groups::Column::PeakEnd, Expr::value(peak_end))
            .col_expr(
                groups::Column::IsExclusive,
                Expr::value(record.is_exclusive),
            )
            .col_expr(groups::Column::DailyLimit, Expr::value(record.daily_limit))
            .col_expr(
                groups::Column::WeeklyLimit,
                Expr::value(record.weekly_limit),
            )
            .col_expr(
                groups::Column::MonthlyLimit,
                Expr::value(record.monthly_limit),
            )
            .col_expr(groups::Column::RpmLimit, Expr::value(record.rpm_limit))
            .col_expr(
                groups::Column::FallbackGroupId,
                Expr::value(record.fallback_group_id.map(GroupId::get)),
            )
            .col_expr(groups::Column::Flags, Expr::value(record.flags))
            .col_expr(groups::Column::UpdatedAt, Expr::value(now));
        if record.peak_schedule.is_some() {
            update = update.col_expr(
                groups::Column::PeakWindows,
                Expr::value(encode_peak_schedule(record.peak_schedule.as_ref())?),
            );
        }
        let result = update
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        if result.rows_affected != 1 {
            return Err(record_internal_error(AdminGroupRepositoryError::Invariant));
        }
        let snapshot = fetch_group_snapshot(&transaction, group_id).await?;
        commit_transaction(transaction).await?;
        Ok(AdminGroupMutationOutcome::Mutated(Box::new(snapshot)))
    }

    async fn delete_inner(
        &self,
        group_id: GroupId,
        replacement_group_id: Option<GroupId>,
    ) -> Result<AdminGroupDeleteOutcome, AdminGroupRepositoryError> {
        for _ in 0..3 {
            if let Some(outcome) = self.delete_attempt(group_id, replacement_group_id).await? {
                return Ok(outcome);
            }
            tokio::task::yield_now().await;
        }
        Err(record_internal_error(AdminGroupRepositoryError::Query))
    }

    async fn delete_attempt(
        &self,
        group_id: GroupId,
        replacement_group_id: Option<GroupId>,
    ) -> Result<Option<AdminGroupDeleteOutcome>, AdminGroupRepositoryError> {
        let transaction = begin_delete_transaction(self).await?;
        if transaction.get_database_backend() == DbBackend::Sqlite
            && !lock_active_group(&transaction, group_id)
                .await
                .map_err(map_write_db_error)?
        {
            return Ok(Some(AdminGroupDeleteOutcome::NotFound));
        }
        let owners = deletion_owner_ids(&transaction, group_id).await?;
        let mut owner_query = users::Entity::find()
            .select_only()
            .column(users::Column::Id)
            .filter(users::Column::Id.is_in(owners))
            .order_by_asc(users::Column::Id);
        if transaction.get_database_backend() != DbBackend::Sqlite {
            owner_query = owner_query.lock(LockType::Update);
        }
        let locked_owners = owner_query
            .into_tuple::<i64>()
            .all(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        // Key and route mutations lock their owner before any group. Lock all
        // affected owners first, then group IDs in order for reciprocal deletes.
        let mut groups_to_lock = vec![group_id];
        if let Some(replacement) = replacement_group_id {
            groups_to_lock.push(replacement);
        }
        groups_to_lock.sort_by_key(|id| id.get());
        groups_to_lock.dedup();
        for id in groups_to_lock {
            if !lock_active_group(&transaction, id)
                .await
                .map_err(map_write_db_error)?
            {
                return if id == group_id {
                    Ok(Some(AdminGroupDeleteOutcome::NotFound))
                } else {
                    Err(AdminGroupRepositoryError::InvalidReference)
                };
            }
        }
        // A user or route may have bound the group between discovery and locking.
        // Under READ COMMITTED this non-locking recheck sees that committed change
        // without waiting on a new owner that is itself waiting for our group lock.
        if deletion_owner_ids(&transaction, group_id)
            .await?
            .iter()
            .any(|id| locked_owners.binary_search(id).is_err())
        {
            transaction
                .rollback()
                .await
                .map_err(|_| record_internal_error(AdminGroupRepositoryError::Query))?;
            return Ok(None);
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = groups::Entity::update_many()
            .filter(groups::Column::Id.eq(group_id.get()))
            .filter(groups::Column::DeletedAt.is_null())
            .col_expr(groups::Column::DeletedAt, Expr::value(now))
            .col_expr(groups::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        if result.rows_affected == 0 {
            return Ok(Some(AdminGroupDeleteOutcome::NotFound));
        }
        if result.rows_affected != 1 {
            return Err(record_internal_error(AdminGroupRepositoryError::Invariant));
        }

        // 先写墓碑再检查引用，事务回滚可保证失败时不暴露半删除状态。
        if has_merchant_pool(&transaction, group_id, false).await? {
            return Err(AdminGroupRepositoryError::InUse);
        }
        if let Some(replacement) = replacement_group_id {
            migrate_group_owners(&transaction, group_id, replacement, now).await?;
        }
        if has_blocking_reference(&transaction, group_id).await? {
            return Err(AdminGroupRepositoryError::InUse);
        }
        clear_safe_references(&transaction, group_id, now).await?;
        commit_transaction(transaction).await?;
        Ok(Some(AdminGroupDeleteOutcome::Deleted))
    }
}

fn encode_peak_schedule(
    schedule: Option<&GroupPeakSchedule>,
) -> Result<Option<serde_json::Value>, AdminGroupRepositoryError> {
    schedule
        .map(|schedule| {
            serde_json::to_value(schedule.windows())
                .map_err(|_| AdminGroupRepositoryError::Invariant)
        })
        .transpose()
}

async fn begin_delete_transaction(
    repository: &AdminGroupRepository,
) -> Result<DatabaseTransaction, AdminGroupRepositoryError> {
    let connection = repository.pool.connection();
    let result = if connection.get_database_backend() == DbBackend::Sqlite {
        connection
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
    } else {
        connection
            .begin_with_config(Some(IsolationLevel::ReadCommitted), None)
            .with_subscriber(NoSubscriber::default())
            .await
    };
    result.map_err(|_| record_internal_error(AdminGroupRepositoryError::Query))
}

async fn deletion_owner_ids(
    transaction: &DatabaseTransaction,
    group_id: GroupId,
) -> Result<Vec<i64>, AdminGroupRepositoryError> {
    let mut owners = users::Entity::find()
        .select_only()
        .column(users::Column::Id)
        .filter(users::Column::DefaultGroupId.eq(group_id.get()))
        .filter(users::Column::DeletedAt.is_null())
        .into_tuple::<i64>()
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    let route_owners = Query::select()
        .expr_as(
            Expr::col((user_routes::Entity, user_routes::Column::OwnerUserId)),
            Alias::new("owner_id"),
        )
        .from(user_routes::Entity)
        .inner_join(
            user_route_members::Entity,
            Expr::col((user_routes::Entity, user_routes::Column::Id)).equals((
                user_route_members::Entity,
                user_route_members::Column::RouteId,
            )),
        )
        .and_where(
            Expr::col((
                user_route_members::Entity,
                user_route_members::Column::GroupId,
            ))
            .eq(group_id.get()),
        )
        .and_where(Expr::col((user_routes::Entity, user_routes::Column::DeletedAt)).is_null())
        .to_owned();
    let rows = transaction
        .query_all(transaction.get_database_backend().build(&route_owners))
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    for row in rows {
        owners.push(row.try_get("", "owner_id").map_err(map_write_db_error)?);
    }
    owners.sort_unstable();
    owners.dedup();
    Ok(owners)
}

async fn begin_transaction(
    repository: &AdminGroupRepository,
) -> Result<DatabaseTransaction, AdminGroupRepositoryError> {
    repository
        .pool
        .connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminGroupRepositoryError::Query))
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), AdminGroupRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminGroupRepositoryError::Query))
}

async fn fetch_group_snapshot(
    transaction: &DatabaseTransaction,
    group_id: GroupId,
) -> Result<AdminGroupRecord, AdminGroupRepositoryError> {
    let model = groups::Entity::find_by_id(group_id.get())
        .filter(groups::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminGroupRepositoryError::Query))?
        .ok_or_else(|| record_internal_error(AdminGroupRepositoryError::Invariant))?;
    AdminGroupRecord::try_from_model(model)
}

async fn ensure_name_available(
    transaction: &DatabaseTransaction,
    name: &str,
    except_group_id: Option<GroupId>,
) -> Result<(), AdminGroupRepositoryError> {
    let mut query = Query::select();
    query
        .expr_as(
            Expr::col((groups::Entity, groups::Column::Id)),
            Alias::new("id"),
        )
        .from(groups::Entity)
        .and_where(Expr::col((groups::Entity, groups::Column::Name)).eq(name))
        .and_where(Expr::col((groups::Entity, groups::Column::DeletedAt)).is_null())
        .limit(1);
    if let Some(group_id) = except_group_id {
        query.and_where(Expr::col((groups::Entity, groups::Column::Id)).ne(group_id.get()));
    }
    if query_exists(transaction, query.to_owned()).await? {
        Err(AdminGroupRepositoryError::Conflict)
    } else {
        Ok(())
    }
}

async fn ensure_fallback_exists(
    transaction: &DatabaseTransaction,
    fallback_group_id: Option<GroupId>,
) -> Result<(), AdminGroupRepositoryError> {
    let Some(fallback_group_id) = fallback_group_id else {
        return Ok(());
    };
    if active_group_exists(transaction, fallback_group_id).await? {
        Ok(())
    } else {
        Err(AdminGroupRepositoryError::InvalidReference)
    }
}

async fn active_group_exists(
    transaction: &DatabaseTransaction,
    group_id: GroupId,
) -> Result<bool, AdminGroupRepositoryError> {
    let query = Query::select()
        .expr_as(
            Expr::col((groups::Entity, groups::Column::Id)),
            Alias::new("id"),
        )
        .from(groups::Entity)
        .and_where(Expr::col((groups::Entity, groups::Column::Id)).eq(group_id.get()))
        .and_where(Expr::col((groups::Entity, groups::Column::DeletedAt)).is_null())
        .limit(1)
        .to_owned();
    current_query_exists(transaction, query).await
}

async fn has_merchant_pool(
    transaction: &DatabaseTransaction,
    group_id: GroupId,
    include_deleted: bool,
) -> Result<bool, AdminGroupRepositoryError> {
    let mut query = Query::select()
        .column(Alias::new("id"))
        .from(Alias::new("merchant_pools"))
        .and_where(Expr::col(Alias::new("group_id")).eq(group_id.get()))
        .limit(1)
        .to_owned();
    if !include_deleted {
        query.and_where(Expr::col(Alias::new("deleted_at")).is_null());
    }
    current_query_exists(transaction, query).await
}

async fn migrate_group_owners(
    transaction: &DatabaseTransaction,
    source: GroupId,
    replacement: GroupId,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AdminGroupRepositoryError> {
    if source == replacement {
        return Err(AdminGroupRepositoryError::InvalidReference);
    }
    if !lock_active_group(transaction, replacement)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?
        || has_merchant_pool(transaction, replacement, true).await?
    {
        return Err(AdminGroupRepositoryError::InvalidReference);
    }
    users::Entity::update_many()
        .filter(users::Column::DefaultGroupId.eq(source.get()))
        .filter(users::Column::DeletedAt.is_null())
        .col_expr(
            users::Column::DefaultGroupId,
            Expr::value(replacement.get()),
        )
        .col_expr(users::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    // Preserve the explicit group binding on revoked keys: clearing it would silently
    // grant access through the owner's replacement/default group.
    tokens::Entity::update_many()
        .filter(tokens::Column::GroupId.eq(source.get()))
        .filter(tokens::Column::DeletedAt.is_null())
        .col_expr(tokens::Column::Status, Expr::value(2_i16))
        .col_expr(tokens::Column::DeletedAt, Expr::value(now))
        .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    authentication_settings::Entity::update_many()
        .filter(authentication_settings::Column::RegistrationDefaultGroupId.eq(source.get()))
        .filter(authentication_settings::Column::Version.lt(i64::MAX))
        .col_expr(
            authentication_settings::Column::RegistrationDefaultGroupId,
            Expr::value(replacement.get()),
        )
        .col_expr(
            authentication_settings::Column::Version,
            Expr::col(authentication_settings::Column::Version).add(1_i64),
        )
        .col_expr(authentication_settings::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    Ok(())
}

async fn has_blocking_reference(
    transaction: &DatabaseTransaction,
    group_id: GroupId,
) -> Result<bool, AdminGroupRepositoryError> {
    let active_user = Query::select()
        .expr_as(
            Expr::col((users::Entity, users::Column::Id)),
            Alias::new("id"),
        )
        .from(users::Entity)
        .and_where(Expr::col((users::Entity, users::Column::DefaultGroupId)).eq(group_id.get()))
        .and_where(Expr::col((users::Entity, users::Column::DeletedAt)).is_null())
        .limit(1)
        .to_owned();
    if current_query_exists(transaction, active_user).await? {
        return Ok(true);
    }
    let active_token = Query::select()
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Id)),
            Alias::new("id"),
        )
        .from(tokens::Entity)
        .and_where(Expr::col((tokens::Entity, tokens::Column::GroupId)).eq(group_id.get()))
        .and_where(Expr::col((tokens::Entity, tokens::Column::DeletedAt)).is_null())
        .limit(1)
        .to_owned();
    if current_query_exists(transaction, active_token).await? {
        return Ok(true);
    }
    let registration_default = Query::select()
        .column(authentication_settings::Column::Id)
        .from(authentication_settings::Entity)
        .and_where(
            Expr::col(authentication_settings::Column::RegistrationDefaultGroupId)
                .eq(group_id.get()),
        )
        .limit(1)
        .to_owned();
    current_query_exists(transaction, registration_default).await
}

async fn clear_safe_references(
    transaction: &DatabaseTransaction,
    group_id: GroupId,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AdminGroupRepositoryError> {
    groups::Entity::update_many()
        .filter(groups::Column::FallbackGroupId.eq(group_id.get()))
        .col_expr(groups::Column::FallbackGroupId, Expr::value(None::<i64>))
        .col_expr(groups::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    delete_group_abilities(transaction, group_id, now)
        .await
        .map_err(map_ability_write_error)?;
    channel_groups::Entity::delete_many()
        .filter(channel_groups::Column::GroupId.eq(group_id.get()))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    group_model_ratios::Entity::delete_many()
        .filter(
            Condition::any()
                .add(group_model_ratios::Column::SourceGroupId.eq(group_id.get()))
                .add(group_model_ratios::Column::TargetGroupId.eq(group_id.get())),
        )
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    group_model_ratio_overrides::Entity::delete_many()
        .filter(
            Condition::any()
                .add(group_model_ratio_overrides::Column::SourceGroupId.eq(group_id.get()))
                .add(group_model_ratio_overrides::Column::TargetGroupId.eq(group_id.get())),
        )
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    group_group_grants::Entity::delete_many()
        .filter(
            Condition::any()
                .add(group_group_grants::Column::SourceGroupId.eq(group_id.get()))
                .add(group_group_grants::Column::TargetGroupId.eq(group_id.get())),
        )
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    user_group_grants::Entity::delete_many()
        .filter(user_group_grants::Column::GroupId.eq(group_id.get()))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    group_key_policies::Entity::delete_by_id(group_id.get())
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    group_model_policies::Entity::delete_by_id(group_id.get())
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    let affected_routes = user_route_members::Entity::find()
        .filter(user_route_members::Column::GroupId.eq(group_id.get()))
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    for member in &affected_routes {
        let mut query = user_routes::Entity::find_by_id(member.route_id);
        if transaction.get_database_backend() != DbBackend::Sqlite {
            query = query.lock(LockType::Update);
        }
        query
            .one(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
    }
    user_route_members::Entity::delete_many()
        .filter(user_route_members::Column::GroupId.eq(group_id.get()))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    user_route_health::Entity::delete_many()
        .filter(user_route_health::Column::GroupId.eq(group_id.get()))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
    for member in affected_routes {
        let mut remaining = user_route_members::Entity::find()
            .filter(user_route_members::Column::RouteId.eq(member.route_id));
        if transaction.get_database_backend() != DbBackend::Sqlite {
            remaining = remaining.lock(LockType::Update);
        }
        if remaining
            .one(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?
            .is_none()
        {
            user_routes::Entity::update_many()
                .filter(user_routes::Column::Id.eq(member.route_id))
                .filter(user_routes::Column::DeletedAt.is_null())
                .col_expr(user_routes::Column::Enabled, Expr::value(false))
                .col_expr(user_routes::Column::UpdatedAt, Expr::value(now))
                .exec(transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_write_db_error)?;
        }
    }
    Ok(())
}

fn map_ability_write_error(error: AbilityWriteError) -> AdminGroupRepositoryError {
    match error {
        AbilityWriteError::Query => record_internal_error(AdminGroupRepositoryError::Query),
        AbilityWriteError::InvalidInput
        | AbilityWriteError::InvalidReference
        | AbilityWriteError::CapacityExceeded
        | AbilityWriteError::Invariant => {
            record_internal_error(AdminGroupRepositoryError::Invariant)
        }
    }
}

async fn current_query_exists(
    transaction: &DatabaseTransaction,
    mut query: SelectStatement,
) -> Result<bool, AdminGroupRepositoryError> {
    // MySQL's default REPEATABLE READ must inspect the state after acquired row
    // locks, including a pool or owner that committed while deletion was waiting.
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query.lock(LockType::Update);
    }
    query_exists(transaction, query).await
}

async fn query_exists(
    transaction: &DatabaseTransaction,
    query: SelectStatement,
) -> Result<bool, AdminGroupRepositoryError> {
    let statement = transaction.get_database_backend().build(&query);
    let results = transaction
        .query_all(statement)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminGroupRepositoryError::Query))?;
    Ok(!results.is_empty())
}

fn peak_columns(
    peak: Option<AdminGroupPeakWriteRecord>,
) -> Result<(Option<i64>, Option<TimeTime>, Option<TimeTime>), AdminGroupRepositoryError> {
    let Some(peak) = peak else {
        return Ok((None, None, None));
    };
    let start = second_to_time(peak.start_second)?;
    let end = second_to_time(peak.end_second)?;
    if peak.ratio_micros < 0 || start == end {
        return Err(record_internal_error(AdminGroupRepositoryError::Invariant));
    }
    Ok((Some(peak.ratio_micros), Some(start), Some(end)))
}

fn second_to_time(second: u32) -> Result<TimeTime, AdminGroupRepositoryError> {
    if second >= 24 * 60 * 60 {
        return Err(record_internal_error(AdminGroupRepositoryError::Invariant));
    }
    TimeTime::from_hms(
        (second / 3_600) as u8,
        ((second % 3_600) / 60) as u8,
        (second % 60) as u8,
    )
    .map_err(|_| record_internal_error(AdminGroupRepositoryError::Invariant))
}

fn map_write_db_error(error: DbErr) -> AdminGroupRepositoryError {
    let rendered = error.to_string();
    if rendered.contains("uq_groups_active_name")
        || rendered.contains("_active_name")
        || rendered.contains("groups.name")
    {
        return AdminGroupRepositoryError::Conflict;
    }
    if rendered.contains("FOREIGN KEY") || rendered.contains("foreign key") {
        return AdminGroupRepositoryError::InvalidReference;
    }
    record_internal_error(AdminGroupRepositoryError::Query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_records_redact_business_configuration() {
        let peak = AdminGroupPeakWriteRecord::new(1_500_000, 8 * 3_600, 20 * 3_600);
        let record = AdminGroupWriteRecord::new(
            "private-group".to_owned(),
            "Private Group".to_owned(),
            1_000_000,
            Some(peak),
            false,
            None,
            None,
            None,
            None,
            None,
            serde_json::json!({"private_switch": true}),
        );
        assert_eq!(format!("{peak:?}"), "AdminGroupPeakWriteRecord(<redacted>)");
        assert_eq!(format!("{record:?}"), "AdminGroupWriteRecord(<redacted>)");
    }
}
