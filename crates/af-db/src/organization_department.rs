use std::{collections::HashMap, fmt, time::Duration};

use af_domain::{
    MAX_ORGANIZATION_DEPARTMENT_DEPTH, OrganizationDepartmentDisableFacts,
    OrganizationDepartmentDisableRejection, OrganizationDepartmentId, OrganizationDepartmentKey,
    OrganizationDepartmentNode, OrganizationDepartmentStatus, OrganizationDepartmentTreeError,
    OrganizationId, validate_organization_department_move, validate_organization_department_tree,
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
    entity::{organization_departments, organizations},
};

const ACTIVE_ORGANIZATION: i16 = 1;
const ACTIVE_DEPARTMENT: i16 = OrganizationDepartmentStatus::Active.code();
const DISABLED_DEPARTMENT: i16 = OrganizationDepartmentStatus::Disabled.code();

/// 企业部门名称最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_DEPARTMENT_NAME_BYTES: usize = 128;
/// 企业部门目录单页最大记录数。
pub const MAX_ORGANIZATION_DEPARTMENT_PAGE_SIZE: usize = 100;

/// 创建部门所需的名称、父部门和稳定排序键。
pub struct OrganizationDepartmentCreate {
    pub(super) name: String,
    pub(super) parent_id: Option<OrganizationDepartmentId>,
    pub(super) sort_order: i64,
}

impl OrganizationDepartmentCreate {
    /// 组装企业部门创建输入；公开标识、深度和版本由仓储生成。
    #[must_use]
    pub fn new(name: String, parent_id: Option<OrganizationDepartmentId>, sort_order: i64) -> Self {
        Self {
            name,
            parent_id,
            sort_order,
        }
    }
}

/// 按排序键和内部 ID 组成的稳定部门游标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationDepartmentCursor {
    sort_order: i64,
    id: OrganizationDepartmentId,
}

impl OrganizationDepartmentCursor {
    /// 组装目录查询游标；通常使用上一页返回的 `next_cursor`。
    #[must_use]
    pub const fn new(sort_order: i64, id: OrganizationDepartmentId) -> Self {
        Self { sort_order, id }
    }

    #[must_use]
    pub const fn sort_order(self) -> i64 {
        self.sort_order
    }

    #[must_use]
    pub const fn id(self) -> OrganizationDepartmentId {
        self.id
    }
}

/// 已通过持久化边界校验的部门快照。
#[derive(Clone, PartialEq)]
pub struct OrganizationDepartmentRecord {
    id: OrganizationDepartmentId,
    organization_id: OrganizationId,
    department_key: OrganizationDepartmentKey,
    parent_id: Option<OrganizationDepartmentId>,
    name: String,
    sort_order: i64,
    depth: u8,
    status: OrganizationDepartmentStatus,
    version: i64,
    created_at: i64,
    updated_at: i64,
    disabled_at: Option<i64>,
}

impl OrganizationDepartmentRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationDepartmentId {
        self.id
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    #[must_use]
    pub const fn department_key(&self) -> OrganizationDepartmentKey {
        self.department_key
    }

    #[must_use]
    pub const fn parent_id(&self) -> Option<OrganizationDepartmentId> {
        self.parent_id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn sort_order(&self) -> i64 {
        self.sort_order
    }

    #[must_use]
    pub const fn depth(&self) -> u8 {
        self.depth
    }

    #[must_use]
    pub const fn status(&self) -> OrganizationDepartmentStatus {
        self.status
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }

    #[must_use]
    pub const fn disabled_at(&self) -> Option<i64> {
        self.disabled_at
    }
}

impl fmt::Debug for OrganizationDepartmentRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationDepartmentRecord")
            .field("status", &self.status)
            .field("depth", &self.depth)
            .field("version", &self.version)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("disabled_at", &self.disabled_at)
            .finish_non_exhaustive()
    }
}

/// 企业范围稳定游标目录页。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationDepartmentPageRecord {
    records: Vec<OrganizationDepartmentRecord>,
    next_cursor: Option<OrganizationDepartmentCursor>,
}

impl OrganizationDepartmentPageRecord {
    /// 拆出部门记录和下一页游标。
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Vec<OrganizationDepartmentRecord>,
        Option<OrganizationDepartmentCursor>,
    ) {
        (self.records, self.next_cursor)
    }
}

/// 部门持久化仓储配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationDepartmentRepositoryConfigError {
    #[error("企业部门仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 部门持久化仓储错误；不携带名称、公开标识或其他原始输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationDepartmentRepositoryError {
    #[error("企业部门输入无效")]
    InvalidInput,
    #[error("企业部门不存在")]
    NotFound,
    #[error("企业不可用")]
    OrganizationUnavailable,
    #[error("企业部门父级不存在")]
    ParentNotFound,
    #[error("企业部门父级企业不一致")]
    ParentOrganizationMismatch,
    #[error("企业部门父级不可用")]
    ParentDisabled,
    #[error("企业部门不能挂载到自身")]
    SelfParent,
    #[error("企业部门树存在循环")]
    CycleDetected,
    #[error("企业部门层级超出上限")]
    DepthExceeded,
    #[error("企业部门仍有活动子部门")]
    ActiveChildren,
    #[error("企业部门已经禁用")]
    AlreadyDisabled,
    #[error("企业部门仍有活动成员归属")]
    ActiveMembers,
    #[error("企业部门仍有活动预算绑定")]
    ActiveBudgetBindings,
    #[error("企业部门仍有活动服务账号归属")]
    ActiveServiceAccountBindings,
    #[error("企业部门状态冲突")]
    Conflict,
    #[error("企业部门公开标识生成失败")]
    EntropyUnavailable,
    #[error("企业部门数据库操作失败")]
    Query,
    #[error("企业部门操作超时")]
    Timeout,
    #[error("企业部门持久化状态损坏")]
    Invariant,
}

/// 企业部门邻接表与版本 CAS 仓储。
#[derive(Clone)]
pub struct OrganizationDepartmentRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationDepartmentRepository {
    /// 使用共享连接池和非零硬截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationDepartmentRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationDepartmentRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 在企业事务锁内创建活动部门并计算真实树深度。
    pub async fn create(
        &self,
        organization_id: OrganizationId,
        write: OrganizationDepartmentCreate,
    ) -> Result<OrganizationDepartmentRecord, OrganizationDepartmentRepositoryError> {
        self.run(self.create_inner(organization_id, write)).await
    }

    /// 按稳定 `(sort_order, id)` 游标列出企业内全部部门生命周期记录。
    pub async fn list(
        &self,
        organization_id: OrganizationId,
        after: Option<OrganizationDepartmentCursor>,
        limit: usize,
    ) -> Result<OrganizationDepartmentPageRecord, OrganizationDepartmentRepositoryError> {
        self.run(self.list_inner(organization_id, after, limit))
            .await
    }

    /// 在企业范围读取单个部门，并先验证完整树未损坏。
    pub async fn get(
        &self,
        organization_id: OrganizationId,
        department_id: OrganizationDepartmentId,
    ) -> Result<OrganizationDepartmentRecord, OrganizationDepartmentRepositoryError> {
        self.run(self.get_inner(organization_id, department_id))
            .await
    }

    /// 按部门版本 CAS 移动子树，并同步递增受影响后代版本。
    pub async fn move_department(
        &self,
        organization_id: OrganizationId,
        department_id: OrganizationDepartmentId,
        new_parent_id: Option<OrganizationDepartmentId>,
        expected_version: i64,
    ) -> Result<OrganizationDepartmentRecord, OrganizationDepartmentRepositoryError> {
        validate_expected_version(expected_version)?;
        self.run(self.move_inner(
            organization_id,
            department_id,
            new_parent_id,
            expected_version,
        ))
        .await
    }

    /// 按部门版本 CAS 软禁用部门；活动子部门必须先迁移。
    pub async fn disable(
        &self,
        organization_id: OrganizationId,
        department_id: OrganizationDepartmentId,
        expected_version: i64,
    ) -> Result<OrganizationDepartmentRecord, OrganizationDepartmentRepositoryError> {
        validate_expected_version(expected_version)?;
        self.run(self.disable_inner(organization_id, department_id, expected_version))
            .await
    }

    async fn create_inner(
        &self,
        organization_id: OrganizationId,
        write: OrganizationDepartmentCreate,
    ) -> Result<OrganizationDepartmentRecord, OrganizationDepartmentRepositoryError> {
        validate_department_name(&write.name)?;
        let transaction = self.begin().await?;
        lock_active_organization(&transaction, organization_id).await?;
        let models = load_models(&transaction, organization_id, true).await?;
        validate_models(&models)?;
        let depth = match write.parent_id {
            None => 1,
            Some(parent_id) => {
                let parent = if let Some(parent) = find_scoped_model(&models, parent_id) {
                    parent
                } else {
                    if find_any_model(&transaction, parent_id).await?.is_some() {
                        return Err(
                            OrganizationDepartmentRepositoryError::ParentOrganizationMismatch,
                        );
                    }
                    return Err(OrganizationDepartmentRepositoryError::ParentNotFound);
                };
                if parent.status != ACTIVE_DEPARTMENT {
                    return Err(OrganizationDepartmentRepositoryError::ParentDisabled);
                }
                parent
                    .depth
                    .checked_add(1)
                    .and_then(|value| u8::try_from(value).ok())
                    .filter(|value| *value <= MAX_ORGANIZATION_DEPARTMENT_DEPTH)
                    .ok_or(OrganizationDepartmentRepositoryError::DepthExceeded)?
            }
        };
        let key = generate_department_key()?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_departments::ActiveModel {
            organization_id: Set(organization_id.get()),
            department_key: Set(key.persistence_key()),
            parent_department_id: Set(write.parent_id.map(OrganizationDepartmentId::get)),
            name: Set(write.name),
            sort_order: Set(write.sort_order),
            depth: Set(i16::from(depth)),
            status: Set(ACTIVE_DEPARTMENT),
            version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            disabled_at: Set(None),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(map_write_error)?;
        let record = record_from_model(model)?;
        transaction.commit().await.map_err(query_error)?;
        Ok(record)
    }

    async fn list_inner(
        &self,
        organization_id: OrganizationId,
        after: Option<OrganizationDepartmentCursor>,
        limit: usize,
    ) -> Result<OrganizationDepartmentPageRecord, OrganizationDepartmentRepositoryError> {
        validate_limit(limit)?;
        let transaction = self.begin().await?;
        lock_active_organization(&transaction, organization_id).await?;
        let mut models = load_models(&transaction, organization_id, false).await?;
        validate_models(&models)?;
        models.sort_by_key(|model| (model.sort_order, model.id));
        let mut records = models
            .into_iter()
            .map(record_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(cursor) = after {
            records.retain(|record| {
                (record.sort_order(), record.id().get()) > (cursor.sort_order(), cursor.id().get())
            });
        }
        let has_more = records.len() > limit;
        if has_more {
            records.truncate(limit);
        }
        let next_cursor = has_more.then(|| {
            records
                .last()
                .map(|record| OrganizationDepartmentCursor::new(record.sort_order(), record.id()))
        });
        transaction.commit().await.map_err(query_error)?;
        Ok(OrganizationDepartmentPageRecord {
            records,
            next_cursor: next_cursor.flatten(),
        })
    }

    async fn get_inner(
        &self,
        organization_id: OrganizationId,
        department_id: OrganizationDepartmentId,
    ) -> Result<OrganizationDepartmentRecord, OrganizationDepartmentRepositoryError> {
        let transaction = self.begin().await?;
        lock_active_organization(&transaction, organization_id).await?;
        let models = load_models(&transaction, organization_id, false).await?;
        validate_models(&models)?;
        let model = find_scoped_model(&models, department_id)
            .ok_or(OrganizationDepartmentRepositoryError::NotFound)?;
        let record = record_from_model(model)?;
        transaction.commit().await.map_err(query_error)?;
        Ok(record)
    }

    async fn move_inner(
        &self,
        organization_id: OrganizationId,
        department_id: OrganizationDepartmentId,
        new_parent_id: Option<OrganizationDepartmentId>,
        expected_version: i64,
    ) -> Result<OrganizationDepartmentRecord, OrganizationDepartmentRepositoryError> {
        let transaction = self.begin().await?;
        lock_active_organization(&transaction, organization_id).await?;
        let models = load_models(&transaction, organization_id, true).await?;
        validate_models(&models)?;
        let moving = find_scoped_model(&models, department_id)
            .ok_or(OrganizationDepartmentRepositoryError::NotFound)?;
        if moving.version != expected_version {
            return Err(OrganizationDepartmentRepositoryError::Conflict);
        }
        if let Some(parent_id) = new_parent_id
            && find_scoped_model(&models, parent_id).is_none()
            && find_any_model(&transaction, parent_id).await?.is_some()
        {
            return Err(OrganizationDepartmentRepositoryError::ParentOrganizationMismatch);
        }
        let nodes = nodes_from_models(&models)?;
        let new_depth = validate_organization_department_move(
            &nodes,
            organization_id,
            department_id,
            new_parent_id,
        )
        .map_err(map_tree_error)?;
        let delta = i16::from(new_depth) - moving.depth;
        let by_id = models
            .iter()
            .map(|model| (model.id, model))
            .collect::<HashMap<_, _>>();
        let mut descendants = Vec::new();
        for model in &models {
            if model.id == moving.id || !is_descendant(model.id, moving.id, &by_id) {
                continue;
            }
            let depth = i16::from(model.depth)
                .checked_add(delta)
                .filter(|value| (1..=i16::from(MAX_ORGANIZATION_DEPARTMENT_DEPTH)).contains(value))
                .ok_or(OrganizationDepartmentRepositoryError::DepthExceeded)?;
            let version = model
                .version
                .checked_add(1)
                .filter(|value| *value > 0)
                .ok_or(OrganizationDepartmentRepositoryError::Invariant)?;
            descendants.push((model.id, model.version, depth, version));
        }
        let moving_version = moving
            .version
            .checked_add(1)
            .filter(|value| *value > 0)
            .ok_or(OrganizationDepartmentRepositoryError::Invariant)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let updated = organization_departments::Entity::update_many()
            .filter(organization_departments::Column::Id.eq(moving.id))
            .filter(organization_departments::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_departments::Column::Version.eq(expected_version))
            .col_expr(
                organization_departments::Column::ParentDepartmentId,
                Expr::value(new_parent_id.map(OrganizationDepartmentId::get)),
            )
            .col_expr(
                organization_departments::Column::Depth,
                Expr::value(i16::from(new_depth)),
            )
            .col_expr(
                organization_departments::Column::Version,
                Expr::value(moving_version),
            )
            .col_expr(
                organization_departments::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if updated.rows_affected != 1 {
            return Err(OrganizationDepartmentRepositoryError::Conflict);
        }
        for (id, version, depth, next_version) in descendants {
            let updated = organization_departments::Entity::update_many()
                .filter(organization_departments::Column::Id.eq(id))
                .filter(organization_departments::Column::OrganizationId.eq(organization_id.get()))
                .filter(organization_departments::Column::Version.eq(version))
                .col_expr(organization_departments::Column::Depth, Expr::value(depth))
                .col_expr(
                    organization_departments::Column::Version,
                    Expr::value(next_version),
                )
                .col_expr(
                    organization_departments::Column::UpdatedAt,
                    Expr::value(now),
                )
                .exec(&transaction)
                .await
                .map_err(query_error)?;
            if updated.rows_affected != 1 {
                return Err(OrganizationDepartmentRepositoryError::Conflict);
            }
        }
        let updated = organization_departments::Entity::find_by_id(moving.id)
            .filter(organization_departments::Column::OrganizationId.eq(organization_id.get()))
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or(OrganizationDepartmentRepositoryError::Invariant)?;
        let record = record_from_model(updated)?;
        transaction.commit().await.map_err(query_error)?;
        Ok(record)
    }

    async fn disable_inner(
        &self,
        organization_id: OrganizationId,
        department_id: OrganizationDepartmentId,
        expected_version: i64,
    ) -> Result<OrganizationDepartmentRecord, OrganizationDepartmentRepositoryError> {
        let transaction = self.begin().await?;
        lock_active_organization(&transaction, organization_id).await?;
        let models = load_models(&transaction, organization_id, true).await?;
        validate_models(&models)?;
        let department = find_scoped_model(&models, department_id)
            .ok_or(OrganizationDepartmentRepositoryError::NotFound)?;
        if department.version != expected_version {
            return Err(OrganizationDepartmentRepositoryError::Conflict);
        }
        let has_active_children = models.iter().any(|model| {
            model.parent_department_id == Some(department.id) && model.status == ACTIVE_DEPARTMENT
        });
        OrganizationDepartmentDisableFacts::new(
            OrganizationDepartmentStatus::try_from(department.status)
                .map_err(|_| OrganizationDepartmentRepositoryError::Invariant)?,
            has_active_children,
            false,
            false,
            false,
        )
        .validate()
        .map_err(map_disable_rejection)?;
        let version = department
            .version
            .checked_add(1)
            .filter(|value| *value > 0)
            .ok_or(OrganizationDepartmentRepositoryError::Invariant)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let updated = organization_departments::Entity::update_many()
            .filter(organization_departments::Column::Id.eq(department.id))
            .filter(organization_departments::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_departments::Column::Status.eq(ACTIVE_DEPARTMENT))
            .filter(organization_departments::Column::Version.eq(expected_version))
            .col_expr(
                organization_departments::Column::Status,
                Expr::value(DISABLED_DEPARTMENT),
            )
            .col_expr(
                organization_departments::Column::Version,
                Expr::value(version),
            )
            .col_expr(
                organization_departments::Column::DisabledAt,
                Expr::value(Some(now)),
            )
            .col_expr(
                organization_departments::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if updated.rows_affected != 1 {
            return Err(OrganizationDepartmentRepositoryError::Conflict);
        }
        let updated = organization_departments::Entity::find_by_id(department.id)
            .filter(organization_departments::Column::OrganizationId.eq(organization_id.get()))
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or(OrganizationDepartmentRepositoryError::Invariant)?;
        let record = record_from_model(updated)?;
        transaction.commit().await.map_err(query_error)?;
        Ok(record)
    }

    async fn begin(&self) -> Result<DatabaseTransaction, OrganizationDepartmentRepositoryError> {
        self.pool.connection().begin().await.map_err(query_error)
    }

    async fn run<T>(
        &self,
        operation: impl std::future::Future<Output = Result<T, OrganizationDepartmentRepositoryError>>,
    ) -> Result<T, OrganizationDepartmentRepositoryError> {
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationDepartmentRepositoryError::Timeout)?
    }
}

impl fmt::Debug for OrganizationDepartmentRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationDepartmentRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn lock_active_organization(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<(), OrganizationDepartmentRepositoryError> {
    let mut query = organizations::Entity::find_by_id(organization_id.get());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let organization = query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationDepartmentRepositoryError::OrganizationUnavailable)?;
    if organization.status != ACTIVE_ORGANIZATION || organization.closed_at.is_some() {
        return Err(OrganizationDepartmentRepositoryError::OrganizationUnavailable);
    }
    Ok(())
}

async fn load_models(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    lock: bool,
) -> Result<Vec<organization_departments::Model>, OrganizationDepartmentRepositoryError> {
    let mut query = organization_departments::Entity::find()
        .filter(organization_departments::Column::OrganizationId.eq(organization_id.get()))
        .order_by_asc(organization_departments::Column::Id);
    if lock && transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.all(transaction).await.map_err(query_error)
}

async fn find_any_model(
    transaction: &DatabaseTransaction,
    department_id: OrganizationDepartmentId,
) -> Result<Option<organization_departments::Model>, OrganizationDepartmentRepositoryError> {
    organization_departments::Entity::find_by_id(department_id.get())
        .one(transaction)
        .await
        .map_err(query_error)
}

fn find_scoped_model(
    models: &[organization_departments::Model],
    department_id: OrganizationDepartmentId,
) -> Option<organization_departments::Model> {
    models
        .iter()
        .find(|model| model.id == department_id.get())
        .cloned()
}

fn nodes_from_models(
    models: &[organization_departments::Model],
) -> Result<Vec<OrganizationDepartmentNode>, OrganizationDepartmentRepositoryError> {
    models.iter().map(node_from_model).collect()
}

fn node_from_model(
    model: &organization_departments::Model,
) -> Result<OrganizationDepartmentNode, OrganizationDepartmentRepositoryError> {
    let organization_id = OrganizationId::new(model.organization_id).map_err(|_| invariant())?;
    let department_id = OrganizationDepartmentId::new(model.id).map_err(|_| invariant())?;
    let parent_id = model
        .parent_department_id
        .map(OrganizationDepartmentId::new)
        .transpose()
        .map_err(|_| invariant())?;
    let depth = u8::try_from(model.depth).map_err(|_| invariant())?;
    let status = OrganizationDepartmentStatus::try_from(model.status).map_err(|_| invariant())?;
    OrganizationDepartmentNode::new(
        organization_id,
        department_id,
        parent_id,
        depth,
        status,
        model.version,
    )
    .map_err(map_tree_error)
}

fn validate_models(
    models: &[organization_departments::Model],
) -> Result<(), OrganizationDepartmentRepositoryError> {
    let nodes = nodes_from_models(models)?;
    validate_organization_department_tree(&nodes).map_err(map_tree_error)
}

fn record_from_model(
    model: organization_departments::Model,
) -> Result<OrganizationDepartmentRecord, OrganizationDepartmentRepositoryError> {
    let organization_id = OrganizationId::new(model.organization_id).map_err(|_| invariant())?;
    let id = OrganizationDepartmentId::new(model.id).map_err(|_| invariant())?;
    let department_key = OrganizationDepartmentKey::from_persistence_key(&model.department_key)
        .map_err(|_| invariant())?;
    let parent_id = model
        .parent_department_id
        .map(OrganizationDepartmentId::new)
        .transpose()
        .map_err(|_| invariant())?;
    let depth = u8::try_from(model.depth).map_err(|_| invariant())?;
    let status = OrganizationDepartmentStatus::try_from(model.status).map_err(|_| invariant())?;
    if model.version <= 0
        || depth == 0
        || depth > MAX_ORGANIZATION_DEPARTMENT_DEPTH
        || (status == OrganizationDepartmentStatus::Disabled) != model.disabled_at.is_some()
    {
        return Err(invariant());
    }
    Ok(OrganizationDepartmentRecord {
        id,
        organization_id,
        department_key,
        parent_id,
        name: model.name,
        sort_order: model.sort_order,
        depth,
        status,
        version: model.version,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
        disabled_at: model.disabled_at.map(|value| value.unix_timestamp()),
    })
}

fn generate_department_key()
-> Result<OrganizationDepartmentKey, OrganizationDepartmentRepositoryError> {
    for _ in 0..4 {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|_| OrganizationDepartmentRepositoryError::EntropyUnavailable)?;
        if let Ok(key) = OrganizationDepartmentKey::new(bytes) {
            return Ok(key);
        }
    }
    Err(OrganizationDepartmentRepositoryError::EntropyUnavailable)
}

fn validate_department_name(value: &str) -> Result<(), OrganizationDepartmentRepositoryError> {
    if value.is_empty()
        || value.len() > MAX_ORGANIZATION_DEPARTMENT_NAME_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(OrganizationDepartmentRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_limit(limit: usize) -> Result<(), OrganizationDepartmentRepositoryError> {
    if limit == 0 || limit > MAX_ORGANIZATION_DEPARTMENT_PAGE_SIZE {
        return Err(OrganizationDepartmentRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_expected_version(version: i64) -> Result<(), OrganizationDepartmentRepositoryError> {
    if version <= 0 || version == i64::MAX {
        return Err(OrganizationDepartmentRepositoryError::InvalidInput);
    }
    Ok(())
}

fn is_descendant(
    candidate_id: i64,
    ancestor_id: i64,
    by_id: &HashMap<i64, &organization_departments::Model>,
) -> bool {
    let mut current_id = candidate_id;
    let mut visited = HashMap::new();
    while let Some(model) = by_id.get(&current_id) {
        if visited.insert(current_id, ()).is_some() {
            return false;
        }
        match model.parent_department_id {
            Some(parent_id) if parent_id == ancestor_id => return true,
            Some(parent_id) => current_id = parent_id,
            None => return false,
        }
    }
    false
}

fn map_tree_error(error: OrganizationDepartmentTreeError) -> OrganizationDepartmentRepositoryError {
    match error {
        OrganizationDepartmentTreeError::ParentNotFound => {
            OrganizationDepartmentRepositoryError::ParentNotFound
        }
        OrganizationDepartmentTreeError::ParentOrganizationMismatch => {
            OrganizationDepartmentRepositoryError::ParentOrganizationMismatch
        }
        OrganizationDepartmentTreeError::CycleDetected => {
            OrganizationDepartmentRepositoryError::CycleDetected
        }
        OrganizationDepartmentTreeError::SelfParent => {
            OrganizationDepartmentRepositoryError::SelfParent
        }
        OrganizationDepartmentTreeError::ParentDisabled => {
            OrganizationDepartmentRepositoryError::ParentDisabled
        }
        OrganizationDepartmentTreeError::DepthExceeded => {
            OrganizationDepartmentRepositoryError::DepthExceeded
        }
        OrganizationDepartmentTreeError::DepartmentOrganizationMismatch => {
            OrganizationDepartmentRepositoryError::ParentOrganizationMismatch
        }
        OrganizationDepartmentTreeError::InvalidStatus
        | OrganizationDepartmentTreeError::InvalidVersion
        | OrganizationDepartmentTreeError::DuplicateDepartment => invariant(),
    }
}

fn map_disable_rejection(
    rejection: OrganizationDepartmentDisableRejection,
) -> OrganizationDepartmentRepositoryError {
    match rejection {
        OrganizationDepartmentDisableRejection::AlreadyDisabled => {
            OrganizationDepartmentRepositoryError::AlreadyDisabled
        }
        OrganizationDepartmentDisableRejection::ActiveChildren => {
            OrganizationDepartmentRepositoryError::ActiveChildren
        }
        OrganizationDepartmentDisableRejection::ActiveMembers => {
            OrganizationDepartmentRepositoryError::ActiveMembers
        }
        OrganizationDepartmentDisableRejection::ActiveBudgetBindings => {
            OrganizationDepartmentRepositoryError::ActiveBudgetBindings
        }
        OrganizationDepartmentDisableRejection::ActiveServiceAccountBindings => {
            OrganizationDepartmentRepositoryError::ActiveServiceAccountBindings
        }
    }
}

fn map_write_error(error: DbErr) -> OrganizationDepartmentRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        OrganizationDepartmentRepositoryError::Conflict
    } else {
        query_error(error)
    }
}

fn query_error(_error: impl fmt::Debug) -> OrganizationDepartmentRepositoryError {
    OrganizationDepartmentRepositoryError::Query
}

fn invariant() -> OrganizationDepartmentRepositoryError {
    OrganizationDepartmentRepositoryError::Invariant
}
