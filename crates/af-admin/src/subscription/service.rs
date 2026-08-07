use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    SubscriptionPlanCreateOutcome, SubscriptionPlanDisable, SubscriptionPlanDisableOutcome,
    SubscriptionPlanWrite, SubscriptionRepository, SubscriptionRepositoryError,
    UserSubscriptionBind, UserSubscriptionBindOutcome,
};
use af_domain::{
    SubscriptionPlanId, SubscriptionPlanStatus, SubscriptionWindow, UserId, UserSubscriptionId,
};

use crate::{SessionPrincipal, SessionRole};

use super::types::{
    AdminSubscriptionPageQuery, AdminSubscriptionPlan, AdminSubscriptionPlanCreateCommand,
    AdminSubscriptionPlanDisableCommand, AdminSubscriptionPlanPage, AdminUserSubscription,
    AdminUserSubscriptionBindCommand, AdminUserSubscriptionPage, SubscriptionBindFuture,
    SubscriptionCreatePlanFuture, SubscriptionDisablePlanFuture, SubscriptionListPlansFuture,
    SubscriptionListUserFuture, SubscriptionService, SubscriptionServiceError,
};

/// 使用订阅仓储的生产应用服务。
pub struct DatabaseSubscriptionService {
    repository: SubscriptionRepository,
}

impl DatabaseSubscriptionService {
    /// 使用订阅仓储构造生产应用服务。
    #[must_use]
    pub const fn new(repository: SubscriptionRepository) -> Self {
        Self { repository }
    }
}

impl SubscriptionService for DatabaseSubscriptionService {
    fn list_plans<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminSubscriptionPageQuery,
    ) -> SubscriptionListPlansFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .repository
                .list_plans(query.before, query.limit)
                .await
                .map_err(map_read_error)?;
            Ok(AdminSubscriptionPlanPage::new(
                page.plans()
                    .iter()
                    .map(AdminSubscriptionPlan::from_record)
                    .collect(),
                page.next_cursor(),
            ))
        })
    }

    fn create_plan<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminSubscriptionPlanCreateCommand,
    ) -> SubscriptionCreatePlanFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let created_at = unix_now()?;
            let write = SubscriptionPlanWrite::new(
                random_plan_id()?,
                command.name,
                principal.user_id(),
                command.quota_amount,
                command.cycle,
                created_at,
            )
            .map_err(|_| SubscriptionServiceError::InvalidInput)?;
            let record = match recover_create(&self.repository, &write).await? {
                SubscriptionPlanCreateOutcome::Created(record)
                | SubscriptionPlanCreateOutcome::Existing(record) => record,
                SubscriptionPlanCreateOutcome::CreatorNotFound => {
                    return Err(SubscriptionServiceError::InvalidSession);
                }
            };
            Ok(AdminSubscriptionPlan::from_record(&record))
        })
    }

    fn disable_plan<'a>(
        &'a self,
        principal: SessionPrincipal,
        plan_id: SubscriptionPlanId,
        command: AdminSubscriptionPlanDisableCommand,
    ) -> SubscriptionDisablePlanFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let write =
                SubscriptionPlanDisable::new(plan_id, command.expected_version, unix_now()?)
                    .map_err(|_| SubscriptionServiceError::InvalidInput)?;
            let record = match recover_disable(&self.repository, &write).await? {
                SubscriptionPlanDisableOutcome::Applied(record)
                | SubscriptionPlanDisableOutcome::Existing(record) => record,
                SubscriptionPlanDisableOutcome::NotFound => {
                    return Err(SubscriptionServiceError::PlanNotFound);
                }
            };
            Ok(AdminSubscriptionPlan::from_record(&record))
        })
    }

    fn list_user_subscriptions<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        query: AdminSubscriptionPageQuery,
    ) -> SubscriptionListUserFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            list_user_page(&self.repository, user_id, query).await
        })
    }

    fn list_current_subscriptions<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminSubscriptionPageQuery,
    ) -> SubscriptionListUserFuture<'a> {
        Box::pin(async move { list_user_page(&self.repository, principal.user_id(), query).await })
    }

    fn bind_user<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        command: AdminUserSubscriptionBindCommand,
    ) -> SubscriptionBindFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let plan = self
                .repository
                .get_plan(command.plan_id)
                .await
                .map_err(map_read_error)?
                .ok_or(SubscriptionServiceError::PlanNotFound)?;
            if plan.status() != SubscriptionPlanStatus::Active {
                return Err(SubscriptionServiceError::PlanDisabled);
            }
            let bound_at = unix_now()?;
            let window = SubscriptionWindow::initial(plan.cycle(), bound_at)
                .map_err(|_| SubscriptionServiceError::Internal)?;
            let bind = UserSubscriptionBind::new(
                random_subscription_id()?,
                user_id,
                command.plan_id,
                window.started_at(),
                window.ends_at(),
                bound_at,
            )
            .map_err(|_| SubscriptionServiceError::InvalidInput)?;
            let record = match recover_bind(&self.repository, &bind).await? {
                UserSubscriptionBindOutcome::Created(record)
                | UserSubscriptionBindOutcome::Existing(record) => record,
                UserSubscriptionBindOutcome::UserNotFound => {
                    return Err(SubscriptionServiceError::UserNotFound);
                }
                UserSubscriptionBindOutcome::PlanNotFound => {
                    return Err(SubscriptionServiceError::PlanNotFound);
                }
                UserSubscriptionBindOutcome::PlanDisabled => {
                    return Err(SubscriptionServiceError::PlanDisabled);
                }
            };
            Ok(AdminUserSubscription::from_record(&record))
        })
    }
}

impl fmt::Debug for DatabaseSubscriptionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseSubscriptionService(<redacted>)")
    }
}

async fn list_user_page(
    repository: &SubscriptionRepository,
    user_id: UserId,
    query: AdminSubscriptionPageQuery,
) -> Result<AdminUserSubscriptionPage, SubscriptionServiceError> {
    let page = repository
        .list_user_subscriptions(user_id, query.before, query.limit)
        .await
        .map_err(map_read_error)?;
    Ok(AdminUserSubscriptionPage::new(
        page.subscriptions()
            .iter()
            .map(AdminUserSubscription::from_record)
            .collect(),
        page.next_cursor(),
    ))
}

async fn recover_create(
    repository: &SubscriptionRepository,
    write: &SubscriptionPlanWrite,
) -> Result<SubscriptionPlanCreateOutcome, SubscriptionServiceError> {
    match repository.create_plan(write).await {
        Err(SubscriptionRepositoryError::OutcomeUnknown) => {
            repository.create_plan(write).await.map_err(map_write_error)
        }
        result => result.map_err(map_write_error),
    }
}

async fn recover_disable(
    repository: &SubscriptionRepository,
    write: &SubscriptionPlanDisable,
) -> Result<SubscriptionPlanDisableOutcome, SubscriptionServiceError> {
    match repository.disable_plan(write).await {
        Err(SubscriptionRepositoryError::OutcomeUnknown) => repository
            .disable_plan(write)
            .await
            .map_err(map_write_error),
        result => result.map_err(map_write_error),
    }
}

async fn recover_bind(
    repository: &SubscriptionRepository,
    bind: &UserSubscriptionBind,
) -> Result<UserSubscriptionBindOutcome, SubscriptionServiceError> {
    match repository.bind_user(bind).await {
        Err(SubscriptionRepositoryError::OutcomeUnknown) => {
            repository.bind_user(bind).await.map_err(map_write_error)
        }
        result => result.map_err(map_write_error),
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), SubscriptionServiceError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(SubscriptionServiceError::Forbidden)
    }
}

fn unix_now() -> Result<u64, SubscriptionServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| SubscriptionServiceError::Internal)
}

fn random_plan_id() -> Result<SubscriptionPlanId, SubscriptionServiceError> {
    loop {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| SubscriptionServiceError::Internal)?;
        if let Ok(plan_id) = SubscriptionPlanId::new(bytes) {
            return Ok(plan_id);
        }
    }
}

fn random_subscription_id() -> Result<UserSubscriptionId, SubscriptionServiceError> {
    loop {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| SubscriptionServiceError::Internal)?;
        if let Ok(subscription_id) = UserSubscriptionId::new(bytes) {
            return Ok(subscription_id);
        }
    }
}

fn map_read_error(error: SubscriptionRepositoryError) -> SubscriptionServiceError {
    match error {
        SubscriptionRepositoryError::Conflict
        | SubscriptionRepositoryError::Query
        | SubscriptionRepositoryError::Timeout
        | SubscriptionRepositoryError::OutcomeUnknown
        | SubscriptionRepositoryError::Invariant => SubscriptionServiceError::Internal,
    }
}

fn map_write_error(error: SubscriptionRepositoryError) -> SubscriptionServiceError {
    match error {
        SubscriptionRepositoryError::Conflict => SubscriptionServiceError::Conflict,
        SubscriptionRepositoryError::OutcomeUnknown => SubscriptionServiceError::OutcomeUnknown,
        SubscriptionRepositoryError::Query
        | SubscriptionRepositoryError::Timeout
        | SubscriptionRepositoryError::Invariant => SubscriptionServiceError::Internal,
    }
}
