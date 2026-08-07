use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    IssuedRedemptionBatch, RedemptionAttempt, RedemptionBatchCreateOutcome, RedemptionBatchDisable,
    RedemptionBatchDisableOutcome, RedemptionBatchWrite, RedemptionOutcome, RedemptionRejection,
    RedemptionRepository, RedemptionRepositoryError,
};

use crate::{SessionPrincipal, SessionRole};

use super::audit::{AdminRedemptionAuditPage, AdminRedemptionAuditQuery, RedemptionAuditFuture};
use super::types::{
    AdminRedemptionBatch, AdminRedemptionBatchCreateCommand, AdminRedemptionBatchDisableCommand,
    AdminRedemptionBatchDisableResult, AdminRedemptionBatchListQuery, AdminRedemptionBatchPage,
    IssuedAdminRedemptionBatch, RedemptionCreateFuture, RedemptionDisableFuture,
    RedemptionListFuture, RedemptionRedeemFuture, RedemptionService, RedemptionServiceError,
    UserRedemptionCommand, UserRedemptionResult,
};

/// 使用原子兑换仓储的生产应用服务。
pub struct DatabaseRedemptionService {
    repository: RedemptionRepository,
}

impl DatabaseRedemptionService {
    /// 绑定已经配置数据库截止时间的兑换码仓储。
    #[must_use]
    pub const fn new(repository: RedemptionRepository) -> Self {
        Self { repository }
    }
}

impl RedemptionService for DatabaseRedemptionService {
    fn audit<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminRedemptionAuditQuery,
    ) -> RedemptionAuditFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let query = query.into_repository_query(unix_now()?)?;
            let page = self
                .repository
                .audit_batches(&query)
                .await
                .map_err(map_read_error)?;
            Ok(AdminRedemptionAuditPage::from_record(page))
        })
    }

    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminRedemptionBatchListQuery,
    ) -> RedemptionListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let (records, next_cursor) = self
                .repository
                .list_batches(query.before, query.limit)
                .await
                .map_err(map_read_error)?
                .into_parts();
            let batches = records
                .iter()
                .map(|record| {
                    AdminRedemptionBatch::from_record(record.batch(), record.redeemed_count())
                })
                .collect();
            Ok(AdminRedemptionBatchPage::from_parts(batches, next_cursor))
        })
    }

    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminRedemptionBatchCreateCommand,
    ) -> RedemptionCreateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let created_at = unix_now()?;
            if command
                .expires_at
                .is_some_and(|expires_at| expires_at <= created_at)
            {
                return Err(RedemptionServiceError::InvalidInput);
            }
            let issued = IssuedRedemptionBatch::generate(command.code_count)
                .map_err(|_| RedemptionServiceError::Internal)?;
            let write = RedemptionBatchWrite::new(
                issued.batch_id(),
                command.name,
                principal.user_id(),
                command.quota_amount,
                issued.definitions(),
                command.expires_at,
                created_at,
            )
            .map_err(|_| RedemptionServiceError::InvalidInput)?;
            let outcome = recover_create(&self.repository, &write).await?;
            let record = match outcome {
                RedemptionBatchCreateOutcome::Created(record)
                | RedemptionBatchCreateOutcome::Existing(record) => record,
                RedemptionBatchCreateOutcome::CreatorNotFound => {
                    return Err(RedemptionServiceError::InvalidSession);
                }
            };
            Ok(IssuedAdminRedemptionBatch::from_parts(
                AdminRedemptionBatch::from_record(&record, 0),
                issued.into_codes(),
            ))
        })
    }

    fn disable<'a>(
        &'a self,
        principal: SessionPrincipal,
        batch_id: af_domain::RedemptionBatchId,
        command: AdminRedemptionBatchDisableCommand,
    ) -> RedemptionDisableFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let write =
                RedemptionBatchDisable::new(batch_id, command.expected_version, unix_now()?)
                    .map_err(|_| RedemptionServiceError::InvalidInput)?;
            let outcome = recover_disable(&self.repository, &write).await?;
            match outcome {
                RedemptionBatchDisableOutcome::Applied(record)
                | RedemptionBatchDisableOutcome::Existing(record) => {
                    AdminRedemptionBatchDisableResult::from_record(&record)
                }
                RedemptionBatchDisableOutcome::NotFound => {
                    Err(RedemptionServiceError::BatchNotFound)
                }
            }
        })
    }

    fn redeem<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserRedemptionCommand,
    ) -> RedemptionRedeemFuture<'a> {
        Box::pin(async move {
            let attempt = RedemptionAttempt::new(principal.user_id(), command.code, unix_now()?)
                .map_err(|_| RedemptionServiceError::Internal)?;
            match recover_redeem(&self.repository, &attempt).await? {
                RedemptionOutcome::Applied(record) => Ok(UserRedemptionResult::new(
                    record.quota_amount(),
                    record.balance_after(),
                    record.redeemed_at(),
                    false,
                )),
                RedemptionOutcome::Existing(record) => Ok(UserRedemptionResult::new(
                    record.quota_amount(),
                    record.balance_after(),
                    record.redeemed_at(),
                    true,
                )),
                RedemptionOutcome::Rejected(reason) => Err(map_rejection(reason)),
                RedemptionOutcome::UserNotFound => Err(RedemptionServiceError::InvalidSession),
            }
        })
    }
}

impl fmt::Debug for DatabaseRedemptionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseRedemptionService(<redacted>)")
    }
}

async fn recover_create(
    repository: &RedemptionRepository,
    write: &RedemptionBatchWrite,
) -> Result<RedemptionBatchCreateOutcome, RedemptionServiceError> {
    match repository.create_batch(write).await {
        Err(RedemptionRepositoryError::OutcomeUnknown) => repository
            .create_batch(write)
            .await
            .map_err(map_write_error),
        result => result.map_err(map_write_error),
    }
}

async fn recover_disable(
    repository: &RedemptionRepository,
    write: &RedemptionBatchDisable,
) -> Result<RedemptionBatchDisableOutcome, RedemptionServiceError> {
    match repository.disable_batch(write).await {
        Err(RedemptionRepositoryError::OutcomeUnknown) => repository
            .disable_batch(write)
            .await
            .map_err(map_write_error),
        result => result.map_err(map_write_error),
    }
}

async fn recover_redeem(
    repository: &RedemptionRepository,
    attempt: &RedemptionAttempt,
) -> Result<RedemptionOutcome, RedemptionServiceError> {
    match repository.redeem(attempt).await {
        Err(RedemptionRepositoryError::OutcomeUnknown) => {
            repository.redeem(attempt).await.map_err(map_redeem_error)
        }
        result => result.map_err(map_redeem_error),
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), RedemptionServiceError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(RedemptionServiceError::Forbidden)
    }
}

fn unix_now() -> Result<u64, RedemptionServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| RedemptionServiceError::Internal)
}

fn map_read_error(error: RedemptionRepositoryError) -> RedemptionServiceError {
    match error {
        RedemptionRepositoryError::OutcomeUnknown => RedemptionServiceError::OutcomeUnknown,
        RedemptionRepositoryError::Conflict
        | RedemptionRepositoryError::Query
        | RedemptionRepositoryError::Timeout
        | RedemptionRepositoryError::Invariant => RedemptionServiceError::Internal,
    }
}

fn map_write_error(error: RedemptionRepositoryError) -> RedemptionServiceError {
    match error {
        RedemptionRepositoryError::Conflict => RedemptionServiceError::BatchConflict,
        RedemptionRepositoryError::OutcomeUnknown => RedemptionServiceError::OutcomeUnknown,
        RedemptionRepositoryError::Query
        | RedemptionRepositoryError::Timeout
        | RedemptionRepositoryError::Invariant => RedemptionServiceError::Internal,
    }
}

fn map_redeem_error(error: RedemptionRepositoryError) -> RedemptionServiceError {
    match error {
        RedemptionRepositoryError::OutcomeUnknown => RedemptionServiceError::OutcomeUnknown,
        RedemptionRepositoryError::Conflict
        | RedemptionRepositoryError::Query
        | RedemptionRepositoryError::Timeout
        | RedemptionRepositoryError::Invariant => RedemptionServiceError::Internal,
    }
}

fn map_rejection(rejection: RedemptionRejection) -> RedemptionServiceError {
    match rejection {
        RedemptionRejection::InvalidCode => RedemptionServiceError::CodeInvalid,
        RedemptionRejection::BatchDisabled => RedemptionServiceError::BatchDisabled,
        RedemptionRejection::Expired => RedemptionServiceError::CodeExpired,
        RedemptionRejection::AlreadyUsed => RedemptionServiceError::CodeAlreadyUsed,
        RedemptionRejection::CreditOverflow => RedemptionServiceError::BalanceOverflow,
        RedemptionRejection::TimingConflict => RedemptionServiceError::Internal,
    }
}
