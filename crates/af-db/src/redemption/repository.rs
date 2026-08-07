use std::{collections::HashMap, fmt, time::Duration};

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use af_domain::{
    Quota, QuotaDelta, QuotaError, RedemptionBatchId, RedemptionBatchStatus, RedemptionCodeId,
    RedemptionCodeStatus, UserId, WalletEventId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType, Query},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, WalletLedgerEntryType,
    entity::{
        SensitiveString, WalletLedgerKey, redemption_batches, redemption_codes, users,
        wallet_ledger_entries,
    },
};

use super::types::valid_name;
use super::{
    MAX_REDEMPTION_BATCH_CODES, MAX_REDEMPTION_BATCH_PAGE_SIZE, RedemptionAttempt,
    RedemptionAuditBatchRecord, RedemptionAuditPageRecord, RedemptionAuditQuery,
    RedemptionAuditStatus, RedemptionAuditSummaryRecord, RedemptionBatchCreateOutcome,
    RedemptionBatchDisable, RedemptionBatchDisableOutcome, RedemptionBatchListRecord,
    RedemptionBatchPageRecord, RedemptionBatchRecord, RedemptionBatchWrite,
    RedemptionCodeDefinition, RedemptionCodeDigest, RedemptionOutcome, RedemptionRecord,
    RedemptionRejection, RedemptionRepositoryConfigError, RedemptionRepositoryError,
};

/// 原子维护兑换码批次、单次消费与钱包到账的数据库仓储。
#[derive(Clone)]
pub struct RedemptionRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    #[cfg(test)]
    outcome_unknown_after_commit: Arc<AtomicBool>,
}

impl RedemptionRepository {
    /// 使用共享连接池和单次读写截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, RedemptionRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(RedemptionRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
            #[cfg(test)]
            outcome_unknown_after_commit: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 按数据库主键倒序读取一页批次，并聚合已经到账的兑换码数量。
    pub async fn list_batches(
        &self,
        before: Option<i64>,
        limit: usize,
    ) -> Result<RedemptionBatchPageRecord, RedemptionRepositoryError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_REDEMPTION_BATCH_PAGE_SIZE).contains(&limit)
        {
            return Err(internal(RedemptionRepositoryError::Invariant));
        }
        let operation = self
            .list_batches_inner(before, limit)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(RedemptionRepositoryError::Timeout)),
        }
    }

    /// 按批次读取兑换码运营统计，筛选只作用于已持久化的批次和兑换事实。
    pub async fn audit_batches(
        &self,
        query: &RedemptionAuditQuery,
    ) -> Result<RedemptionAuditPageRecord, RedemptionRepositoryError> {
        let operation = self
            .audit_batches_inner(query)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(RedemptionRepositoryError::Timeout)),
        }
    }

    /// 幂等创建一个只保存摘要的兑换码批次。
    pub async fn create_batch(
        &self,
        write: &RedemptionBatchWrite,
    ) -> Result<RedemptionBatchCreateOutcome, RedemptionRepositoryError> {
        let operation = self
            .create_batch_inner(write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(RedemptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, RedemptionBatchCreateOutcome::Created(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(RedemptionRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    /// 以 CAS 版本整体禁用一个兑换码批次。
    pub async fn disable_batch(
        &self,
        write: &RedemptionBatchDisable,
    ) -> Result<RedemptionBatchDisableOutcome, RedemptionRepositoryError> {
        let operation = self
            .disable_batch_inner(write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(RedemptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, RedemptionBatchDisableOutcome::Applied(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(RedemptionRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    /// 原子消费一个兑换码，并在同一事务内完成余额与钱包账本到账。
    pub async fn redeem(
        &self,
        attempt: &RedemptionAttempt,
    ) -> Result<RedemptionOutcome, RedemptionRepositoryError> {
        let operation = self
            .redeem_inner(attempt)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(RedemptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, RedemptionOutcome::Applied(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(RedemptionRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    async fn list_batches_inner(
        &self,
        before: Option<i64>,
        limit: usize,
    ) -> Result<RedemptionBatchPageRecord, RedemptionRepositoryError> {
        let query_limit = u64::try_from(limit)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(RedemptionRepositoryError::Invariant)?;
        let mut query = redemption_batches::Entity::find()
            .order_by_desc(redemption_batches::Column::Id)
            .limit(query_limit);
        if let Some(before) = before {
            query = query.filter(redemption_batches::Column::Id.lt(before));
        }
        let mut models = query
            .all(self.pool.connection())
            .await
            .map_err(|_| RedemptionRepositoryError::Query)?;
        let has_more = models.len() > limit;
        if has_more {
            models.truncate(limit);
        }
        let batch_ids = models.iter().map(|model| model.id).collect::<Vec<_>>();
        let counts = if batch_ids.is_empty() {
            Vec::new()
        } else {
            redemption_codes::Entity::find()
                .select_only()
                .column(redemption_codes::Column::BatchId)
                .column_as(redemption_codes::Column::Id.count(), "redeemed_count")
                .filter(redemption_codes::Column::BatchId.is_in(batch_ids))
                .filter(redemption_codes::Column::Status.eq(RedemptionCodeStatus::Redeemed.code()))
                .group_by(redemption_codes::Column::BatchId)
                .into_tuple::<(i64, i64)>()
                .all(self.pool.connection())
                .await
                .map_err(|_| RedemptionRepositoryError::Query)?
        };
        let mut redeemed_counts = HashMap::with_capacity(counts.len());
        for (batch_id, count) in counts {
            let count = usize::try_from(count).map_err(|_| RedemptionRepositoryError::Invariant)?;
            if redeemed_counts.insert(batch_id, count).is_some() {
                return Err(RedemptionRepositoryError::Invariant);
            }
        }
        let next_cursor = has_more
            .then(|| models.last().map(|model| model.id))
            .flatten();
        let mut batches = Vec::with_capacity(models.len());
        for model in models {
            let redeemed_count = redeemed_counts.remove(&model.id).unwrap_or_default();
            let record = batch_record(model)?;
            if redeemed_count > record.code_count() {
                return Err(RedemptionRepositoryError::Invariant);
            }
            batches.push(RedemptionBatchListRecord::new(record, redeemed_count));
        }
        if !redeemed_counts.is_empty() {
            return Err(RedemptionRepositoryError::Invariant);
        }
        Ok(RedemptionBatchPageRecord::new(batches, next_cursor))
    }

    async fn audit_batches_inner(
        &self,
        query: &RedemptionAuditQuery,
    ) -> Result<RedemptionAuditPageRecord, RedemptionRepositoryError> {
        let now = to_database_time(query.now())?;
        let query_limit = u64::try_from(query.limit())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(RedemptionRepositoryError::Invariant)?;
        let mut batches_query = redemption_batches::Entity::find()
            .order_by_desc(redemption_batches::Column::Id)
            .limit(query_limit);
        if let Some(before) = query.before() {
            batches_query = batches_query.filter(redemption_batches::Column::Id.lt(before));
        }
        if let Some(batch_id) = query.batch_id() {
            batches_query = batches_query.filter(
                redemption_batches::Column::BatchKey
                    .eq(SensitiveString::from(batch_id.persistence_key())),
            );
        }
        match query.status() {
            Some(RedemptionAuditStatus::Active) => {
                batches_query = batches_query.filter(
                    redemption_batches::Column::Status
                        .eq(RedemptionBatchStatus::Active.code())
                        .and(
                            redemption_batches::Column::ExpiresAt
                                .is_null()
                                .or(redemption_batches::Column::ExpiresAt.gt(now)),
                        ),
                );
            }
            Some(RedemptionAuditStatus::Expired) => {
                batches_query = batches_query.filter(
                    redemption_batches::Column::Status
                        .eq(RedemptionBatchStatus::Active.code())
                        .and(
                            redemption_batches::Column::ExpiresAt
                                .is_not_null()
                                .and(redemption_batches::Column::ExpiresAt.lte(now)),
                        ),
                );
            }
            Some(RedemptionAuditStatus::Disabled) => {
                batches_query = batches_query.filter(
                    redemption_batches::Column::Status.eq(RedemptionBatchStatus::Disabled.code()),
                );
            }
            Some(RedemptionAuditStatus::Redeemed) | None => {}
        }

        let needs_redeemed_filter = query.status() == Some(RedemptionAuditStatus::Redeemed)
            || query.redeemed_after().is_some()
            || query.redeemed_before().is_some();
        if needs_redeemed_filter {
            // 兑换事实可能跨越大量批次，使用数据库子查询避免把全量 ID 拉回并撞绑定参数上限。
            let mut matching_batches = Query::select();
            matching_batches
                .distinct()
                .column(redemption_codes::Column::BatchId)
                .from(redemption_codes::Entity)
                .and_where(
                    Expr::col(redemption_codes::Column::Status)
                        .eq(RedemptionCodeStatus::Redeemed.code()),
                );
            if let Some(after) = query.redeemed_after() {
                matching_batches.and_where(
                    Expr::col(redemption_codes::Column::RedeemedAt)
                        .is_not_null()
                        .and(
                            Expr::col(redemption_codes::Column::RedeemedAt)
                                .gte(to_database_time(after)?),
                        ),
                );
            }
            if let Some(before) = query.redeemed_before() {
                matching_batches.and_where(
                    Expr::col(redemption_codes::Column::RedeemedAt)
                        .is_not_null()
                        .and(
                            Expr::col(redemption_codes::Column::RedeemedAt)
                                .lt(to_database_time(before)?),
                        ),
                );
            }
            batches_query = batches_query.filter(
                Expr::col(redemption_batches::Column::Id).in_subquery(matching_batches.to_owned()),
            );
        }

        let mut batch_models = batches_query
            .all(self.pool.connection())
            .await
            .map_err(|_| RedemptionRepositoryError::Query)?;
        let has_more = batch_models.len() > query.limit();
        if has_more {
            batch_models.truncate(query.limit());
        }
        let next_cursor = has_more
            .then(|| batch_models.last().map(|model| model.id))
            .flatten();
        let batch_records = batch_models
            .into_iter()
            .map(batch_record)
            .collect::<Result<Vec<_>, _>>()?;
        let batch_ids = batch_records
            .iter()
            .map(RedemptionBatchRecord::database_id)
            .collect::<Vec<_>>();
        if batch_ids.is_empty() {
            return Ok(empty_audit_page());
        }

        let code_models = redemption_codes::Entity::find()
            .filter(redemption_codes::Column::BatchId.is_in(batch_ids.clone()))
            .all(self.pool.connection())
            .await
            .map_err(|_| RedemptionRepositoryError::Query)?;
        let mut code_stats = HashMap::<i64, AuditCodeStats>::with_capacity(batch_ids.len());
        for model in code_models {
            let code = code_record(model)?;
            if !batch_ids.contains(&code.batch_database_id) {
                return Err(RedemptionRepositoryError::Invariant);
            }
            let stats = code_stats.entry(code.batch_database_id).or_default();
            if code.status == RedemptionCodeStatus::Redeemed {
                stats.redeemed_count = stats
                    .redeemed_count
                    .checked_add(1)
                    .ok_or(RedemptionRepositoryError::Invariant)?;
                let redeemed_at = code
                    .redeemed_at
                    .ok_or(RedemptionRepositoryError::Invariant)?;
                stats.last_redeemed_at = Some(
                    stats
                        .last_redeemed_at
                        .map_or(redeemed_at, |current| current.max(redeemed_at)),
                );
                continue;
            }
            let batch = batch_records
                .iter()
                .find(|record| record.database_id() == code.batch_database_id)
                .ok_or(RedemptionRepositoryError::Invariant)?;
            if batch.status() == RedemptionBatchStatus::Disabled {
                stats.disabled_count = stats
                    .disabled_count
                    .checked_add(1)
                    .ok_or(RedemptionRepositoryError::Invariant)?;
            } else if batch.expires_at().is_some_and(|value| value <= query.now()) {
                stats.expired_count = stats
                    .expired_count
                    .checked_add(1)
                    .ok_or(RedemptionRepositoryError::Invariant)?;
            } else {
                stats.remaining_count = stats
                    .remaining_count
                    .checked_add(1)
                    .ok_or(RedemptionRepositoryError::Invariant)?;
            }
        }

        let mut audit_batches = Vec::with_capacity(batch_records.len());
        let mut summary = AuditSummaryAccumulator::default();
        for batch in batch_records {
            let stats = code_stats.remove(&batch.database_id()).unwrap_or_default();
            let classified = stats
                .redeemed_count
                .checked_add(stats.remaining_count)
                .and_then(|value| value.checked_add(stats.expired_count))
                .and_then(|value| value.checked_add(stats.disabled_count))
                .ok_or(RedemptionRepositoryError::Invariant)?;
            if classified != batch.code_count() {
                return Err(RedemptionRepositoryError::Invariant);
            }
            summary.add(
                batch.code_count(),
                stats.redeemed_count,
                stats.remaining_count,
                stats.expired_count,
                stats.disabled_count,
            )?;
            audit_batches.push(RedemptionAuditBatchRecord::new(
                batch,
                classified,
                stats.redeemed_count,
                stats.remaining_count,
                stats.expired_count,
                stats.disabled_count,
                stats.last_redeemed_at,
            ));
        }
        if !code_stats.is_empty() {
            return Err(RedemptionRepositoryError::Invariant);
        }
        Ok(RedemptionAuditPageRecord::new(
            audit_batches,
            summary.finish(),
            next_cursor,
        ))
    }

    /// 仅供回归测试模拟事务已提交但调用方未收到确定结果。
    #[cfg(test)]
    pub(crate) fn inject_outcome_unknown_after_commit(&self) {
        self.outcome_unknown_after_commit
            .store(true, Ordering::Release);
    }

    async fn create_batch_inner(
        &self,
        write: &RedemptionBatchWrite,
    ) -> Result<RedemptionBatchCreateOutcome, RedemptionRepositoryError> {
        if let Some(snapshot) = load_batch_snapshot(self.pool.connection(), write.batch_id).await? {
            return classify_batch_snapshot(snapshot, write);
        }

        let transaction = begin(&self.pool).await?;
        let creator_exists = lock_user(&transaction, write.created_by_user_id)
            .await?
            .is_some();
        if !creator_exists {
            rollback(transaction).await?;
            return Ok(RedemptionBatchCreateOutcome::CreatorNotFound);
        }
        if let Some(snapshot) = load_batch_snapshot(&transaction, write.batch_id).await? {
            rollback(transaction).await?;
            return classify_batch_snapshot(snapshot, write);
        }

        let created_at = to_database_time(write.created_at)?;
        let expires_at = write.expires_at.map(to_database_time).transpose()?;
        let batch = redemption_batches::ActiveModel {
            batch_key: Set(SensitiveString::from(write.batch_id.persistence_key())),
            name: Set(write.name.clone()),
            created_by_user_id: Set(write.created_by_user_id.get()),
            status: Set(RedemptionBatchStatus::Active.code()),
            quota_amount: Set(write.quota_amount.units()),
            code_count: Set(i32::try_from(write.codes.len())
                .map_err(|_| RedemptionRepositoryError::Invariant)?),
            version: Set(1),
            expires_at: Set(expires_at),
            disabled_at: Set(None),
            created_at: Set(created_at),
            updated_at: Set(created_at),
            ..Default::default()
        }
        .insert(&transaction)
        .await;
        let batch = match batch {
            Ok(batch) => batch,
            Err(error) => {
                let unique = is_unique_conflict(&error);
                rollback(transaction).await?;
                return if unique {
                    recover_batch_collision(self.pool.connection(), write).await
                } else {
                    Err(RedemptionRepositoryError::Query)
                };
            }
        };

        for definition in &write.codes {
            let inserted = redemption_codes::ActiveModel {
                code_key: Set(SensitiveString::from(
                    definition.code_id().persistence_key(),
                )),
                batch_id: Set(batch.id),
                code_sha256: Set(SensitiveString::from(definition.digest().persistence_key())),
                status: Set(RedemptionCodeStatus::Available.code()),
                used_by_user_id: Set(None),
                redeemed_at: Set(None),
                created_at: Set(created_at),
                ..Default::default()
            }
            .insert(&transaction)
            .await;
            if let Err(error) = inserted {
                let unique = is_unique_conflict(&error);
                rollback(transaction).await?;
                return if unique {
                    recover_batch_collision(self.pool.connection(), write).await
                } else {
                    Err(RedemptionRepositoryError::Query)
                };
            }
        }

        let record = batch_record(batch)?;
        commit(transaction).await?;
        Ok(RedemptionBatchCreateOutcome::Created(record))
    }

    async fn disable_batch_inner(
        &self,
        write: &RedemptionBatchDisable,
    ) -> Result<RedemptionBatchDisableOutcome, RedemptionRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let Some(model) = lock_batch_by_key(&transaction, write.batch_id).await? else {
            rollback(transaction).await?;
            return Ok(RedemptionBatchDisableOutcome::NotFound);
        };
        let current = batch_record(model.clone())?;
        if current.status == RedemptionBatchStatus::Disabled {
            rollback(transaction).await?;
            return if current.version == write.expected_version as u64 + 1
                && current.disabled_at == Some(write.disabled_at)
            {
                Ok(RedemptionBatchDisableOutcome::Existing(current))
            } else {
                Err(RedemptionRepositoryError::Conflict)
            };
        }
        if current.version != write.expected_version as u64
            || write.disabled_at < current.created_at
        {
            rollback(transaction).await?;
            return Err(RedemptionRepositoryError::Conflict);
        }

        let disabled_at = to_database_time(write.disabled_at)?;
        let updated_at = monotonic_updated_at(model.updated_at, disabled_at);
        let next_version = write
            .expected_version
            .checked_add(1)
            .ok_or(RedemptionRepositoryError::Invariant)?;
        let update = redemption_batches::Entity::update_many()
            .filter(redemption_batches::Column::Id.eq(model.id))
            .filter(redemption_batches::Column::Status.eq(RedemptionBatchStatus::Active.code()))
            .filter(redemption_batches::Column::Version.eq(write.expected_version))
            .col_expr(
                redemption_batches::Column::Status,
                Expr::value(RedemptionBatchStatus::Disabled.code()),
            )
            .col_expr(
                redemption_batches::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                redemption_batches::Column::DisabledAt,
                Expr::value(disabled_at),
            )
            .col_expr(
                redemption_batches::Column::UpdatedAt,
                Expr::value(updated_at),
            )
            .exec(&transaction)
            .await
            .map_err(|_| RedemptionRepositoryError::Query)?;
        if update.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(RedemptionRepositoryError::Conflict);
        }
        let updated = redemption_batches::Entity::find_by_id(model.id)
            .one(&transaction)
            .await
            .map_err(|_| RedemptionRepositoryError::Query)?
            .ok_or(RedemptionRepositoryError::Invariant)?;
        let record = batch_record(updated)?;
        commit(transaction).await?;
        Ok(RedemptionBatchDisableOutcome::Applied(record))
    }

    async fn redeem_inner(
        &self,
        attempt: &RedemptionAttempt,
    ) -> Result<RedemptionOutcome, RedemptionRepositoryError> {
        let digest = attempt.code.digest();
        let transaction = begin(&self.pool).await?;
        let Some(user) = lock_user(&transaction, attempt.user_id).await? else {
            rollback(transaction).await?;
            return Ok(RedemptionOutcome::UserNotFound);
        };
        let Some(code_model) = lock_code_by_digest(&transaction, digest).await? else {
            rollback(transaction).await?;
            return Ok(RedemptionOutcome::Rejected(
                RedemptionRejection::InvalidCode,
            ));
        };
        let code = code_record(code_model.clone())?;
        let batch_model = lock_batch_by_database_id(&transaction, code.batch_database_id)
            .await?
            .ok_or(RedemptionRepositoryError::Invariant)?;
        let batch = batch_record(batch_model)?;

        if code.status == RedemptionCodeStatus::Redeemed {
            let outcome =
                classify_redeemed_code(&transaction, &code, &batch, attempt.user_id).await;
            rollback(transaction).await?;
            return outcome;
        }
        if !batch.status.is_active() {
            rollback(transaction).await?;
            return Ok(RedemptionOutcome::Rejected(
                RedemptionRejection::BatchDisabled,
            ));
        }
        if batch
            .expires_at
            .is_some_and(|expires_at| expires_at <= attempt.redeemed_at)
        {
            rollback(transaction).await?;
            return Ok(RedemptionOutcome::Rejected(RedemptionRejection::Expired));
        }
        if attempt.redeemed_at < batch.created_at || attempt.redeemed_at < code.created_at {
            rollback(transaction).await?;
            return Ok(RedemptionOutcome::Rejected(
                RedemptionRejection::TimingConflict,
            ));
        }

        let balance_before =
            Quota::new(user.quota).map_err(|_| RedemptionRepositoryError::Invariant)?;
        let delta = QuotaDelta::new(batch.quota_amount.units())
            .map_err(|_| RedemptionRepositoryError::Invariant)?;
        let balance_after = match balance_before.checked_apply(delta) {
            Ok(balance) => balance,
            Err(QuotaError::Overflow) => {
                rollback(transaction).await?;
                return Ok(RedemptionOutcome::Rejected(
                    RedemptionRejection::CreditOverflow,
                ));
            }
            Err(_) => return Err(RedemptionRepositoryError::Invariant),
        };
        let redeemed_at = to_database_time(attempt.redeemed_at)?;
        let updated_at = monotonic_updated_at(user.updated_at, redeemed_at);

        let code_update = redemption_codes::Entity::update_many()
            .filter(redemption_codes::Column::Id.eq(code.database_id))
            .filter(redemption_codes::Column::Status.eq(RedemptionCodeStatus::Available.code()))
            .filter(redemption_codes::Column::UsedByUserId.is_null())
            .filter(redemption_codes::Column::RedeemedAt.is_null())
            .col_expr(
                redemption_codes::Column::Status,
                Expr::value(RedemptionCodeStatus::Redeemed.code()),
            )
            .col_expr(
                redemption_codes::Column::UsedByUserId,
                Expr::value(attempt.user_id.get()),
            )
            .col_expr(
                redemption_codes::Column::RedeemedAt,
                Expr::value(redeemed_at),
            )
            .exec(&transaction)
            .await
            .map_err(|_| RedemptionRepositoryError::Query)?;
        if code_update.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(RedemptionRepositoryError::Conflict);
        }

        let user_update = users::Entity::update_many()
            .filter(users::Column::Id.eq(attempt.user_id.get()))
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::Quota.eq(balance_before.units()))
            .col_expr(users::Column::Quota, Expr::value(balance_after.units()))
            .col_expr(users::Column::UpdatedAt, Expr::value(updated_at))
            .exec(&transaction)
            .await
            .map_err(|_| RedemptionRepositoryError::Query)?;
        if user_update.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(RedemptionRepositoryError::Invariant);
        }

        wallet_ledger_entries::ActiveModel {
            event_key: Set(WalletLedgerKey::parse(&code.code_id.persistence_key())
                .map_err(|_| RedemptionRepositoryError::Invariant)?),
            user_id: Set(attempt.user_id.get()),
            actor_user_id: Set(None),
            entry_type: Set(WalletLedgerEntryType::Redemption as i16),
            quota_delta: Set(batch.quota_amount.units()),
            balance_before: Set(balance_before.units()),
            balance_after: Set(balance_after.units()),
            reason: Set(None),
            created_at: Set(redeemed_at),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(|_| RedemptionRepositoryError::Invariant)?;

        let record = RedemptionRecord::new(
            code.code_id,
            batch.batch_id,
            attempt.user_id,
            batch.quota_amount,
            balance_after,
            attempt.redeemed_at,
        );
        commit(transaction).await?;
        Ok(RedemptionOutcome::Applied(record))
    }
}

impl fmt::Debug for RedemptionRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedemptionRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

struct BatchSnapshot {
    record: RedemptionBatchRecord,
    definitions: Vec<RedemptionCodeDefinition>,
}

struct StoredCode {
    database_id: i64,
    code_id: RedemptionCodeId,
    batch_database_id: i64,
    digest: RedemptionCodeDigest,
    status: RedemptionCodeStatus,
    used_by_user_id: Option<UserId>,
    redeemed_at: Option<u64>,
    created_at: u64,
}

#[derive(Default)]
struct AuditCodeStats {
    redeemed_count: usize,
    remaining_count: usize,
    expired_count: usize,
    disabled_count: usize,
    last_redeemed_at: Option<u64>,
}

#[derive(Default)]
struct AuditSummaryAccumulator {
    issued_count: usize,
    redeemed_count: usize,
    remaining_count: usize,
    expired_count: usize,
    disabled_count: usize,
}

impl AuditSummaryAccumulator {
    fn add(
        &mut self,
        issued_count: usize,
        redeemed_count: usize,
        remaining_count: usize,
        expired_count: usize,
        disabled_count: usize,
    ) -> Result<(), RedemptionRepositoryError> {
        self.issued_count = self
            .issued_count
            .checked_add(issued_count)
            .ok_or(RedemptionRepositoryError::Invariant)?;
        self.redeemed_count = self
            .redeemed_count
            .checked_add(redeemed_count)
            .ok_or(RedemptionRepositoryError::Invariant)?;
        self.remaining_count = self
            .remaining_count
            .checked_add(remaining_count)
            .ok_or(RedemptionRepositoryError::Invariant)?;
        self.expired_count = self
            .expired_count
            .checked_add(expired_count)
            .ok_or(RedemptionRepositoryError::Invariant)?;
        self.disabled_count = self
            .disabled_count
            .checked_add(disabled_count)
            .ok_or(RedemptionRepositoryError::Invariant)?;
        Ok(())
    }

    const fn finish(self) -> RedemptionAuditSummaryRecord {
        RedemptionAuditSummaryRecord::new(
            self.issued_count,
            self.redeemed_count,
            self.remaining_count,
            self.expired_count,
            self.disabled_count,
        )
    }
}

fn empty_audit_page() -> RedemptionAuditPageRecord {
    RedemptionAuditPageRecord::new(
        Vec::new(),
        RedemptionAuditSummaryRecord::new(0, 0, 0, 0, 0),
        None,
    )
}

async fn classify_redeemed_code(
    transaction: &DatabaseTransaction,
    code: &StoredCode,
    batch: &RedemptionBatchRecord,
    user_id: UserId,
) -> Result<RedemptionOutcome, RedemptionRepositoryError> {
    if code.used_by_user_id != Some(user_id) {
        return Ok(RedemptionOutcome::Rejected(
            RedemptionRejection::AlreadyUsed,
        ));
    }
    let redeemed_at = code
        .redeemed_at
        .ok_or(RedemptionRepositoryError::Invariant)?;
    let ledger = wallet_ledger_entries::Entity::find()
        .filter(
            wallet_ledger_entries::Column::EventKey
                .eq(WalletLedgerKey::parse(&code.code_id.persistence_key())
                    .map_err(|_| RedemptionRepositoryError::Invariant)?),
        )
        .one(transaction)
        .await
        .map_err(|_| RedemptionRepositoryError::Query)?
        .ok_or(RedemptionRepositoryError::Invariant)?;
    let balance_before =
        Quota::new(ledger.balance_before).map_err(|_| RedemptionRepositoryError::Invariant)?;
    let balance_after =
        Quota::new(ledger.balance_after).map_err(|_| RedemptionRepositoryError::Invariant)?;
    let delta =
        QuotaDelta::new(ledger.quota_delta).map_err(|_| RedemptionRepositoryError::Invariant)?;
    if ledger.user_id != user_id.get()
        || ledger.actor_user_id.is_some()
        || ledger.entry_type != WalletLedgerEntryType::Redemption as i16
        || ledger.quota_delta != batch.quota_amount.units()
        || ledger.reason.is_some()
        || ledger.created_at.unix_timestamp() != redeemed_at as i64
        || balance_before.checked_apply(delta) != Ok(balance_after)
    {
        return Err(RedemptionRepositoryError::Invariant);
    }
    Ok(RedemptionOutcome::Existing(RedemptionRecord::new(
        code.code_id,
        batch.batch_id,
        user_id,
        batch.quota_amount,
        balance_after,
        redeemed_at,
    )))
}

async fn recover_batch_collision<C>(
    connection: &C,
    write: &RedemptionBatchWrite,
) -> Result<RedemptionBatchCreateOutcome, RedemptionRepositoryError>
where
    C: ConnectionTrait,
{
    let snapshot = load_batch_snapshot(connection, write.batch_id)
        .await?
        .ok_or(RedemptionRepositoryError::Conflict)?;
    classify_batch_snapshot(snapshot, write)
}

fn classify_batch_snapshot(
    snapshot: BatchSnapshot,
    write: &RedemptionBatchWrite,
) -> Result<RedemptionBatchCreateOutcome, RedemptionRepositoryError> {
    let mut expected = write.codes.clone();
    expected.sort_unstable_by_key(|definition| definition.code_id().persistence_key());
    let mut actual = snapshot.definitions;
    actual.sort_unstable_by_key(|definition| definition.code_id().persistence_key());
    if snapshot.record.matches_write(write) && actual == expected {
        Ok(RedemptionBatchCreateOutcome::Existing(snapshot.record))
    } else {
        Err(RedemptionRepositoryError::Conflict)
    }
}

async fn load_batch_snapshot<C>(
    connection: &C,
    batch_id: RedemptionBatchId,
) -> Result<Option<BatchSnapshot>, RedemptionRepositoryError>
where
    C: ConnectionTrait,
{
    let Some(model) = redemption_batches::Entity::find()
        .filter(
            redemption_batches::Column::BatchKey
                .eq(SensitiveString::from(batch_id.persistence_key())),
        )
        .one(connection)
        .await
        .map_err(|_| RedemptionRepositoryError::Query)?
    else {
        return Ok(None);
    };
    let record = batch_record(model)?;
    let codes = redemption_codes::Entity::find()
        .filter(redemption_codes::Column::BatchId.eq(record.database_id))
        .order_by_asc(redemption_codes::Column::CodeKey)
        .all(connection)
        .await
        .map_err(|_| RedemptionRepositoryError::Query)?;
    if codes.len() != record.code_count {
        return Err(RedemptionRepositoryError::Invariant);
    }
    let mut definitions = Vec::with_capacity(codes.len());
    for model in codes {
        let code = code_record(model)?;
        if code.batch_database_id != record.database_id || code.created_at != record.created_at {
            return Err(RedemptionRepositoryError::Invariant);
        }
        definitions.push(
            RedemptionCodeDefinition::new(code.code_id, code.digest)
                .map_err(|_| RedemptionRepositoryError::Invariant)?,
        );
    }
    Ok(Some(BatchSnapshot {
        record,
        definitions,
    }))
}

async fn lock_user(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<Option<users::Model>, RedemptionRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 不支持 FOR UPDATE，先通过恒等更新取得数据库写锁。
        let update = users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id.get()))
            .filter(users::Column::DeletedAt.is_null())
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
            .exec(transaction)
            .await
            .map_err(|_| RedemptionRepositoryError::Query)?;
        if update.rows_affected == 0 {
            return Ok(None);
        }
        if update.rows_affected != 1 {
            return Err(RedemptionRepositoryError::Invariant);
        }
    }
    let mut query =
        users::Entity::find_by_id(user_id.get()).filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| RedemptionRepositoryError::Query)
}

async fn lock_batch_by_key(
    transaction: &DatabaseTransaction,
    batch_id: RedemptionBatchId,
) -> Result<Option<redemption_batches::Model>, RedemptionRepositoryError> {
    let mut query = redemption_batches::Entity::find().filter(
        redemption_batches::Column::BatchKey.eq(SensitiveString::from(batch_id.persistence_key())),
    );
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| RedemptionRepositoryError::Query)
}

async fn lock_batch_by_database_id(
    transaction: &DatabaseTransaction,
    database_id: i64,
) -> Result<Option<redemption_batches::Model>, RedemptionRepositoryError> {
    let mut query = redemption_batches::Entity::find_by_id(database_id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| RedemptionRepositoryError::Query)
}

async fn lock_code_by_digest(
    transaction: &DatabaseTransaction,
    digest: RedemptionCodeDigest,
) -> Result<Option<redemption_codes::Model>, RedemptionRepositoryError> {
    let mut query = redemption_codes::Entity::find().filter(
        redemption_codes::Column::CodeSha256.eq(SensitiveString::from(digest.persistence_key())),
    );
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| RedemptionRepositoryError::Query)
}

fn batch_record(
    model: redemption_batches::Model,
) -> Result<RedemptionBatchRecord, RedemptionRepositoryError> {
    let batch_id = RedemptionBatchId::from_persistence_key(model.batch_key.as_str())
        .map_err(|_| RedemptionRepositoryError::Invariant)?;
    let created_by_user_id =
        UserId::new(model.created_by_user_id).map_err(|_| RedemptionRepositoryError::Invariant)?;
    let status = RedemptionBatchStatus::try_from(model.status)
        .map_err(|_| RedemptionRepositoryError::Invariant)?;
    let quota_amount =
        Quota::new(model.quota_amount).map_err(|_| RedemptionRepositoryError::Invariant)?;
    let code_count =
        usize::try_from(model.code_count).map_err(|_| RedemptionRepositoryError::Invariant)?;
    let version = u64::try_from(model.version).map_err(|_| RedemptionRepositoryError::Invariant)?;
    let expires_at = optional_unix_seconds(model.expires_at)?;
    let disabled_at = optional_unix_seconds(model.disabled_at)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    let valid_state = match status {
        RedemptionBatchStatus::Active => disabled_at.is_none(),
        RedemptionBatchStatus::Disabled => disabled_at.is_some(),
    };
    if model.id <= 0
        || !valid_name(&model.name)
        || quota_amount.is_zero()
        || !(1..=MAX_REDEMPTION_BATCH_CODES).contains(&code_count)
        || version == 0
        || expires_at.is_some_and(|value| value <= created_at)
        || disabled_at.is_some_and(|value| value < created_at)
        || disabled_at.is_some_and(|value| value > updated_at)
        || updated_at < created_at
        || !valid_state
    {
        return Err(RedemptionRepositoryError::Invariant);
    }
    Ok(RedemptionBatchRecord {
        database_id: model.id,
        batch_id,
        name: model.name,
        created_by_user_id,
        status,
        quota_amount,
        code_count,
        version,
        expires_at,
        disabled_at,
        created_at,
        updated_at,
    })
}

fn code_record(model: redemption_codes::Model) -> Result<StoredCode, RedemptionRepositoryError> {
    let code_id = RedemptionCodeId::from_persistence_key(model.code_key.as_str())
        .map_err(|_| RedemptionRepositoryError::Invariant)?;
    let wallet_event =
        WalletEventId::new(code_id.bytes()).map_err(|_| RedemptionRepositoryError::Invariant)?;
    let digest = RedemptionCodeDigest::from_persistence_key(model.code_sha256.as_str())
        .map_err(|_| RedemptionRepositoryError::Invariant)?;
    let status = RedemptionCodeStatus::try_from(model.status)
        .map_err(|_| RedemptionRepositoryError::Invariant)?;
    let used_by_user_id = model
        .used_by_user_id
        .map(UserId::new)
        .transpose()
        .map_err(|_| RedemptionRepositoryError::Invariant)?;
    let redeemed_at = optional_unix_seconds(model.redeemed_at)?;
    let created_at = unix_seconds(model.created_at)?;
    let valid_state = match status {
        RedemptionCodeStatus::Available => used_by_user_id.is_none() && redeemed_at.is_none(),
        RedemptionCodeStatus::Redeemed => {
            used_by_user_id.is_some() && redeemed_at.is_some_and(|value| value >= created_at)
        }
    };
    if model.id <= 0 || model.batch_id <= 0 || wallet_event.is_system_opening() || !valid_state {
        return Err(RedemptionRepositoryError::Invariant);
    }
    Ok(StoredCode {
        database_id: model.id,
        code_id,
        batch_database_id: model.batch_id,
        digest,
        status,
        used_by_user_id,
        redeemed_at,
        created_at,
    })
}

fn to_database_time(value: u64) -> Result<TimeDateTimeWithTimeZone, RedemptionRepositoryError> {
    let value = i64::try_from(value).map_err(|_| RedemptionRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| RedemptionRepositoryError::Invariant)
}

fn unix_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, RedemptionRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| RedemptionRepositoryError::Invariant)
}

fn optional_unix_seconds(
    value: Option<TimeDateTimeWithTimeZone>,
) -> Result<Option<u64>, RedemptionRepositoryError> {
    value.map(unix_seconds).transpose()
}

fn monotonic_updated_at(
    current: TimeDateTimeWithTimeZone,
    business_time: TimeDateTimeWithTimeZone,
) -> TimeDateTimeWithTimeZone {
    // 审计更新时间不能因跨主机时钟偏差或历史业务时间发生回拨。
    current
        .max(TimeDateTimeWithTimeZone::now_utc())
        .max(business_time)
}

fn is_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_redemption_batches_batch_key")
        || rendered.contains("uq_redemption_codes_code_key")
        || rendered.contains("uq_redemption_codes_sha256")
        || rendered.contains("redemption_batches.batch_key")
        || rendered.contains("redemption_codes.code_key")
        || rendered.contains("redemption_codes.code_sha256")
        || rendered.contains("Duplicate entry")
}

async fn begin(pool: &DatabasePool) -> Result<DatabaseTransaction, RedemptionRepositoryError> {
    pool.connection()
        .begin()
        .await
        .map_err(|_| RedemptionRepositoryError::Query)
}

async fn commit(transaction: DatabaseTransaction) -> Result<(), RedemptionRepositoryError> {
    transaction
        .commit()
        .await
        .map_err(|_| RedemptionRepositoryError::OutcomeUnknown)
}

async fn rollback(transaction: DatabaseTransaction) -> Result<(), RedemptionRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| RedemptionRepositoryError::OutcomeUnknown)
}

/// 仅记录闭合内部分类，避免兑换码、摘要、主体和余额进入日志。
fn internal(error: RedemptionRepositoryError) -> RedemptionRepositoryError {
    let error_kind = match error {
        RedemptionRepositoryError::Conflict => return error,
        RedemptionRepositoryError::Query => "redemption_query",
        RedemptionRepositoryError::OutcomeUnknown => "redemption_outcome_unknown",
        RedemptionRepositoryError::Timeout => "redemption_timeout",
        RedemptionRepositoryError::Invariant => "redemption_invariant",
    };
    tracing::error!(
        target: "af_db::redemption",
        error_kind,
        "兑换码仓储发生内部错误"
    );
    error
}
