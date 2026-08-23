use std::{collections::HashSet, fmt, future::Future, time::Duration};

use af_domain::{
    OrganizationCustomRoleId, OrganizationCustomRoleStatus, OrganizationId,
    OrganizationMembershipId, OrganizationMembershipStatus, OrganizationPermission,
    OrganizationRole,
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{
        organization_custom_role_permissions, organization_custom_roles, organization_memberships,
    },
};

/// 成员与自定义角色绑定完成后的最小安全事实。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationCustomRoleBindingRecord {
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    custom_role_id: Option<OrganizationCustomRoleId>,
    membership_version: i64,
}

impl OrganizationCustomRoleBindingRecord {
    /// 返回绑定所属企业。
    #[must_use]
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }

    /// 返回目标成员关系。
    #[must_use]
    pub const fn membership_id(self) -> OrganizationMembershipId {
        self.membership_id
    }

    /// 返回当前自定义角色；空值表示已清除绑定。
    #[must_use]
    pub const fn custom_role_id(self) -> Option<OrganizationCustomRoleId> {
        self.custom_role_id
    }

    /// 返回写入后的成员 CAS 版本。
    #[must_use]
    pub const fn membership_version(self) -> i64 {
        self.membership_version
    }
}

/// 企业自定义角色绑定仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCustomRoleBindingRepositoryConfigError {
    /// 零超时无法形成有效的数据库截止边界。
    #[error("企业自定义角色绑定仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 企业自定义角色绑定仓储错误；不暴露企业、成员或角色内部标识。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCustomRoleBindingRepositoryError {
    /// CAS 版本不满足持久化边界。
    #[error("企业自定义角色绑定输入无效")]
    InvalidInput,
    /// 成员不属于目标企业。
    #[error("企业成员不存在")]
    NotFound,
    /// 成员不是活动的固定 Member，不能改写绑定。
    #[error("企业成员不可绑定自定义角色")]
    MembershipUnavailable,
    /// 目标角色不属于当前企业、已经停用或不存在。
    #[error("企业自定义角色不可绑定")]
    RoleUnavailable,
    /// 成员 CAS 版本与当前事实冲突。
    #[error("企业自定义角色绑定状态冲突")]
    Conflict,
    /// 获取连接、事务或执行数据库语句失败。
    #[error("企业自定义角色绑定数据库操作失败")]
    Query,
    /// 单次数据库操作超过硬截止时间。
    #[error("企业自定义角色绑定数据库操作超时")]
    Timeout,
    /// 持久化绑定、角色或权限集合违反闭合边界。
    #[error("企业自定义角色绑定持久化状态损坏")]
    Invariant,
}

/// 企业成员与自定义角色绑定事实仓储。
#[derive(Clone)]
pub struct OrganizationCustomRoleBindingRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationCustomRoleBindingRepository {
    /// 使用共享连接池和非零硬截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationCustomRoleBindingRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationCustomRoleBindingRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 按成员版本设置或清除角色；只有活动固定 Member 可被改写。
    pub async fn set_binding(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        custom_role_id: Option<OrganizationCustomRoleId>,
        expected_membership_version: i64,
    ) -> Result<OrganizationCustomRoleBindingRecord, OrganizationCustomRoleBindingRepositoryError>
    {
        self.run(self.set_binding_inner(
            organization_id,
            membership_id,
            custom_role_id,
            expected_membership_version,
        ))
        .await
    }

    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, OrganizationCustomRoleBindingRepositoryError>>,
    ) -> Result<T, OrganizationCustomRoleBindingRepositoryError> {
        match timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                OrganizationCustomRoleBindingRepositoryError::Timeout,
            )),
        }
    }

    async fn set_binding_inner(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        custom_role_id: Option<OrganizationCustomRoleId>,
        expected_membership_version: i64,
    ) -> Result<OrganizationCustomRoleBindingRecord, OrganizationCustomRoleBindingRepositoryError>
    {
        validate_expected_version(expected_membership_version)?;
        let transaction = self.pool.connection().begin().await.map_err(query_error)?;
        let current = lock_membership(&transaction, organization_id, membership_id).await?;
        validate_bindable_member(&current)?;
        if current.version != expected_membership_version {
            rollback(transaction).await?;
            return Err(OrganizationCustomRoleBindingRepositoryError::Conflict);
        }
        if let Some(custom_role_id) = custom_role_id {
            lock_active_role(&transaction, organization_id, custom_role_id).await?;
        }
        if current.custom_role_id == custom_role_id.map(OrganizationCustomRoleId::get) {
            let record = binding_record(current)?;
            commit(transaction).await?;
            return Ok(record);
        }

        let next_version = current.version.checked_add(1).ok_or_else(invariant)?;
        let result = organization_memberships::Entity::update_many()
            .filter(organization_memberships::Column::Id.eq(membership_id.get()))
            .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_memberships::Column::Version.eq(current.version))
            .filter(organization_memberships::Column::Role.eq(OrganizationRole::Member.code()))
            .filter(
                organization_memberships::Column::Status
                    .eq(OrganizationMembershipStatus::Active.code()),
            )
            .col_expr(
                organization_memberships::Column::CustomRoleId,
                Expr::value(custom_role_id.map(OrganizationCustomRoleId::get)),
            )
            .col_expr(
                organization_memberships::Column::Version,
                Expr::value(next_version),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationCustomRoleBindingRepositoryError::Conflict);
        }
        let updated = organization_memberships::Entity::find_by_id(membership_id.get())
            .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = binding_record(updated)?;
        commit(transaction).await?;
        Ok(record)
    }
}

impl fmt::Debug for OrganizationCustomRoleBindingRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationCustomRoleBindingRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

/// 解析活动成员的闭合自定义权限；无效绑定只撤销附加权限。
pub(crate) async fn active_custom_role_permissions<C>(
    connection: &C,
    membership: &organization_memberships::Model,
    custom_roles_enabled: bool,
) -> Result<Option<Vec<OrganizationPermission>>, OrganizationCustomRoleBindingRepositoryError>
where
    C: ConnectionTrait,
{
    if !custom_roles_enabled
        || membership.role != OrganizationRole::Member.code()
        || membership.status != OrganizationMembershipStatus::Active.code()
    {
        return Ok(None);
    }
    let Some(custom_role_id) = membership.custom_role_id.filter(|value| *value > 0) else {
        return Ok(None);
    };
    let role = organization_custom_roles::Entity::find_by_id(custom_role_id)
        .filter(organization_custom_roles::Column::OrganizationId.eq(membership.organization_id))
        .filter(
            organization_custom_roles::Column::Status
                .eq(OrganizationCustomRoleStatus::Active.code()),
        )
        .one(connection)
        .await
        .map_err(query_error)?;
    if role.is_none() {
        return Ok(None);
    }
    let codes = organization_custom_role_permissions::Entity::find()
        .filter(organization_custom_role_permissions::Column::CustomRoleId.eq(custom_role_id))
        .order_by_asc(organization_custom_role_permissions::Column::PermissionCode)
        .all(connection)
        .await
        .map_err(query_error)?
        .into_iter()
        .map(|model| model.permission_code)
        .collect();
    Ok(close_permission_codes(codes))
}

fn close_permission_codes(codes: Vec<String>) -> Option<Vec<OrganizationPermission>> {
    if codes.len() > OrganizationPermission::custom_role_grantable_permissions().len() {
        return None;
    }
    let mut permissions = Vec::with_capacity(codes.len());
    let mut seen = HashSet::with_capacity(codes.len());
    for code in codes {
        let permission = OrganizationPermission::from_code(&code).ok()?;
        if !permission.is_custom_role_grantable() || !seen.insert(permission) {
            return None;
        }
        permissions.push(permission);
    }
    permissions.sort_unstable_by_key(|permission| permission.code());
    Some(permissions)
}

async fn lock_membership(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
) -> Result<organization_memberships::Model, OrganizationCustomRoleBindingRepositoryError> {
    let mut query = organization_memberships::Entity::find_by_id(membership_id.get())
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationCustomRoleBindingRepositoryError::NotFound)
}

async fn lock_active_role(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    custom_role_id: OrganizationCustomRoleId,
) -> Result<(), OrganizationCustomRoleBindingRepositoryError> {
    let mut query = organization_custom_roles::Entity::find_by_id(custom_role_id.get())
        .filter(organization_custom_roles::Column::OrganizationId.eq(organization_id.get()))
        .filter(
            organization_custom_roles::Column::Status
                .eq(OrganizationCustomRoleStatus::Active.code()),
        );
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .map(|_| ())
        .ok_or(OrganizationCustomRoleBindingRepositoryError::RoleUnavailable)
}

fn validate_bindable_member(
    membership: &organization_memberships::Model,
) -> Result<(), OrganizationCustomRoleBindingRepositoryError> {
    let role = OrganizationRole::try_from(membership.role).map_err(|_| invariant())?;
    let status =
        OrganizationMembershipStatus::try_from(membership.status).map_err(|_| invariant())?;
    if role != OrganizationRole::Member || !status.is_active() {
        return Err(OrganizationCustomRoleBindingRepositoryError::MembershipUnavailable);
    }
    Ok(())
}

fn validate_expected_version(
    expected_version: i64,
) -> Result<(), OrganizationCustomRoleBindingRepositoryError> {
    if expected_version <= 0 || expected_version == i64::MAX {
        return Err(OrganizationCustomRoleBindingRepositoryError::InvalidInput);
    }
    Ok(())
}

fn binding_record(
    membership: organization_memberships::Model,
) -> Result<OrganizationCustomRoleBindingRecord, OrganizationCustomRoleBindingRepositoryError> {
    validate_bindable_member(&membership)?;
    if membership.version <= 0 {
        return Err(invariant());
    }
    Ok(OrganizationCustomRoleBindingRecord {
        organization_id: OrganizationId::new(membership.organization_id)
            .map_err(|_| invariant())?,
        membership_id: OrganizationMembershipId::new(membership.id).map_err(|_| invariant())?,
        custom_role_id: membership
            .custom_role_id
            .map(OrganizationCustomRoleId::new)
            .transpose()
            .map_err(|_| invariant())?,
        membership_version: membership.version,
    })
}

async fn commit(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationCustomRoleBindingRepositoryError> {
    transaction.commit().await.map_err(query_error)
}

async fn rollback(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationCustomRoleBindingRepositoryError> {
    transaction.rollback().await.map_err(query_error)
}

fn query_error(_error: DbErr) -> OrganizationCustomRoleBindingRepositoryError {
    record_internal_error(OrganizationCustomRoleBindingRepositoryError::Query)
}

fn invariant() -> OrganizationCustomRoleBindingRepositoryError {
    record_internal_error(OrganizationCustomRoleBindingRepositoryError::Invariant)
}

fn record_internal_error(
    error: OrganizationCustomRoleBindingRepositoryError,
) -> OrganizationCustomRoleBindingRepositoryError {
    let error_kind = match error {
        OrganizationCustomRoleBindingRepositoryError::InvalidInput
        | OrganizationCustomRoleBindingRepositoryError::NotFound
        | OrganizationCustomRoleBindingRepositoryError::MembershipUnavailable
        | OrganizationCustomRoleBindingRepositoryError::RoleUnavailable
        | OrganizationCustomRoleBindingRepositoryError::Conflict => return error,
        OrganizationCustomRoleBindingRepositoryError::Query => {
            "organization_custom_role_binding_query"
        }
        OrganizationCustomRoleBindingRepositoryError::Timeout => {
            "organization_custom_role_binding_timeout"
        }
        OrganizationCustomRoleBindingRepositoryError::Invariant => {
            "organization_custom_role_binding_invariant"
        }
    };
    tracing::error!(
        target: "af_db::organization_custom_role_binding",
        error_kind,
        "企业自定义角色绑定仓储发生内部错误"
    );
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_codes_fail_closed_for_unknown_duplicate_and_reserved_values() {
        assert_eq!(
            close_permission_codes(vec!["unknown.permission".to_owned()]),
            None
        );
        assert_eq!(
            close_permission_codes(vec![
                OrganizationPermission::OverviewRead.code().to_owned(),
                OrganizationPermission::OverviewRead.code().to_owned(),
            ]),
            None
        );
        assert_eq!(
            close_permission_codes(vec![OrganizationPermission::RolesAssign.code().to_owned()]),
            None
        );
    }
}
