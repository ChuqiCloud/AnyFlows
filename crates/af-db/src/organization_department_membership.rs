use std::{fmt, time::Duration};

use af_domain::{
    OrganizationDepartmentId, OrganizationDepartmentMembership,
    OrganizationDepartmentMembershipError, OrganizationDepartmentMembershipStatus,
    OrganizationDepartmentStatus, OrganizationId, OrganizationMembershipId,
    OrganizationMembershipStatus,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    DatabasePool,
    entity::{
        organization_department_memberships, organization_departments, organization_memberships,
        organizations,
    },
};

const ACTIVE_ORGANIZATION: i16 = 1;
const ACTIVE_MEMBERSHIP: i16 = OrganizationMembershipStatus::Active.code();
const ACTIVE_DEPARTMENT: i16 = OrganizationDepartmentStatus::Active.code();
const ACTIVE_BINDING: i16 = OrganizationDepartmentMembershipStatus::Active.code();
const REMOVED_BINDING: i16 = OrganizationDepartmentMembershipStatus::Removed.code();

/// 创建部门成员归属的输入；主部门写入必须同时提供成员版本。
pub struct OrganizationDepartmentMembershipCreate {
    department_id: OrganizationDepartmentId,
    is_primary: bool,
    expected_membership_version: Option<i64>,
}

impl OrganizationDepartmentMembershipCreate {
    /// 组装归属输入；非主部门可省略成员 CAS 版本，主部门必须提供。
    #[must_use]
    pub const fn new(
        department_id: OrganizationDepartmentId,
        is_primary: bool,
        expected_membership_version: Option<i64>,
    ) -> Self {
        Self {
            department_id,
            is_primary,
            expected_membership_version,
        }
    }
}

impl fmt::Debug for OrganizationDepartmentMembershipCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationDepartmentMembershipCreate")
            .field("is_primary", &self.is_primary)
            .field(
                "has_expected_membership_version",
                &self.expected_membership_version.is_some(),
            )
            .finish_non_exhaustive()
    }
}

/// 已通过企业、成员和部门边界校验的归属记录。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationDepartmentMembershipRecord {
    id: i64,
    fact: OrganizationDepartmentMembership,
    created_at: i64,
    updated_at: i64,
    removed_at: Option<i64>,
}

impl OrganizationDepartmentMembershipRecord {
    #[must_use]
    pub const fn id(self) -> i64 {
        self.id
    }

    /// 返回不含时间字段的领域归属快照。
    #[must_use]
    pub const fn fact(self) -> OrganizationDepartmentMembership {
        self.fact
    }

    #[must_use]
    pub const fn organization_id(self) -> OrganizationId {
        self.fact.organization_id()
    }

    #[must_use]
    pub const fn membership_id(self) -> OrganizationMembershipId {
        self.fact.membership_id()
    }

    #[must_use]
    pub const fn department_id(self) -> OrganizationDepartmentId {
        self.fact.department_id()
    }

    #[must_use]
    pub const fn is_primary(self) -> bool {
        self.fact.is_primary()
    }

    #[must_use]
    pub const fn status(self) -> OrganizationDepartmentMembershipStatus {
        self.fact.status()
    }

    #[must_use]
    pub const fn version(self) -> i64 {
        self.fact.version()
    }

    #[must_use]
    pub const fn created_at(self) -> i64 {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(self) -> i64 {
        self.updated_at
    }

    #[must_use]
    pub const fn removed_at(self) -> Option<i64> {
        self.removed_at
    }
}

/// 部门成员归属仓储配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationDepartmentMembershipRepositoryConfigError {
    #[error("企业部门成员归属仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 部门成员归属仓储错误；不携带用户、部门名称或其他原始输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationDepartmentMembershipRepositoryError {
    #[error("企业部门成员归属输入无效")]
    InvalidInput,
    #[error("企业不可用")]
    OrganizationUnavailable,
    #[error("企业成员不存在")]
    MembershipNotFound,
    #[error("企业成员不属于目标企业")]
    MembershipOrganizationMismatch,
    #[error("企业成员当前不可用")]
    MembershipUnavailable,
    #[error("企业部门不存在")]
    DepartmentNotFound,
    #[error("企业部门不属于目标企业")]
    DepartmentOrganizationMismatch,
    #[error("企业部门当前不可用")]
    DepartmentUnavailable,
    #[error("企业成员部门归属不存在")]
    NotFound,
    #[error("企业成员部门归属已经存在")]
    AlreadyAssigned,
    #[error("企业成员部门归属已经解除")]
    AlreadyRemoved,
    #[error("企业主部门不能直接解除")]
    PrimaryRemoval,
    #[error("企业成员主部门状态冲突")]
    Conflict,
    #[error("企业部门成员归属持久化状态损坏")]
    Invariant,
    #[error("企业部门成员归属数据库操作失败")]
    Query,
    #[error("企业部门成员归属操作超时")]
    Timeout,
}

/// 企业成员与部门多归属仓储；所有写入固定为企业→成员→部门锁序。
#[derive(Clone)]
pub struct OrganizationDepartmentMembershipRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationDepartmentMembershipRepository {
    /// 使用共享连接池和非零硬截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationDepartmentMembershipRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(
                OrganizationDepartmentMembershipRepositoryConfigError::ZeroOperationTimeout,
            );
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 添加一条活动归属；主部门写入使用成员版本 CAS 并自动降级旧主部门。
    pub async fn add(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        write: OrganizationDepartmentMembershipCreate,
    ) -> Result<
        OrganizationDepartmentMembershipRecord,
        OrganizationDepartmentMembershipRepositoryError,
    > {
        validate_expected_version(write.expected_membership_version, write.is_primary)?;
        self.run(self.add_inner(organization_id, membership_id, write))
            .await
    }

    /// 把已存在的活动归属提升为主部门，并使用成员版本 CAS。
    pub async fn set_primary(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        department_id: OrganizationDepartmentId,
        expected_membership_version: i64,
    ) -> Result<
        OrganizationDepartmentMembershipRecord,
        OrganizationDepartmentMembershipRepositoryError,
    > {
        validate_expected_version(Some(expected_membership_version), true)?;
        self.run(self.set_primary_inner(
            organization_id,
            membership_id,
            department_id,
            expected_membership_version,
        ))
        .await
    }

    /// 解除非主部门归属并保留历史记录；归属版本使用 CAS。
    pub async fn remove(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        department_id: OrganizationDepartmentId,
        expected_version: i64,
    ) -> Result<
        OrganizationDepartmentMembershipRecord,
        OrganizationDepartmentMembershipRepositoryError,
    > {
        validate_expected_version(Some(expected_version), false)?;
        self.run(self.remove_inner(
            organization_id,
            membership_id,
            department_id,
            expected_version,
        ))
        .await
    }

    /// 按企业和成员范围返回全部活动/历史归属，调用方可按状态裁剪。
    pub async fn list(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
    ) -> Result<
        Vec<OrganizationDepartmentMembershipRecord>,
        OrganizationDepartmentMembershipRepositoryError,
    > {
        self.run(self.list_inner(organization_id, membership_id))
            .await
    }

    async fn add_inner(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        write: OrganizationDepartmentMembershipCreate,
    ) -> Result<
        OrganizationDepartmentMembershipRecord,
        OrganizationDepartmentMembershipRepositoryError,
    > {
        let transaction = self.begin().await?;
        lock_active_organization(&transaction, organization_id).await?;
        let membership = lock_membership(&transaction, organization_id, membership_id).await?;
        validate_membership(&membership)?;
        let department =
            lock_department(&transaction, organization_id, write.department_id).await?;
        validate_department(&department)?;
        if find_association(
            &transaction,
            organization_id,
            membership_id,
            write.department_id,
            true,
        )
        .await?
        .is_some()
        {
            return Err(OrganizationDepartmentMembershipRepositoryError::AlreadyAssigned);
        }

        let mut associations = if write.is_primary {
            lock_associations(&transaction, organization_id, membership_id).await?
        } else {
            Vec::new()
        };
        if write.is_primary {
            transition_primary(
                &transaction,
                organization_id,
                membership_id,
                &membership,
                &mut associations,
                write.expected_membership_version,
            )
            .await?;
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_department_memberships::ActiveModel {
            organization_id: Set(organization_id.get()),
            membership_id: Set(membership_id.get()),
            department_id: Set(write.department_id.get()),
            is_primary: Set(if write.is_primary { 1_i16 } else { 0_i16 }),
            status: Set(ACTIVE_BINDING),
            version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            removed_at: Set(None),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(map_write_error)?;
        let record = record_from_model(model)?;
        transaction.commit().await.map_err(query_error)?;
        Ok(record)
    }

    async fn set_primary_inner(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        department_id: OrganizationDepartmentId,
        expected_membership_version: i64,
    ) -> Result<
        OrganizationDepartmentMembershipRecord,
        OrganizationDepartmentMembershipRepositoryError,
    > {
        let transaction = self.begin().await?;
        lock_active_organization(&transaction, organization_id).await?;
        let membership = lock_membership(&transaction, organization_id, membership_id).await?;
        validate_membership(&membership)?;
        let department = lock_department(&transaction, organization_id, department_id).await?;
        validate_department(&department)?;
        let mut associations =
            lock_associations(&transaction, organization_id, membership_id).await?;
        let target = associations
            .iter()
            .find(|model| {
                model.department_id == department_id.get() && model.status == ACTIVE_BINDING
            })
            .cloned()
            .ok_or(OrganizationDepartmentMembershipRepositoryError::NotFound)?;
        if associations
            .iter()
            .filter(|model| model.status == ACTIVE_BINDING && model.is_primary != 0)
            .count()
            > 1
        {
            return Err(OrganizationDepartmentMembershipRepositoryError::Invariant);
        }
        if target.is_primary != 0 {
            let record = record_from_model(target)?;
            transaction.commit().await.map_err(query_error)?;
            return Ok(record);
        }
        transition_primary(
            &transaction,
            organization_id,
            membership_id,
            &membership,
            &mut associations,
            Some(expected_membership_version),
        )
        .await?;
        let updated = organization_department_memberships::Entity::update_many()
            .filter(organization_department_memberships::Column::Id.eq(target.id))
            .filter(
                organization_department_memberships::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .filter(organization_department_memberships::Column::Status.eq(ACTIVE_BINDING))
            .filter(organization_department_memberships::Column::Version.eq(target.version))
            .col_expr(
                organization_department_memberships::Column::IsPrimary,
                Expr::value(1_i16),
            )
            .col_expr(
                organization_department_memberships::Column::Version,
                Expr::value(next_version(target.version)?),
            )
            .col_expr(
                organization_department_memberships::Column::UpdatedAt,
                Expr::value(TimeDateTimeWithTimeZone::now_utc()),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if updated.rows_affected != 1 {
            return Err(OrganizationDepartmentMembershipRepositoryError::Conflict);
        }
        let record = record_from_model(
            organization_department_memberships::Entity::find_by_id(target.id)
                .filter(
                    organization_department_memberships::Column::OrganizationId
                        .eq(organization_id.get()),
                )
                .one(&transaction)
                .await
                .map_err(query_error)?
                .ok_or(OrganizationDepartmentMembershipRepositoryError::Invariant)?,
        )?;
        transaction.commit().await.map_err(query_error)?;
        Ok(record)
    }

    async fn remove_inner(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        department_id: OrganizationDepartmentId,
        expected_version: i64,
    ) -> Result<
        OrganizationDepartmentMembershipRecord,
        OrganizationDepartmentMembershipRepositoryError,
    > {
        let transaction = self.begin().await?;
        lock_active_organization(&transaction, organization_id).await?;
        let membership = lock_membership(&transaction, organization_id, membership_id).await?;
        validate_membership_persisted(&membership)?;
        let _department = lock_department(&transaction, organization_id, department_id).await?;
        let association = find_association(
            &transaction,
            organization_id,
            membership_id,
            department_id,
            true,
        )
        .await?
        .ok_or(OrganizationDepartmentMembershipRepositoryError::NotFound)?;
        if association.version != expected_version {
            return Err(OrganizationDepartmentMembershipRepositoryError::Conflict);
        }
        if association.status == REMOVED_BINDING {
            return Err(OrganizationDepartmentMembershipRepositoryError::AlreadyRemoved);
        }
        if association.is_primary != 0 {
            return Err(OrganizationDepartmentMembershipRepositoryError::PrimaryRemoval);
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let updated = organization_department_memberships::Entity::update_many()
            .filter(organization_department_memberships::Column::Id.eq(association.id))
            .filter(
                organization_department_memberships::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .filter(organization_department_memberships::Column::Status.eq(ACTIVE_BINDING))
            .filter(organization_department_memberships::Column::Version.eq(expected_version))
            .col_expr(
                organization_department_memberships::Column::Status,
                Expr::value(REMOVED_BINDING),
            )
            .col_expr(
                organization_department_memberships::Column::IsPrimary,
                Expr::value(0_i16),
            )
            .col_expr(
                organization_department_memberships::Column::Version,
                Expr::value(next_version(expected_version)?),
            )
            .col_expr(
                organization_department_memberships::Column::UpdatedAt,
                Expr::value(now),
            )
            .col_expr(
                organization_department_memberships::Column::RemovedAt,
                Expr::value(Some(now)),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if updated.rows_affected != 1 {
            return Err(OrganizationDepartmentMembershipRepositoryError::Conflict);
        }
        let record = record_from_model(
            organization_department_memberships::Entity::find_by_id(association.id)
                .filter(
                    organization_department_memberships::Column::OrganizationId
                        .eq(organization_id.get()),
                )
                .one(&transaction)
                .await
                .map_err(query_error)?
                .ok_or(OrganizationDepartmentMembershipRepositoryError::Invariant)?,
        )?;
        transaction.commit().await.map_err(query_error)?;
        Ok(record)
    }

    async fn list_inner(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
    ) -> Result<
        Vec<OrganizationDepartmentMembershipRecord>,
        OrganizationDepartmentMembershipRepositoryError,
    > {
        let transaction = self.begin().await?;
        lock_active_organization(&transaction, organization_id).await?;
        let membership = lock_membership(&transaction, organization_id, membership_id).await?;
        validate_membership_persisted(&membership)?;
        let models = lock_associations(&transaction, organization_id, membership_id).await?;
        let records = models
            .into_iter()
            .map(record_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        transaction.commit().await.map_err(query_error)?;
        Ok(records)
    }

    async fn begin(
        &self,
    ) -> Result<DatabaseTransaction, OrganizationDepartmentMembershipRepositoryError> {
        self.pool.connection().begin().await.map_err(query_error)
    }

    async fn run<T>(
        &self,
        operation: impl std::future::Future<
            Output = Result<T, OrganizationDepartmentMembershipRepositoryError>,
        >,
    ) -> Result<T, OrganizationDepartmentMembershipRepositoryError> {
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationDepartmentMembershipRepositoryError::Timeout)?
    }
}

impl fmt::Debug for OrganizationDepartmentMembershipRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationDepartmentMembershipRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn transition_primary(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    membership: &organization_memberships::Model,
    associations: &mut [organization_department_memberships::Model],
    expected_membership_version: Option<i64>,
) -> Result<(), OrganizationDepartmentMembershipRepositoryError> {
    let expected = expected_membership_version
        .ok_or(OrganizationDepartmentMembershipRepositoryError::InvalidInput)?;
    if membership.version != expected {
        return Err(OrganizationDepartmentMembershipRepositoryError::Conflict);
    }
    let active_primaries = associations
        .iter()
        .filter(|model| model.status == ACTIVE_BINDING && model.is_primary != 0)
        .count();
    if active_primaries > 1 {
        return Err(OrganizationDepartmentMembershipRepositoryError::Invariant);
    }
    let now = TimeDateTimeWithTimeZone::now_utc();
    for model in associations
        .iter()
        .filter(|model| model.status == ACTIVE_BINDING && model.is_primary != 0)
    {
        let updated = organization_department_memberships::Entity::update_many()
            .filter(organization_department_memberships::Column::Id.eq(model.id))
            .filter(
                organization_department_memberships::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .filter(
                organization_department_memberships::Column::MembershipId.eq(membership_id.get()),
            )
            .filter(organization_department_memberships::Column::Status.eq(ACTIVE_BINDING))
            .filter(organization_department_memberships::Column::Version.eq(model.version))
            .col_expr(
                organization_department_memberships::Column::IsPrimary,
                Expr::value(0_i16),
            )
            .col_expr(
                organization_department_memberships::Column::Version,
                Expr::value(next_version(model.version)?),
            )
            .col_expr(
                organization_department_memberships::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(transaction)
            .await
            .map_err(query_error)?;
        if updated.rows_affected != 1 {
            return Err(OrganizationDepartmentMembershipRepositoryError::Conflict);
        }
    }
    let updated = organization_memberships::Entity::update_many()
        .filter(organization_memberships::Column::Id.eq(membership_id.get()))
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::Status.eq(ACTIVE_MEMBERSHIP))
        .filter(organization_memberships::Column::Version.eq(expected))
        .col_expr(
            organization_memberships::Column::Version,
            Expr::value(next_version(expected)?),
        )
        .exec(transaction)
        .await
        .map_err(query_error)?;
    if updated.rows_affected != 1 {
        return Err(OrganizationDepartmentMembershipRepositoryError::Conflict);
    }
    Ok(())
}

async fn lock_active_organization(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<(), OrganizationDepartmentMembershipRepositoryError> {
    let mut query = organizations::Entity::find_by_id(organization_id.get());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let organization = query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationDepartmentMembershipRepositoryError::OrganizationUnavailable)?;
    if organization.status != ACTIVE_ORGANIZATION || organization.closed_at.is_some() {
        return Err(OrganizationDepartmentMembershipRepositoryError::OrganizationUnavailable);
    }
    Ok(())
}

async fn lock_membership(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
) -> Result<organization_memberships::Model, OrganizationDepartmentMembershipRepositoryError> {
    let mut query = organization_memberships::Entity::find_by_id(membership_id.get())
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    if let Some(model) = query.one(transaction).await.map_err(query_error)? {
        return Ok(model);
    }
    let exists = organization_memberships::Entity::find_by_id(membership_id.get())
        .one(transaction)
        .await
        .map_err(query_error)?
        .is_some();
    if exists {
        Err(OrganizationDepartmentMembershipRepositoryError::MembershipOrganizationMismatch)
    } else {
        Err(OrganizationDepartmentMembershipRepositoryError::MembershipNotFound)
    }
}

fn validate_membership(
    membership: &organization_memberships::Model,
) -> Result<(), OrganizationDepartmentMembershipRepositoryError> {
    validate_membership_persisted(membership)?;
    if membership.status != ACTIVE_MEMBERSHIP {
        return Err(OrganizationDepartmentMembershipRepositoryError::MembershipUnavailable);
    }
    Ok(())
}

fn validate_membership_persisted(
    membership: &organization_memberships::Model,
) -> Result<(), OrganizationDepartmentMembershipRepositoryError> {
    if !matches!(membership.status, 1..=3)
        || membership.version <= 0
        || membership.version == i64::MAX
    {
        return Err(OrganizationDepartmentMembershipRepositoryError::MembershipUnavailable);
    }
    Ok(())
}

async fn lock_department(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    department_id: OrganizationDepartmentId,
) -> Result<organization_departments::Model, OrganizationDepartmentMembershipRepositoryError> {
    let mut query = organization_departments::Entity::find_by_id(department_id.get())
        .filter(organization_departments::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let model = query.one(transaction).await.map_err(query_error)?;
    let model = if let Some(model) = model {
        model
    } else {
        let exists = organization_departments::Entity::find_by_id(department_id.get())
            .one(transaction)
            .await
            .map_err(query_error)?
            .is_some();
        return if exists {
            Err(OrganizationDepartmentMembershipRepositoryError::DepartmentOrganizationMismatch)
        } else {
            Err(OrganizationDepartmentMembershipRepositoryError::DepartmentNotFound)
        };
    };
    Ok(model)
}

fn validate_department(
    department: &organization_departments::Model,
) -> Result<(), OrganizationDepartmentMembershipRepositoryError> {
    if department.status != ACTIVE_DEPARTMENT
        || department.version <= 0
        || department.version == i64::MAX
    {
        return Err(OrganizationDepartmentMembershipRepositoryError::DepartmentUnavailable);
    }
    Ok(())
}

async fn lock_associations(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
) -> Result<
    Vec<organization_department_memberships::Model>,
    OrganizationDepartmentMembershipRepositoryError,
> {
    let mut query = organization_department_memberships::Entity::find()
        .filter(
            organization_department_memberships::Column::OrganizationId.eq(organization_id.get()),
        )
        .filter(organization_department_memberships::Column::MembershipId.eq(membership_id.get()))
        .order_by_asc(organization_department_memberships::Column::Id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.all(transaction).await.map_err(query_error)
}

async fn find_association(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    department_id: OrganizationDepartmentId,
    lock: bool,
) -> Result<
    Option<organization_department_memberships::Model>,
    OrganizationDepartmentMembershipRepositoryError,
> {
    let mut query = organization_department_memberships::Entity::find()
        .filter(
            organization_department_memberships::Column::OrganizationId.eq(organization_id.get()),
        )
        .filter(organization_department_memberships::Column::MembershipId.eq(membership_id.get()))
        .filter(organization_department_memberships::Column::DepartmentId.eq(department_id.get()));
    if lock && transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.one(transaction).await.map_err(query_error)
}

fn record_from_model(
    model: organization_department_memberships::Model,
) -> Result<OrganizationDepartmentMembershipRecord, OrganizationDepartmentMembershipRepositoryError>
{
    if model.id <= 0 {
        return Err(invariant());
    }
    let organization_id = OrganizationId::new(model.organization_id).map_err(|_| invariant())?;
    let membership_id =
        OrganizationMembershipId::new(model.membership_id).map_err(|_| invariant())?;
    let department_id =
        OrganizationDepartmentId::new(model.department_id).map_err(|_| invariant())?;
    let status =
        OrganizationDepartmentMembershipStatus::try_from(model.status).map_err(|_| invariant())?;
    let is_primary = match model.is_primary {
        0 => false,
        1 => true,
        _ => return Err(invariant()),
    };
    if (status == OrganizationDepartmentMembershipStatus::Removed) != model.removed_at.is_some()
        || (status == OrganizationDepartmentMembershipStatus::Removed && is_primary)
    {
        return Err(invariant());
    }
    let fact = OrganizationDepartmentMembership::new(
        organization_id,
        membership_id,
        department_id,
        is_primary,
        status,
        model.version,
    )
    .map_err(map_domain_error)?;
    Ok(OrganizationDepartmentMembershipRecord {
        id: model.id,
        fact,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
        removed_at: model.removed_at.map(|value| value.unix_timestamp()),
    })
}

fn validate_expected_version(
    expected: Option<i64>,
    required: bool,
) -> Result<(), OrganizationDepartmentMembershipRepositoryError> {
    if required && expected.is_none() {
        return Err(OrganizationDepartmentMembershipRepositoryError::InvalidInput);
    }
    if expected.is_some_and(|version| version <= 0 || version == i64::MAX) {
        return Err(OrganizationDepartmentMembershipRepositoryError::InvalidInput);
    }
    Ok(())
}

fn next_version(version: i64) -> Result<i64, OrganizationDepartmentMembershipRepositoryError> {
    version
        .checked_add(1)
        .filter(|value| *value > 0)
        .ok_or_else(invariant)
}

fn map_domain_error(
    error: OrganizationDepartmentMembershipError,
) -> OrganizationDepartmentMembershipRepositoryError {
    match error {
        OrganizationDepartmentMembershipError::InvalidIdentifier
        | OrganizationDepartmentMembershipError::InvalidStatus
        | OrganizationDepartmentMembershipError::InvalidVersion
        | OrganizationDepartmentMembershipError::RemovedPrimary => invariant(),
    }
}

fn map_write_error(error: DbErr) -> OrganizationDepartmentMembershipRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        OrganizationDepartmentMembershipRepositoryError::AlreadyAssigned
    } else {
        query_error(error)
    }
}

fn query_error(_error: impl fmt::Debug) -> OrganizationDepartmentMembershipRepositoryError {
    OrganizationDepartmentMembershipRepositoryError::Query
}

const fn invariant() -> OrganizationDepartmentMembershipRepositoryError {
    OrganizationDepartmentMembershipRepositoryError::Invariant
}
