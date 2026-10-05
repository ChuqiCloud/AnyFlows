use std::fmt;

use af_domain::{OrganizationDepartmentId, OrganizationDepartmentMembershipStatus};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryResult,
    sea_query::{Alias, Expr, Query, SelectStatement},
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, DatabaseTimestamp, OrganizationTokenAuthContext, TokenAuthRepositoryError,
    entity::{
        organization_department_memberships, organization_departments, organization_entitlements,
        organization_memberships, organization_teams, organizations, tokens,
    },
};

/// 企业令牌鉴权的数据库读取边界，不接收 API Key 或摘要。
///
/// 方法不创建独立超时；公共鉴权仓储对整个校验流程应用同一个查询截止时间。
#[derive(Clone, Debug)]
pub struct OrganizationTokenAuthRepository {
    pool: DatabasePool,
}

impl OrganizationTokenAuthRepository {
    #[must_use]
    pub const fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// 复验令牌归属并读取企业状态；不判断企业是否允许调用。
    pub async fn lookup(
        &self,
        context: OrganizationTokenAuthContext,
    ) -> Result<Option<OrganizationTokenAuthRecord>, TokenAuthRepositoryError> {
        self.lookup_inner(context)
            .with_subscriber(NoSubscriber::default())
            .await
    }

    async fn lookup_inner(
        &self,
        context: OrganizationTokenAuthContext,
    ) -> Result<Option<OrganizationTokenAuthRecord>, TokenAuthRepositoryError> {
        let connection = self.pool.connection();
        let statement = connection
            .get_database_backend()
            .build(&lookup_query(context));
        let mut results = connection
            .query_all(statement)
            .await
            .map_err(|_| TokenAuthRepositoryError::Query)?;
        let result = match results.len() {
            0 => return Ok(None),
            1 => results.pop().ok_or(TokenAuthRepositoryError::Invariant)?,
            _ => return Err(TokenAuthRepositoryError::Invariant),
        };
        OrganizationTokenAuthRecord::try_from_query_result(&result)
            .map(Some)
            .map_err(|_| TokenAuthRepositoryError::Invariant)
    }

    /// 读取指定组织的部门状态。
    pub async fn department(
        &self,
        context: OrganizationTokenAuthContext,
        department_id: OrganizationDepartmentId,
    ) -> Result<Option<OrganizationTokenAuthDepartmentRecord>, TokenAuthRepositoryError> {
        self.department_inner(context, department_id)
            .with_subscriber(NoSubscriber::default())
            .await
    }

    async fn department_inner(
        &self,
        context: OrganizationTokenAuthContext,
        department_id: OrganizationDepartmentId,
    ) -> Result<Option<OrganizationTokenAuthDepartmentRecord>, TokenAuthRepositoryError> {
        let connection = self.pool.connection();
        let department = organization_departments::Entity::find_by_id(department_id.get())
            .filter(
                organization_departments::Column::OrganizationId.eq(context.organization_id.get()),
            )
            .one(connection)
            .await
            .map_err(|_| TokenAuthRepositoryError::Query)?;
        let Some(department) = department else {
            return Ok(None);
        };
        Ok(Some(OrganizationTokenAuthDepartmentRecord {
            status: department.status,
        }))
    }

    /// 部门状态通过运行时校验后，读取成员的有效部门绑定。
    pub async fn has_active_department_membership(
        &self,
        context: OrganizationTokenAuthContext,
        department_id: OrganizationDepartmentId,
    ) -> Result<bool, TokenAuthRepositoryError> {
        let binding = organization_department_memberships::Entity::find()
            .filter(
                organization_department_memberships::Column::OrganizationId
                    .eq(context.organization_id.get()),
            )
            .filter(
                organization_department_memberships::Column::MembershipId
                    .eq(context.membership_id.get()),
            )
            .filter(
                organization_department_memberships::Column::DepartmentId.eq(department_id.get()),
            )
            .filter(
                organization_department_memberships::Column::Status
                    .eq(OrganizationDepartmentMembershipStatus::Active.code()),
            )
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| TokenAuthRepositoryError::Query)?;
        Ok(binding.is_some())
    }

    /// 统计未软删除的企业令牌，用于运行时容量校验。
    pub async fn key_count(
        &self,
        context: OrganizationTokenAuthContext,
    ) -> Result<u64, TokenAuthRepositoryError> {
        tokens::Entity::find()
            .filter(tokens::Column::OrganizationId.eq(context.organization_id.get()))
            .filter(tokens::Column::DeletedAt.is_null())
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| TokenAuthRepositoryError::Query)
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

/// 企业鉴权读取事实；状态转换和调用许可由企业运行时判断。
pub struct OrganizationTokenAuthRecord {
    pub organization_status: Option<i16>,
    pub membership_id: Option<i64>,
    pub membership_status: Option<i16>,
    pub membership_organization_id: Option<i64>,
    pub membership_user_id: Option<i64>,
    pub membership_team_id: Option<i64>,
    pub organization_team_status: Option<i16>,
    pub organization_team_organization_id: Option<i64>,
    pub entitlement_team_management_enabled: Option<bool>,
    pub entitlement_seat_limit: Option<i64>,
    pub entitlement_team_limit: Option<i64>,
    pub entitlement_organization_key_limit: Option<i64>,
    pub entitlement_audit_retention_days: Option<i64>,
    pub entitlement_sso_enabled: Option<bool>,
    pub entitlement_custom_roles_enabled: Option<bool>,
    pub entitlement_valid_from: Option<DatabaseTimestamp>,
    pub entitlement_valid_until: Option<DatabaseTimestamp>,
    pub entitlement_grace_until: Option<DatabaseTimestamp>,
}

impl fmt::Debug for OrganizationTokenAuthRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationTokenAuthRecord(<redacted>)")
    }
}

/// 部门状态事实，不包含部门名称或成员资料。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationTokenAuthDepartmentRecord {
    pub status: i16,
}

impl OrganizationTokenAuthRecord {
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
}
