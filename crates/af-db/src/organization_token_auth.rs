use std::{future::Future, pin::Pin};

use af_domain::{
    OrganizationDepartmentId, OrganizationDepartmentMembershipStatus, OrganizationDepartmentStatus,
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationGatewayPrincipal,
    OrganizationMembershipId, OrganizationMembershipStatus, OrganizationStatus, OrganizationTeamId,
    OrganizationTeamStatus,
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, ExprTrait, Query, SelectStatement},
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, OrganizationTokenAuthContext, OrganizationTokenAuthValidation,
    OrganizationTokenAuthValidator, TokenAuthRepositoryError,
    entity::{
        organization_department_memberships, organization_departments, organization_entitlements,
        organization_memberships, organization_teams, organizations, tokens,
    },
};

/// 迁移期间保留的内置企业令牌校验实现。
///
/// 由企业运行时装配注入公共鉴权仓储；自身不接收 API Key 或摘要。
#[derive(Clone, Debug)]
pub struct DatabaseOrganizationTokenAuthValidator {
    pool: DatabasePool,
}

impl DatabaseOrganizationTokenAuthValidator {
    #[must_use]
    pub const fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    async fn validate_inner(
        &self,
        context: OrganizationTokenAuthContext,
    ) -> Result<OrganizationTokenAuthValidation, TokenAuthRepositoryError> {
        let connection = self.pool.connection();
        let statement = connection
            .get_database_backend()
            .build(&lookup_query(context));
        let mut results = connection
            .query_all(statement)
            .await
            .map_err(|_| TokenAuthRepositoryError::Query)?;
        let result = match results.len() {
            0 => return Ok(OrganizationTokenAuthValidation::Rejected),
            1 => results.pop().ok_or(TokenAuthRepositoryError::Invariant)?,
            _ => return Err(TokenAuthRepositoryError::Invariant),
        };
        let row = OrganizationTokenAuthRow::try_from_query_result(&result)
            .map_err(|_| TokenAuthRepositoryError::Invariant)?;
        row.validate(connection, context).await
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

fn lookup_query(context: OrganizationTokenAuthContext) -> SelectStatement {
    // 再次核对令牌归属，避免公共查询后归属变更仍使用旧企业身份。
    let mut query = Query::select()
        .expr_as(
            Expr::col((organizations::Entity, organizations::Column::Status)),
            Alias::new("organization_status"),
        )
        .expr_as(
            Expr::col((
                organization_memberships::Entity,
                organization_memberships::Column::Id,
            )),
            Alias::new("membership_id"),
        )
        .expr_as(
            Expr::col((
                organization_memberships::Entity,
                organization_memberships::Column::Status,
            )),
            Alias::new("membership_status"),
        )
        .expr_as(
            Expr::col((
                organization_memberships::Entity,
                organization_memberships::Column::OrganizationId,
            )),
            Alias::new("membership_organization_id"),
        )
        .expr_as(
            Expr::col((
                organization_memberships::Entity,
                organization_memberships::Column::UserId,
            )),
            Alias::new("membership_user_id"),
        )
        .expr_as(
            Expr::col((
                organization_memberships::Entity,
                organization_memberships::Column::TeamId,
            )),
            Alias::new("membership_team_id"),
        )
        .expr_as(
            Expr::col((
                organization_teams::Entity,
                organization_teams::Column::Status,
            )),
            Alias::new("organization_team_status"),
        )
        .expr_as(
            Expr::col((
                organization_teams::Entity,
                organization_teams::Column::OrganizationId,
            )),
            Alias::new("organization_team_organization_id"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::TeamManagementEnabled,
            )),
            Alias::new("entitlement_team_management_enabled"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::SeatLimit,
            )),
            Alias::new("entitlement_seat_limit"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::TeamLimit,
            )),
            Alias::new("entitlement_team_limit"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::OrganizationKeyLimit,
            )),
            Alias::new("entitlement_organization_key_limit"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::AuditRetentionDays,
            )),
            Alias::new("entitlement_audit_retention_days"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::SsoEnabled,
            )),
            Alias::new("entitlement_sso_enabled"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::CustomRolesEnabled,
            )),
            Alias::new("entitlement_custom_roles_enabled"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::ValidFrom,
            )),
            Alias::new("entitlement_valid_from"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::ValidUntil,
            )),
            Alias::new("entitlement_valid_until"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::GraceUntil,
            )),
            Alias::new("entitlement_grace_until"),
        )
        .from(tokens::Entity)
        .left_join(
            organizations::Entity,
            Expr::col((tokens::Entity, tokens::Column::OrganizationId))
                .equals((organizations::Entity, organizations::Column::Id)),
        )
        .left_join(
            organization_memberships::Entity,
            Expr::col((tokens::Entity, tokens::Column::OrganizationMembershipId)).equals((
                organization_memberships::Entity,
                organization_memberships::Column::Id,
            )),
        )
        .left_join(
            organization_teams::Entity,
            Expr::col((tokens::Entity, tokens::Column::OrganizationTeamId))
                .equals((organization_teams::Entity, organization_teams::Column::Id)),
        )
        .left_join(
            organization_entitlements::Entity,
            Expr::col((tokens::Entity, tokens::Column::OrganizationId)).equals((
                organization_entitlements::Entity,
                organization_entitlements::Column::OrganizationId,
            )),
        )
        .and_where(Expr::col((tokens::Entity, tokens::Column::Id)).eq(context.token_id.get()))
        .and_where(Expr::col((tokens::Entity, tokens::Column::UserId)).eq(context.user_id.get()))
        .and_where(
            Expr::col((tokens::Entity, tokens::Column::OrganizationId))
                .eq(context.organization_id.get()),
        )
        .and_where(
            Expr::col((tokens::Entity, tokens::Column::OrganizationMembershipId))
                .eq(context.membership_id.get()),
        )
        .limit(2)
        .to_owned();
    let team = Expr::col((tokens::Entity, tokens::Column::OrganizationTeamId));
    let department = Expr::col((tokens::Entity, tokens::Column::OrganizationDepartmentId));
    query.and_where(match context.team_id {
        Some(id) => team.eq(id.get()),
        None => team.is_null(),
    });
    query.and_where(match context.department_id {
        Some(id) => department.eq(id.get()),
        None => department.is_null(),
    });
    query
}

struct OrganizationTokenAuthRow {
    organization_status: Option<i16>,
    membership_id: Option<i64>,
    membership_status: Option<i16>,
    membership_organization_id: Option<i64>,
    membership_user_id: Option<i64>,
    membership_team_id: Option<i64>,
    organization_team_status: Option<i16>,
    organization_team_organization_id: Option<i64>,
    entitlement_team_management_enabled: Option<bool>,
    entitlement_seat_limit: Option<i64>,
    entitlement_team_limit: Option<i64>,
    entitlement_organization_key_limit: Option<i64>,
    entitlement_audit_retention_days: Option<i64>,
    entitlement_sso_enabled: Option<bool>,
    entitlement_custom_roles_enabled: Option<bool>,
    entitlement_valid_from: Option<TimeDateTimeWithTimeZone>,
    entitlement_valid_until: Option<TimeDateTimeWithTimeZone>,
    entitlement_grace_until: Option<TimeDateTimeWithTimeZone>,
}

impl OrganizationTokenAuthRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            organization_status: result.try_get("", "organization_status")?,
            membership_id: result.try_get("", "membership_id")?,
            membership_status: result.try_get("", "membership_status")?,
            membership_organization_id: result.try_get("", "membership_organization_id")?,
            membership_user_id: result.try_get("", "membership_user_id")?,
            membership_team_id: result.try_get("", "membership_team_id")?,
            organization_team_status: result.try_get("", "organization_team_status")?,
            organization_team_organization_id: result
                .try_get("", "organization_team_organization_id")?,
            entitlement_team_management_enabled: result
                .try_get("", "entitlement_team_management_enabled")?,
            entitlement_seat_limit: result.try_get("", "entitlement_seat_limit")?,
            entitlement_team_limit: result.try_get("", "entitlement_team_limit")?,
            entitlement_organization_key_limit: result
                .try_get("", "entitlement_organization_key_limit")?,
            entitlement_audit_retention_days: result
                .try_get("", "entitlement_audit_retention_days")?,
            entitlement_sso_enabled: result.try_get("", "entitlement_sso_enabled")?,
            entitlement_custom_roles_enabled: result
                .try_get("", "entitlement_custom_roles_enabled")?,
            entitlement_valid_from: result.try_get("", "entitlement_valid_from")?,
            entitlement_valid_until: result.try_get("", "entitlement_valid_until")?,
            entitlement_grace_until: result.try_get("", "entitlement_grace_until")?,
        })
    }

    /// 企业 Key 每次使用都重新核对成员、授权、团队和当前容量。
    async fn validate(
        &self,
        connection: &impl ConnectionTrait,
        context: OrganizationTokenAuthContext,
    ) -> Result<OrganizationTokenAuthValidation, TokenAuthRepositoryError> {
        let organization_value = context.organization_id.get();
        let organization_id = context.organization_id;
        let membership_value = context.membership_id.get();
        let user_id = context.user_id;
        if self.membership_id != Some(membership_value)
            || self.membership_organization_id != Some(organization_value)
            || self.membership_user_id != Some(user_id.get())
            || self.membership_team_id != context.team_id.map(|id| id.get())
        {
            return Ok(OrganizationTokenAuthValidation::Rejected);
        }
        let membership_status = OrganizationMembershipStatus::try_from(
            self.membership_status
                .ok_or(TokenAuthRepositoryError::Invariant)?,
        )
        .map_err(|_| TokenAuthRepositoryError::Invariant)?;
        if !membership_status.is_active() {
            return Ok(OrganizationTokenAuthValidation::Rejected);
        }
        let organization_status = OrganizationStatus::try_from(
            self.organization_status
                .ok_or(TokenAuthRepositoryError::Invariant)?,
        )
        .map_err(|_| TokenAuthRepositoryError::Invariant)?;
        let capacity = OrganizationEntitlementCapacity::new(
            self.entitlement_seat_limit
                .ok_or(TokenAuthRepositoryError::Invariant)?,
            self.entitlement_team_limit
                .ok_or(TokenAuthRepositoryError::Invariant)?,
            self.entitlement_organization_key_limit
                .ok_or(TokenAuthRepositoryError::Invariant)?,
        )
        .map_err(|_| TokenAuthRepositoryError::Invariant)?;
        let entitlement = OrganizationEntitlementSnapshot::new(
            capacity,
            self.entitlement_team_management_enabled
                .ok_or(TokenAuthRepositoryError::Invariant)?,
            self.entitlement_sso_enabled
                .ok_or(TokenAuthRepositoryError::Invariant)?,
            self.entitlement_custom_roles_enabled
                .ok_or(TokenAuthRepositoryError::Invariant)?,
            self.entitlement_audit_retention_days
                .ok_or(TokenAuthRepositoryError::Invariant)?,
            self.entitlement_valid_from
                .ok_or(TokenAuthRepositoryError::Invariant)?
                .unix_timestamp(),
            self.entitlement_valid_until
                .ok_or(TokenAuthRepositoryError::Invariant)?
                .unix_timestamp(),
            self.entitlement_grace_until
                .map(|value| value.unix_timestamp()),
        )
        .map_err(|_| TokenAuthRepositoryError::Invariant)?;
        if !entitlement
            .status(
                organization_status,
                TimeDateTimeWithTimeZone::now_utc().unix_timestamp(),
            )
            .allows_existing_consumption()
        {
            return Ok(OrganizationTokenAuthValidation::Rejected);
        }
        let team_id = match context.team_id.map(|id| id.get()) {
            Some(value) => {
                let status = OrganizationTeamStatus::try_from(
                    self.organization_team_status
                        .ok_or(TokenAuthRepositoryError::Invariant)?,
                )
                .map_err(|_| TokenAuthRepositoryError::Invariant)?;
                if !status.is_active()
                    || self.organization_team_organization_id != Some(organization_value)
                    || !entitlement.team_management_enabled()
                {
                    return Ok(OrganizationTokenAuthValidation::Rejected);
                }
                Some(
                    OrganizationTeamId::new(value)
                        .map_err(|_| TokenAuthRepositoryError::Invariant)?,
                )
            }
            None => None,
        };
        let department_id = match context.department_id.map(|id| id.get()) {
            Some(value) => {
                let department_id = OrganizationDepartmentId::new(value)
                    .map_err(|_| TokenAuthRepositoryError::Invariant)?;
                let department = organization_departments::Entity::find_by_id(value)
                    .filter(organization_departments::Column::OrganizationId.eq(organization_value))
                    .one(connection)
                    .await
                    .map_err(|_| TokenAuthRepositoryError::Query)?;
                let Some(department) = department else {
                    return Ok(OrganizationTokenAuthValidation::Rejected);
                };
                if OrganizationDepartmentStatus::try_from(department.status)
                    .map_err(|_| TokenAuthRepositoryError::Invariant)?
                    != OrganizationDepartmentStatus::Active
                {
                    return Ok(OrganizationTokenAuthValidation::Rejected);
                }
                let binding = organization_department_memberships::Entity::find()
                    .filter(
                        organization_department_memberships::Column::OrganizationId
                            .eq(organization_value),
                    )
                    .filter(
                        organization_department_memberships::Column::MembershipId
                            .eq(membership_value),
                    )
                    .filter(organization_department_memberships::Column::DepartmentId.eq(value))
                    .filter(
                        organization_department_memberships::Column::Status
                            .eq(OrganizationDepartmentMembershipStatus::Active.code()),
                    )
                    .one(connection)
                    .await
                    .map_err(|_| TokenAuthRepositoryError::Query)?;
                if binding.is_none() {
                    return Ok(OrganizationTokenAuthValidation::Rejected);
                }
                Some(department_id)
            }
            None => None,
        };
        let key_count = tokens::Entity::find()
            .filter(tokens::Column::OrganizationId.eq(organization_value))
            .filter(tokens::Column::DeletedAt.is_null())
            .count(connection)
            .await
            .map_err(|_| TokenAuthRepositoryError::Query)?;
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
}
