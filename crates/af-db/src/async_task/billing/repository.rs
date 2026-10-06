use std::{fmt, time::Duration};

use af_domain::{AsyncTaskId, BillingReservationId, GroupId, Quota, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    async_task::{AsyncTaskRepositoryConfigError, AsyncTaskRepositoryError},
    entity::{
        BillingReservationKey, SensitiveString, async_task_billings, async_task_submission_claims,
        groups,
    },
};

use super::types::{
    AsyncTaskBillingAccept, AsyncTaskBillingClear, AsyncTaskBillingMark,
    AsyncTaskBillingMutationOutcome, AsyncTaskBillingPlan, AsyncTaskBillingPlanOutcome,
    AsyncTaskBillingRecord, AsyncTaskBillingResolution, AsyncTaskBillingSettlement,
    AsyncTaskBillingState,
};

/// 以乐观锁维护异步任务冻结、结算和释放事实的仓储。
#[derive(Clone)]
pub struct AsyncTaskBillingRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl AsyncTaskBillingRepository {
    /// 使用共享连接池和严格正数操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, AsyncTaskRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(AsyncTaskRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 创建提交前冻结计划；同一任务只允许完全相同的计划重放。
    pub async fn plan(
        &self,
        write: AsyncTaskBillingPlan,
    ) -> Result<AsyncTaskBillingPlanOutcome, AsyncTaskRepositoryError> {
        let operation =
            plan(self.pool.connection(), &write).with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(AsyncTaskRepositoryError::OutcomeUnknown),
        }
    }

    /// 在用户范围内读取计费快照。
    pub async fn find(
        &self,
        user_id: UserId,
        task_id: AsyncTaskId,
    ) -> Result<Option<AsyncTaskBillingRecord>, AsyncTaskRepositoryError> {
        let operation =
            load(self.pool.connection(), user_id, task_id).with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(AsyncTaskRepositoryError::Timeout),
        }
    }

    /// 确认同一预留计划已经完成持久化冻结。
    pub async fn mark_reserved(
        &self,
        write: AsyncTaskBillingMark,
    ) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError> {
        self.transition(
            write,
            &[AsyncTaskBillingState::Planned],
            AsyncTaskBillingState::Reserved,
        )
        .await
    }

    /// 固化已接受上游模型的官方单价和提交 fallback。
    pub async fn accept(
        &self,
        write: AsyncTaskBillingAccept,
    ) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError> {
        let operation =
            accept(self.pool.connection(), &write).with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(AsyncTaskRepositoryError::OutcomeUnknown),
        }
    }

    /// 在调用额度结算端口前固化实际额度与上游真实时长。
    pub async fn begin_settlement(
        &self,
        write: AsyncTaskBillingSettlement,
    ) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError> {
        let operation = begin_settlement(self.pool.connection(), &write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(AsyncTaskRepositoryError::OutcomeUnknown),
        }
    }

    /// 确认数据库额度预留已经按固化实际额度结算。
    pub async fn mark_settled(
        &self,
        write: AsyncTaskBillingMark,
    ) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError> {
        self.transition(
            write,
            &[AsyncTaskBillingState::SettlementPending],
            AsyncTaskBillingState::Settled,
        )
        .await
    }

    /// 在释放持久化额度前固化释放意图。
    pub async fn begin_release(
        &self,
        write: AsyncTaskBillingMark,
    ) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError> {
        self.transition(
            write,
            &[
                AsyncTaskBillingState::Reserved,
                AsyncTaskBillingState::Submitted,
            ],
            AsyncTaskBillingState::ReleasePending,
        )
        .await
    }

    /// 确认数据库额度预留已经完成失败释放。
    pub async fn mark_released(
        &self,
        write: AsyncTaskBillingMark,
    ) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError> {
        self.transition(
            write,
            &[AsyncTaskBillingState::ReleasePending],
            AsyncTaskBillingState::Released,
        )
        .await
    }

    /// 删除确定未提交且额度已释放的计划，使同一幂等请求可生成新的预留标识。
    pub async fn clear_released(
        &self,
        write: AsyncTaskBillingClear,
    ) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError> {
        let operation =
            clear_released(self.pool.connection(), &write).with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(AsyncTaskRepositoryError::OutcomeUnknown),
        }
    }

    async fn transition(
        &self,
        write: AsyncTaskBillingMark,
        sources: &[AsyncTaskBillingState],
        target: AsyncTaskBillingState,
    ) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError> {
        let operation = transition(self.pool.connection(), &write, sources, target)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(AsyncTaskRepositoryError::OutcomeUnknown),
        }
    }
}

impl fmt::Debug for AsyncTaskBillingRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsyncTaskBillingRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn plan<C>(
    connection: &C,
    write: &AsyncTaskBillingPlan,
) -> Result<AsyncTaskBillingPlanOutcome, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    if let Some(existing) = load(connection, write.user_id, write.task_id).await? {
        return if existing.matches_plan(write) {
            Ok(AsyncTaskBillingPlanOutcome::Existing(existing))
        } else {
            Err(AsyncTaskRepositoryError::Conflict)
        };
    }
    let claim_exists = async_task_submission_claims::Entity::find()
        .filter(
            async_task_submission_claims::Column::TaskKey
                .eq(SensitiveString::from(write.task_id.persistence_key())),
        )
        .filter(async_task_submission_claims::Column::UserId.eq(write.user_id.get()))
        .one(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .is_some();
    let group_exists = groups::Entity::find_by_id(write.target_group_id.get())
        .one(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .is_some();
    if !claim_exists || !group_exists {
        return Ok(AsyncTaskBillingPlanOutcome::NotFound);
    }
    let created_at = to_database_time(write.observed_at)?;
    let inserted = async_task_billings::ActiveModel {
        task_key: Set(SensitiveString::from(write.task_id.persistence_key())),
        user_id: Set(write.user_id.get()),
        reservation_key: Set(
            BillingReservationKey::parse(&write.reservation_id.persistence_key())
                .map_err(|_| AsyncTaskRepositoryError::Invariant)?,
        ),
        target_group_id: Set(write.target_group_id.get()),
        state: Set(AsyncTaskBillingState::Planned.database_value()),
        price_card_version: Set(write.price_card_version),
        billing_resolution: Set(write.resolution.database_value()),
        group_ratio_micros: Set(write.ratios[0]),
        group_model_ratio_micros: Set(write.ratios[1]),
        peak_ratio_micros: Set(write.ratios[2]),
        upper_bound: Set(write.upper_bound.units()),
        rate_microusd: Set(None),
        fallback_quota: Set(None),
        actual_quota: Set(None),
        actual_duration_seconds: Set(None),
        version: Set(1),
        created_at: Set(created_at),
        updated_at: Set(created_at),
    }
    .insert(connection)
    .await;
    match inserted {
        Ok(model) => Ok(AsyncTaskBillingPlanOutcome::Created(record(model)?)),
        Err(error) if is_unique_conflict(&error) => {
            let existing = load(connection, write.user_id, write.task_id)
                .await?
                .ok_or(AsyncTaskRepositoryError::Invariant)?;
            if existing.matches_plan(write) {
                Ok(AsyncTaskBillingPlanOutcome::Existing(existing))
            } else {
                Err(AsyncTaskRepositoryError::Conflict)
            }
        }
        Err(_) => Err(AsyncTaskRepositoryError::Query),
    }
}

async fn transition<C>(
    connection: &C,
    write: &AsyncTaskBillingMark,
    sources: &[AsyncTaskBillingState],
    target: AsyncTaskBillingState,
) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let Some(current) = load(connection, write.user_id, write.task_id).await? else {
        return Ok(AsyncTaskBillingMutationOutcome::NotFound);
    };
    let expected_version =
        u64::try_from(write.expected_version).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    if current.state == target
        && (current.version == expected_version || current.version == expected_version + 1)
    {
        return Ok(AsyncTaskBillingMutationOutcome::Existing(current));
    }
    if !sources.contains(&current.state)
        || current.version != expected_version
        || write.observed_at < current.updated_at
    {
        return Err(AsyncTaskRepositoryError::Conflict);
    }
    let next_version = write
        .expected_version
        .checked_add(1)
        .ok_or(AsyncTaskRepositoryError::Invariant)?;
    let updated_at = to_database_time(write.observed_at)?;
    let source_codes = sources
        .iter()
        .copied()
        .map(AsyncTaskBillingState::database_value)
        .collect::<Vec<_>>();
    let update = async_task_billings::Entity::update_many()
        .filter(
            async_task_billings::Column::TaskKey
                .eq(SensitiveString::from(write.task_id.persistence_key())),
        )
        .filter(async_task_billings::Column::UserId.eq(write.user_id.get()))
        .filter(async_task_billings::Column::Version.eq(write.expected_version))
        .filter(async_task_billings::Column::State.is_in(source_codes))
        .col_expr(
            async_task_billings::Column::State,
            Expr::value(target.database_value()),
        )
        .col_expr(
            async_task_billings::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            async_task_billings::Column::UpdatedAt,
            Expr::value(updated_at),
        )
        .exec(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    classify_update(
        connection,
        write.user_id,
        write.task_id,
        update.rows_affected,
        target,
    )
    .await
}

async fn accept<C>(
    connection: &C,
    write: &AsyncTaskBillingAccept,
) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let Some(current) = load(connection, write.mark.user_id, write.mark.task_id).await? else {
        return Ok(AsyncTaskBillingMutationOutcome::NotFound);
    };
    let expected_version = u64::try_from(write.mark.expected_version)
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let same = current.rate_microusd == Some(write.rate_microusd)
        && current.fallback_quota == Some(write.fallback_quota);
    if current.state == AsyncTaskBillingState::Submitted
        && same
        && (current.version == expected_version || current.version == expected_version + 1)
    {
        return Ok(AsyncTaskBillingMutationOutcome::Existing(current));
    }
    if current.state != AsyncTaskBillingState::Reserved
        || current.version != expected_version
        || write.mark.observed_at < current.updated_at
        || write.fallback_quota > current.upper_bound
    {
        return Err(AsyncTaskRepositoryError::Conflict);
    }
    let next_version = next_version(write.mark.expected_version)?;
    let updated_at = to_database_time(write.mark.observed_at)?;
    let update = async_task_billings::Entity::update_many()
        .filter(
            async_task_billings::Column::TaskKey
                .eq(SensitiveString::from(write.mark.task_id.persistence_key())),
        )
        .filter(async_task_billings::Column::UserId.eq(write.mark.user_id.get()))
        .filter(async_task_billings::Column::Version.eq(write.mark.expected_version))
        .filter(
            async_task_billings::Column::State.eq(AsyncTaskBillingState::Reserved.database_value()),
        )
        .col_expr(
            async_task_billings::Column::State,
            Expr::value(AsyncTaskBillingState::Submitted.database_value()),
        )
        .col_expr(
            async_task_billings::Column::RateMicrousd,
            Expr::value(Some(write.rate_microusd)),
        )
        .col_expr(
            async_task_billings::Column::FallbackQuota,
            Expr::value(Some(write.fallback_quota.units())),
        )
        .col_expr(
            async_task_billings::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            async_task_billings::Column::UpdatedAt,
            Expr::value(updated_at),
        )
        .exec(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    classify_update(
        connection,
        write.mark.user_id,
        write.mark.task_id,
        update.rows_affected,
        AsyncTaskBillingState::Submitted,
    )
    .await
}

async fn begin_settlement<C>(
    connection: &C,
    write: &AsyncTaskBillingSettlement,
) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let Some(current) = load(connection, write.mark.user_id, write.mark.task_id).await? else {
        return Ok(AsyncTaskBillingMutationOutcome::NotFound);
    };
    let expected_version = u64::try_from(write.mark.expected_version)
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let same = current.actual_quota == Some(write.actual_quota)
        && current.actual_duration_seconds == u8::try_from(write.actual_duration_seconds).ok();
    if current.state == AsyncTaskBillingState::SettlementPending
        && same
        && (current.version == expected_version || current.version == expected_version + 1)
    {
        return Ok(AsyncTaskBillingMutationOutcome::Existing(current));
    }
    if current.state != AsyncTaskBillingState::Submitted
        || current.version != expected_version
        || write.mark.observed_at < current.updated_at
        || write.actual_quota > current.upper_bound
    {
        return Err(AsyncTaskRepositoryError::Conflict);
    }
    let next_version = next_version(write.mark.expected_version)?;
    let updated_at = to_database_time(write.mark.observed_at)?;
    let update = async_task_billings::Entity::update_many()
        .filter(
            async_task_billings::Column::TaskKey
                .eq(SensitiveString::from(write.mark.task_id.persistence_key())),
        )
        .filter(async_task_billings::Column::UserId.eq(write.mark.user_id.get()))
        .filter(async_task_billings::Column::Version.eq(write.mark.expected_version))
        .filter(
            async_task_billings::Column::State
                .eq(AsyncTaskBillingState::Submitted.database_value()),
        )
        .col_expr(
            async_task_billings::Column::State,
            Expr::value(AsyncTaskBillingState::SettlementPending.database_value()),
        )
        .col_expr(
            async_task_billings::Column::ActualQuota,
            Expr::value(Some(write.actual_quota.units())),
        )
        .col_expr(
            async_task_billings::Column::ActualDurationSeconds,
            Expr::value(Some(write.actual_duration_seconds)),
        )
        .col_expr(
            async_task_billings::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            async_task_billings::Column::UpdatedAt,
            Expr::value(updated_at),
        )
        .exec(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    classify_update(
        connection,
        write.mark.user_id,
        write.mark.task_id,
        update.rows_affected,
        AsyncTaskBillingState::SettlementPending,
    )
    .await
}

async fn clear_released<C>(
    connection: &C,
    write: &AsyncTaskBillingClear,
) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let Some(current) = load(connection, write.mark.user_id, write.mark.task_id).await? else {
        return Ok(AsyncTaskBillingMutationOutcome::NotFound);
    };
    let expected_version = u64::try_from(write.mark.expected_version)
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    if current.state != AsyncTaskBillingState::Released
        || current.version != expected_version
        || current.reservation_id != write.reservation_id
        || write.mark.observed_at < current.updated_at
    {
        return Err(AsyncTaskRepositoryError::Conflict);
    }
    let deleted = async_task_billings::Entity::delete_many()
        .filter(
            async_task_billings::Column::TaskKey
                .eq(SensitiveString::from(write.mark.task_id.persistence_key())),
        )
        .filter(async_task_billings::Column::UserId.eq(write.mark.user_id.get()))
        .filter(async_task_billings::Column::Version.eq(write.mark.expected_version))
        .filter(
            async_task_billings::Column::State.eq(AsyncTaskBillingState::Released.database_value()),
        )
        .filter(
            async_task_billings::Column::ReservationKey.eq(BillingReservationKey::parse(
                &write.reservation_id.persistence_key(),
            )
            .map_err(|_| AsyncTaskRepositoryError::Invariant)?),
        )
        .exec(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    if deleted.rows_affected == 1 {
        Ok(AsyncTaskBillingMutationOutcome::Applied(current))
    } else {
        Err(AsyncTaskRepositoryError::Conflict)
    }
}

async fn classify_update<C>(
    connection: &C,
    user_id: UserId,
    task_id: AsyncTaskId,
    rows_affected: u64,
    target: AsyncTaskBillingState,
) -> Result<AsyncTaskBillingMutationOutcome, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let current = load(connection, user_id, task_id).await?;
    if rows_affected == 1 {
        return current
            .map(AsyncTaskBillingMutationOutcome::Applied)
            .ok_or(AsyncTaskRepositoryError::Invariant);
    }
    match current {
        None => Ok(AsyncTaskBillingMutationOutcome::NotFound),
        Some(record) if record.state == target => {
            Ok(AsyncTaskBillingMutationOutcome::Existing(record))
        }
        Some(_) => Err(AsyncTaskRepositoryError::Conflict),
    }
}

async fn load<C>(
    connection: &C,
    user_id: UserId,
    task_id: AsyncTaskId,
) -> Result<Option<AsyncTaskBillingRecord>, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    async_task_billings::Entity::find()
        .filter(
            async_task_billings::Column::TaskKey
                .eq(SensitiveString::from(task_id.persistence_key())),
        )
        .filter(async_task_billings::Column::UserId.eq(user_id.get()))
        .one(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .map(record)
        .transpose()
}

fn record(
    model: async_task_billings::Model,
) -> Result<AsyncTaskBillingRecord, AsyncTaskRepositoryError> {
    let task_id = AsyncTaskId::from_persistence_key(model.task_key.as_str())
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let user_id = UserId::new(model.user_id).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let reservation_id = BillingReservationId::from_persistence_key(model.reservation_key.as_str())
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let target_group_id =
        GroupId::new(model.target_group_id).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let state = AsyncTaskBillingState::from_database(model.state)
        .ok_or(AsyncTaskRepositoryError::Invariant)?;
    let price_card_version =
        u16::try_from(model.price_card_version).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    if price_card_version == 0 {
        return Err(AsyncTaskRepositoryError::Invariant);
    }
    let resolution = AsyncTaskBillingResolution::from_database(model.billing_resolution)
        .ok_or(AsyncTaskRepositoryError::Invariant)?;
    let ratios = [
        model.group_ratio_micros,
        model.group_model_ratio_micros,
        model.peak_ratio_micros,
    ];
    if ratios.into_iter().any(|value| value < 0) {
        return Err(AsyncTaskRepositoryError::Invariant);
    }
    let upper_bound =
        Quota::new(model.upper_bound).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    if upper_bound.is_zero() {
        return Err(AsyncTaskRepositoryError::Invariant);
    }
    let rate_microusd = model.rate_microusd;
    let fallback_quota = model
        .fallback_quota
        .map(Quota::new)
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let actual_quota = model
        .actual_quota
        .map(Quota::new)
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let actual_duration_seconds = model
        .actual_duration_seconds
        .map(u8::try_from)
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let version = u64::try_from(model.version).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    if version == 0
        || updated_at < created_at
        || !valid_shape(
            state,
            rate_microusd,
            fallback_quota,
            actual_quota,
            actual_duration_seconds,
            upper_bound,
        )
    {
        return Err(AsyncTaskRepositoryError::Invariant);
    }
    Ok(AsyncTaskBillingRecord {
        task_id,
        user_id,
        reservation_id,
        target_group_id,
        state,
        price_card_version,
        resolution,
        ratios,
        upper_bound,
        rate_microusd,
        fallback_quota,
        actual_quota,
        actual_duration_seconds,
        version,
        created_at,
        updated_at,
    })
}

fn valid_shape(
    state: AsyncTaskBillingState,
    rate_microusd: Option<i64>,
    fallback_quota: Option<Quota>,
    actual_quota: Option<Quota>,
    actual_duration_seconds: Option<u8>,
    upper_bound: Quota,
) -> bool {
    let submission = rate_microusd.is_some_and(|value| value > 0)
        && fallback_quota.is_some_and(|value| value <= upper_bound);
    let settlement = actual_quota.is_some_and(|value| value <= upper_bound)
        && actual_duration_seconds.is_some_and(|value| (1..=15).contains(&value));
    match state {
        AsyncTaskBillingState::Planned | AsyncTaskBillingState::Reserved => {
            rate_microusd.is_none()
                && fallback_quota.is_none()
                && actual_quota.is_none()
                && actual_duration_seconds.is_none()
        }
        AsyncTaskBillingState::Submitted => {
            submission && actual_quota.is_none() && actual_duration_seconds.is_none()
        }
        AsyncTaskBillingState::SettlementPending | AsyncTaskBillingState::Settled => {
            submission && settlement
        }
        AsyncTaskBillingState::ReleasePending | AsyncTaskBillingState::Released => {
            actual_quota.is_none()
                && actual_duration_seconds.is_none()
                && ((rate_microusd.is_none() && fallback_quota.is_none()) || submission)
        }
    }
}

fn next_version(value: i64) -> Result<i64, AsyncTaskRepositoryError> {
    value
        .checked_add(1)
        .ok_or(AsyncTaskRepositoryError::Invariant)
}

fn to_database_time(value: u64) -> Result<TimeDateTimeWithTimeZone, AsyncTaskRepositoryError> {
    let value = i64::try_from(value).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| AsyncTaskRepositoryError::Invariant)
}

fn unix_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, AsyncTaskRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| AsyncTaskRepositoryError::Invariant)
}

fn is_unique_conflict(error: &sea_orm::DbErr) -> bool {
    matches!(
        error.sql_err(),
        Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
    )
}
