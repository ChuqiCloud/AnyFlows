use std::{future::Future, pin::Pin};

use af_db::{
    OrganizationTokenAuthContext, OrganizationTokenAuthRecord, OrganizationTokenAuthRepository,
    OrganizationTokenAuthValidation, OrganizationTokenAuthValidator, TokenAuthRepositoryError,
};
use af_domain::{
    OrganizationDepartmentId, OrganizationDepartmentStatus, OrganizationEntitlementCapacity,
    OrganizationEntitlementSnapshot, OrganizationGatewayPrincipal, OrganizationMembershipId,
    OrganizationMembershipStatus, OrganizationStatus, OrganizationTeamId, OrganizationTeamStatus,
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

/// 将企业数据库读取事实接入公共鉴权端口。
///
/// 数据库查询和企业状态判定分开，发行版可以替换整个适配器而不改变公共
/// `TokenAuthRepository` 的调用契约。
#[derive(Clone, Debug)]
pub(super) struct DatabaseOrganizationTokenAuthValidator {
    repository: OrganizationTokenAuthRepository,
}

impl DatabaseOrganizationTokenAuthValidator {
    #[must_use]
    pub(super) const fn new(pool: af_db::DatabasePool) -> Self {
        Self {
            repository: OrganizationTokenAuthRepository::new(pool),
        }
    }

    async fn validate_inner(
        &self,
        context: OrganizationTokenAuthContext,
    ) -> Result<OrganizationTokenAuthValidation, TokenAuthRepositoryError> {
        let Some(record) = self.repository.lookup(context).await? else {
            return Ok(OrganizationTokenAuthValidation::Rejected);
        };
        validate_record(&self.repository, record, context).await
    }
}

impl OrganizationTokenAuthValidator for DatabaseOrganizationTokenAuthValidator {
    fn validate<'a>(
        &'a self,
        context: OrganizationTokenAuthContext,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<OrganizationTokenAuthValidation, TokenAuthRepositoryError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(
            self.validate_inner(context)
                .with_subscriber(NoSubscriber::default()),
        )
    }
}

async fn validate_record(
    repository: &OrganizationTokenAuthRepository,
    record: OrganizationTokenAuthRecord,
    context: OrganizationTokenAuthContext,
) -> Result<OrganizationTokenAuthValidation, TokenAuthRepositoryError> {
    let organization_value = context.organization_id.get();
    let organization_id = context.organization_id;
    let membership_value = context.membership_id.get();
    let user_id = context.user_id;
    if record.membership_id != Some(membership_value)
        || record.membership_organization_id != Some(organization_value)
        || record.membership_user_id != Some(user_id.get())
        || record.membership_team_id != context.team_id.map(|id| id.get())
    {
        return Ok(OrganizationTokenAuthValidation::Rejected);
    }
    let membership_status = OrganizationMembershipStatus::try_from(
        record
            .membership_status
            .ok_or(TokenAuthRepositoryError::Invariant)?,
    )
    .map_err(|_| TokenAuthRepositoryError::Invariant)?;
    if !membership_status.is_active() {
        return Ok(OrganizationTokenAuthValidation::Rejected);
    }
    let organization_status = OrganizationStatus::try_from(
        record
            .organization_status
            .ok_or(TokenAuthRepositoryError::Invariant)?,
    )
    .map_err(|_| TokenAuthRepositoryError::Invariant)?;
    let capacity = OrganizationEntitlementCapacity::new(
        record
            .entitlement_seat_limit
            .ok_or(TokenAuthRepositoryError::Invariant)?,
        record
            .entitlement_team_limit
            .ok_or(TokenAuthRepositoryError::Invariant)?,
        record
            .entitlement_organization_key_limit
            .ok_or(TokenAuthRepositoryError::Invariant)?,
    )
    .map_err(|_| TokenAuthRepositoryError::Invariant)?;
    let entitlement = OrganizationEntitlementSnapshot::new(
        capacity,
        record
            .entitlement_team_management_enabled
            .ok_or(TokenAuthRepositoryError::Invariant)?,
        record
            .entitlement_sso_enabled
            .ok_or(TokenAuthRepositoryError::Invariant)?,
        record
            .entitlement_custom_roles_enabled
            .ok_or(TokenAuthRepositoryError::Invariant)?,
        record
            .entitlement_audit_retention_days
            .ok_or(TokenAuthRepositoryError::Invariant)?,
        record
            .entitlement_valid_from
            .ok_or(TokenAuthRepositoryError::Invariant)?
            .unix_timestamp(),
        record
            .entitlement_valid_until
            .ok_or(TokenAuthRepositoryError::Invariant)?
            .unix_timestamp(),
        record
            .entitlement_grace_until
            .map(|value| value.unix_timestamp()),
    )
    .map_err(|_| TokenAuthRepositoryError::Invariant)?;
    if !entitlement
        .status(
            organization_status,
            af_db::DatabaseTimestamp::now_utc().unix_timestamp(),
        )
        .allows_existing_consumption()
    {
        return Ok(OrganizationTokenAuthValidation::Rejected);
    }
    let team_id = match context.team_id.map(|id| id.get()) {
        Some(value) => {
            let status = OrganizationTeamStatus::try_from(
                record
                    .organization_team_status
                    .ok_or(TokenAuthRepositoryError::Invariant)?,
            )
            .map_err(|_| TokenAuthRepositoryError::Invariant)?;
            if !status.is_active()
                || record.organization_team_organization_id != Some(organization_value)
                || !entitlement.team_management_enabled()
            {
                return Ok(OrganizationTokenAuthValidation::Rejected);
            }
            Some(OrganizationTeamId::new(value).map_err(|_| TokenAuthRepositoryError::Invariant)?)
        }
        None => None,
    };
    let department_id = match context.department_id.map(|id| id.get()) {
        Some(value) => {
            let department_id = OrganizationDepartmentId::new(value)
                .map_err(|_| TokenAuthRepositoryError::Invariant)?;
            let Some(department) = repository.department(context, department_id).await? else {
                return Ok(OrganizationTokenAuthValidation::Rejected);
            };
            if OrganizationDepartmentStatus::try_from(department.status)
                .map_err(|_| TokenAuthRepositoryError::Invariant)?
                != OrganizationDepartmentStatus::Active
            {
                return Ok(OrganizationTokenAuthValidation::Rejected);
            }
            if !repository
                .has_active_department_membership(context, department_id)
                .await?
            {
                return Ok(OrganizationTokenAuthValidation::Rejected);
            }
            Some(department_id)
        }
        None => None,
    };
    let key_count = repository.key_count(context).await?;
    let key_limit = u64::try_from(capacity.organization_key_limit())
        .map_err(|_| TokenAuthRepositoryError::Invariant)?;
    if key_count > key_limit {
        return Ok(OrganizationTokenAuthValidation::Rejected);
    }
    let membership_id = OrganizationMembershipId::new(membership_value)
        .map_err(|_| TokenAuthRepositoryError::Invariant)?;
    Ok(OrganizationTokenAuthValidation::Authenticated(
        match department_id {
            Some(department_id) => OrganizationGatewayPrincipal::with_department(
                organization_id,
                membership_id,
                team_id,
                department_id,
            ),
            None => OrganizationGatewayPrincipal::new(organization_id, membership_id, team_id),
        },
    ))
}
