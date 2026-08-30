use af_domain::{
    OrganizationBudgetLimit, OrganizationBudgetPeriod, OrganizationBudgetPolicyId,
    OrganizationBudgetScopeKind, OrganizationDepartmentId, OrganizationId,
    OrganizationMembershipId, OrganizationPermission, OrganizationRole, OrganizationTeamId, Quota,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
    sea_query::{Expr, LockType},
};

use crate::entity::{
    organization_budget_policies, organization_entitlements, organization_memberships,
    organization_teams, organizations,
};
use crate::organization_custom_role_binding::active_custom_role_permissions;

use super::types::{
    OrganizationBudgetPolicyRecord, OrganizationBudgetPolicyWrite,
    OrganizationBudgetRepositoryError, OrganizationBudgetScope,
};

pub(super) async fn create(
    transaction: &DatabaseTransaction,
    write: &OrganizationBudgetPolicyWrite,
    expected_version: Option<i64>,
) -> Result<OrganizationBudgetPolicyRecord, OrganizationBudgetRepositoryError> {
    if expected_version.is_some_and(|version| version < 0) {
        return Err(OrganizationBudgetRepositoryError::InvalidInput);
    }
    lock_organization(transaction, write.organization_id).await?;
    require_budget_manager(transaction, write.organization_id, write.actor_user_id).await?;
    validate_scope(transaction, write.organization_id, write.scope).await?;

    let previous = latest_policy(transaction, write.organization_id, write.scope, true).await?;
    let current_version = previous.as_ref().map_or(0, |policy| policy.version);
    if expected_version.is_some_and(|version| version != current_version) {
        return Err(OrganizationBudgetRepositoryError::Conflict);
    }
    let version = match previous.as_ref() {
        Some(previous) => previous
            .version
            .checked_add(1)
            .ok_or(OrganizationBudgetRepositoryError::Invariant)?,
        None => 1,
    };
    if let Some(previous) = previous {
        if write.effective_from <= previous.effective_from {
            return Err(OrganizationBudgetRepositoryError::Conflict);
        }
        close_previous(transaction, previous, write.effective_from).await?;
    }

    let now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
    let model = organization_budget_policies::ActiveModel {
        organization_id: Set(write.organization_id.get()),
        scope_kind: Set(write.scope.kind().code()),
        team_id: Set(write.scope.team_id().map(OrganizationTeamId::get)),
        membership_id: Set(write
            .scope
            .membership_id()
            .map(OrganizationMembershipId::get)),
        department_id: Set(write
            .scope
            .department_id()
            .map(OrganizationDepartmentId::get)),
        period_kind: Set(write.period.code()),
        fixed_period_seconds: Set(write.period.fixed_seconds().map(i64::from)),
        quota_limit: Set(write.limit.quota().map(Quota::units)),
        effective_from: Set(write.effective_from),
        effective_until: Set(None),
        created_by_user_id: Set(write.actor_user_id.get()),
        version: Set(version),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    record(model)
}

pub(super) async fn list(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    actor_user_id: af_domain::UserId,
) -> Result<Vec<OrganizationBudgetPolicyRecord>, OrganizationBudgetRepositoryError> {
    if !organizations::Entity::find_by_id(organization_id.get())
        .filter(organizations::Column::Status.eq(1_i16))
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .is_some()
    {
        return Err(OrganizationBudgetRepositoryError::NotFound);
    }
    require_budget_manager(transaction, organization_id, actor_user_id).await?;
    organization_budget_policies::Entity::find()
        .filter(organization_budget_policies::Column::OrganizationId.eq(organization_id.get()))
        .order_by_asc(organization_budget_policies::Column::ScopeKind)
        .order_by_asc(organization_budget_policies::Column::TeamId)
        .order_by_asc(organization_budget_policies::Column::MembershipId)
        .order_by_asc(organization_budget_policies::Column::DepartmentId)
        .order_by_desc(organization_budget_policies::Column::EffectiveFrom)
        .all(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .into_iter()
        .map(record)
        .collect()
}

pub(super) async fn active_policy(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    scope: OrganizationBudgetScope,
    now: sea_orm::entity::prelude::TimeDateTimeWithTimeZone,
) -> Result<Option<organization_budget_policies::Model>, OrganizationBudgetRepositoryError> {
    let mut query = scoped_query(organization_id, scope)
        .filter(organization_budget_policies::Column::EffectiveFrom.lte(now))
        .filter(
            organization_budget_policies::Column::EffectiveUntil
                .is_null()
                .or(organization_budget_policies::Column::EffectiveUntil.gt(now)),
        )
        .order_by_desc(organization_budget_policies::Column::EffectiveFrom)
        .limit(2);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let policies = query
        .all(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    match policies.as_slice() {
        [] => Ok(None),
        [policy] => Ok(Some(policy.clone())),
        _ => Err(OrganizationBudgetRepositoryError::Invariant),
    }
}

async fn latest_policy(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    scope: OrganizationBudgetScope,
    lock: bool,
) -> Result<Option<organization_budget_policies::Model>, OrganizationBudgetRepositoryError> {
    let mut query = scoped_query(organization_id, scope)
        .order_by_desc(organization_budget_policies::Column::EffectiveFrom)
        .limit(1);
    if lock && transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)
}

fn scoped_query(
    organization_id: OrganizationId,
    scope: OrganizationBudgetScope,
) -> sea_orm::Select<organization_budget_policies::Entity> {
    let query = organization_budget_policies::Entity::find()
        .filter(organization_budget_policies::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_budget_policies::Column::ScopeKind.eq(scope.kind().code()));
    match scope {
        OrganizationBudgetScope::Organization => query
            .filter(organization_budget_policies::Column::TeamId.is_null())
            .filter(organization_budget_policies::Column::MembershipId.is_null())
            .filter(organization_budget_policies::Column::DepartmentId.is_null()),
        OrganizationBudgetScope::Team(team_id) => query
            .filter(organization_budget_policies::Column::TeamId.eq(team_id.get()))
            .filter(organization_budget_policies::Column::MembershipId.is_null())
            .filter(organization_budget_policies::Column::DepartmentId.is_null()),
        OrganizationBudgetScope::Member(membership_id) => query
            .filter(organization_budget_policies::Column::TeamId.is_null())
            .filter(organization_budget_policies::Column::MembershipId.eq(membership_id.get()))
            .filter(organization_budget_policies::Column::DepartmentId.is_null()),
        OrganizationBudgetScope::Department(department_id) => query
            .filter(organization_budget_policies::Column::TeamId.is_null())
            .filter(organization_budget_policies::Column::MembershipId.is_null())
            .filter(organization_budget_policies::Column::DepartmentId.eq(department_id.get())),
    }
}

async fn close_previous(
    transaction: &DatabaseTransaction,
    previous: organization_budget_policies::Model,
    effective_until: sea_orm::entity::prelude::TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationBudgetRepositoryError> {
    if previous
        .effective_until
        .is_some_and(|current| current <= effective_until)
    {
        return Ok(());
    }
    let result = organization_budget_policies::Entity::update_many()
        .filter(organization_budget_policies::Column::Id.eq(previous.id))
        .filter(organization_budget_policies::Column::Version.eq(previous.version))
        .col_expr(
            organization_budget_policies::Column::EffectiveUntil,
            Expr::value(effective_until),
        )
        .col_expr(
            organization_budget_policies::Column::UpdatedAt,
            Expr::value(sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc()),
        )
        .exec(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(OrganizationBudgetRepositoryError::Conflict);
    }
    Ok(())
}

async fn lock_organization(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<(), OrganizationBudgetRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = organizations::Entity::update_many()
            .filter(organizations::Column::Id.eq(organization_id.get()))
            .filter(organizations::Column::Status.eq(1_i16))
            .col_expr(
                organizations::Column::Version,
                Expr::col(organizations::Column::Version).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        if result.rows_affected != 1 {
            return Err(OrganizationBudgetRepositoryError::NotFound);
        }
        return Ok(());
    }
    let found = organizations::Entity::find_by_id(organization_id.get())
        .filter(organizations::Column::Status.eq(1_i16))
        .lock(LockType::Update)
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    found
        .is_some()
        .then_some(())
        .ok_or(OrganizationBudgetRepositoryError::NotFound)
}

pub(super) async fn require_budget_manager(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    actor_user_id: af_domain::UserId,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let membership = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(actor_user_id.get()))
        .filter(organization_memberships::Column::Status.eq(1_i16))
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .ok_or(OrganizationBudgetRepositoryError::PermissionDenied)?;
    let role = OrganizationRole::try_from(membership.role)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    if role.allows(OrganizationPermission::BudgetsManage) {
        return Ok(());
    }
    let entitlement = organization_entitlements::Entity::find()
        .filter(organization_entitlements::Column::OrganizationId.eq(organization_id.get()))
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let custom =
        active_custom_role_permissions(transaction, &membership, entitlement.custom_roles_enabled)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    custom
        .as_deref()
        .is_some_and(|permissions| permissions.contains(&OrganizationPermission::BudgetsManage))
        .then_some(())
        .ok_or(OrganizationBudgetRepositoryError::PermissionDenied)
}

async fn validate_scope(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    scope: OrganizationBudgetScope,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let valid = match scope {
        OrganizationBudgetScope::Organization => true,
        OrganizationBudgetScope::Team(team_id) => {
            organization_teams::Entity::find_by_id(team_id.get())
                .filter(organization_teams::Column::OrganizationId.eq(organization_id.get()))
                .filter(organization_teams::Column::Status.eq(1_i16))
                .one(transaction)
                .await
                .map_err(|_| OrganizationBudgetRepositoryError::Query)?
                .is_some()
        }
        OrganizationBudgetScope::Member(membership_id) => {
            organization_memberships::Entity::find_by_id(membership_id.get())
                .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
                .filter(organization_memberships::Column::Status.eq(1_i16))
                .one(transaction)
                .await
                .map_err(|_| OrganizationBudgetRepositoryError::Query)?
                .is_some()
        }
        OrganizationBudgetScope::Department(department_id) => {
            use crate::entity::organization_departments;
            organization_departments::Entity::find_by_id(department_id.get())
                .filter(organization_departments::Column::OrganizationId.eq(organization_id.get()))
                .filter(organization_departments::Column::Status.eq(1_i16))
                .one(transaction)
                .await
                .map_err(|_| OrganizationBudgetRepositoryError::Query)?
                .is_some()
        }
    };
    valid
        .then_some(())
        .ok_or(OrganizationBudgetRepositoryError::NotFound)
}

pub(super) fn record(
    model: organization_budget_policies::Model,
) -> Result<OrganizationBudgetPolicyRecord, OrganizationBudgetRepositoryError> {
    let kind = OrganizationBudgetScopeKind::try_from(model.scope_kind)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let scope = match (
        kind,
        model.team_id,
        model.membership_id,
        model.department_id,
    ) {
        (OrganizationBudgetScopeKind::Organization, None, None, None) => {
            OrganizationBudgetScope::Organization
        }
        (OrganizationBudgetScopeKind::Team, Some(team_id), None, None) => {
            OrganizationBudgetScope::Team(
                OrganizationTeamId::new(team_id)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
            )
        }
        (OrganizationBudgetScopeKind::Member, None, Some(membership_id), None) => {
            OrganizationBudgetScope::Member(
                OrganizationMembershipId::new(membership_id)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
            )
        }
        (OrganizationBudgetScopeKind::Department, None, None, Some(department_id)) => {
            OrganizationBudgetScope::Department(
                OrganizationDepartmentId::new(department_id)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
            )
        }
        _ => return Err(OrganizationBudgetRepositoryError::Invariant),
    };
    let fixed_seconds = model
        .fixed_period_seconds
        .map(u32::try_from)
        .transpose()
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let period = OrganizationBudgetPeriod::from_persistence(model.period_kind, fixed_seconds)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let quota_limit = model
        .quota_limit
        .map(Quota::new)
        .transpose()
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let limit = OrganizationBudgetLimit::new(quota_limit)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    if model.version <= 0
        || model
            .effective_until
            .is_some_and(|until| until <= model.effective_from)
    {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(OrganizationBudgetPolicyRecord {
        id: OrganizationBudgetPolicyId::new(model.id)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        organization_id: OrganizationId::new(model.organization_id)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        scope,
        period,
        limit,
        effective_from: model.effective_from,
        effective_until: model.effective_until,
        version: model.version,
    })
}
