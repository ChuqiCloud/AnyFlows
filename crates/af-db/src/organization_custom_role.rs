use std::{collections::HashSet, fmt, future::Future, time::Duration};

use af_domain::{
    OrganizationCustomRoleId, OrganizationCustomRoleKey, OrganizationCustomRoleStatus,
    OrganizationId, OrganizationMembershipStatus, OrganizationPermission, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, SqlErr,
    TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{
        organization_custom_role_permissions, organization_custom_roles, organization_memberships,
        organizations,
    },
};

/// 企业自定义角色名称最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_CUSTOM_ROLE_NAME_BYTES: usize = 128;
/// 企业自定义角色说明最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_CUSTOM_ROLE_DESCRIPTION_BYTES: usize = 2_000;
/// 企业自定义角色目录单页最大记录数。
pub const MAX_ORGANIZATION_CUSTOM_ROLE_PAGE_SIZE: usize = 100;

/// 创建企业自定义角色所需的受控事实。
pub struct OrganizationCustomRoleCreate {
    name: String,
    description: Option<String>,
    permissions: Vec<OrganizationPermission>,
    created_by_user_id: UserId,
}

impl OrganizationCustomRoleCreate {
    /// 组装角色定义；公开标识、状态和版本由仓储生成。
    #[must_use]
    pub fn new(
        name: String,
        description: Option<String>,
        permissions: Vec<OrganizationPermission>,
        created_by_user_id: UserId,
    ) -> Self {
        Self {
            name,
            description,
            permissions,
            created_by_user_id,
        }
    }
}

impl fmt::Debug for OrganizationCustomRoleCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationCustomRoleCreate")
            .field("permission_count", &self.permissions.len())
            .finish_non_exhaustive()
    }
}

/// 更新活动自定义角色所需的完整定义和 CAS 版本。
pub struct OrganizationCustomRoleUpdate {
    name: String,
    description: Option<String>,
    permissions: Vec<OrganizationPermission>,
    expected_version: i64,
}

impl OrganizationCustomRoleUpdate {
    /// 组装活动角色完整替换请求；停用使用独立入口。
    #[must_use]
    pub fn new(
        name: String,
        description: Option<String>,
        permissions: Vec<OrganizationPermission>,
        expected_version: i64,
    ) -> Self {
        Self {
            name,
            description,
            permissions,
            expected_version,
        }
    }
}

impl fmt::Debug for OrganizationCustomRoleUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationCustomRoleUpdate")
            .field("permission_count", &self.permissions.len())
            .field("expected_version", &self.expected_version)
            .finish_non_exhaustive()
    }
}

/// 已持久化的企业自定义角色及闭合权限事实。
#[derive(Clone, PartialEq)]
pub struct OrganizationCustomRoleRecord {
    id: OrganizationCustomRoleId,
    organization_id: OrganizationId,
    role_key: OrganizationCustomRoleKey,
    name: String,
    description: Option<String>,
    status: OrganizationCustomRoleStatus,
    permissions: Vec<OrganizationPermission>,
    version: i64,
    created_by_user_id: UserId,
    created_at: i64,
    updated_at: i64,
}

impl OrganizationCustomRoleRecord {
    /// 返回角色内部标识，仅供仓储游标和后续同企业外键使用。
    #[must_use]
    pub const fn id(&self) -> OrganizationCustomRoleId {
        self.id
    }

    /// 返回角色所属企业。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回不可枚举的角色公开标识。
    #[must_use]
    pub const fn role_key(&self) -> OrganizationCustomRoleKey {
        self.role_key
    }

    /// 返回企业内角色显示名。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回可空角色说明。
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// 返回角色目录状态。
    #[must_use]
    pub const fn status(&self) -> OrganizationCustomRoleStatus {
        self.status
    }

    /// 返回按稳定权限码排序的闭合权限集合。
    #[must_use]
    pub fn permissions(&self) -> &[OrganizationPermission] {
        &self.permissions
    }

    /// 返回 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    /// 返回创建角色的企业成员用户。
    #[must_use]
    pub const fn created_by_user_id(&self) -> UserId {
        self.created_by_user_id
    }

    /// 返回创建时间 Unix 秒。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// 返回最近更新时间 Unix 秒。
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

impl fmt::Debug for OrganizationCustomRoleRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationCustomRoleRecord")
            .field("id", &self.id)
            .field("organization_id", &self.organization_id)
            .field("role_key", &self.role_key)
            .field("status", &self.status)
            .field("permission_count", &self.permissions.len())
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 企业范围自定义角色稳定游标分页结果。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationCustomRolePageRecord {
    items: Vec<OrganizationCustomRoleRecord>,
    next_cursor: Option<OrganizationCustomRoleId>,
}

impl OrganizationCustomRolePageRecord {
    /// 返回当前页角色定义。
    #[must_use]
    pub fn items(&self) -> &[OrganizationCustomRoleRecord] {
        &self.items
    }

    /// 返回下一页使用的内部稳定游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<OrganizationCustomRoleId> {
        self.next_cursor
    }
}

/// 企业自定义角色仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCustomRoleRepositoryConfigError {
    /// 零超时无法形成有效的数据库截止边界。
    #[error("企业自定义角色仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 企业自定义角色仓储错误；不携带角色名、说明或外部标识原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCustomRoleRepositoryError {
    /// 名称、说明、权限集合、分页或版本输入无效。
    #[error("企业自定义角色输入无效")]
    InvalidInput,
    /// 目标企业或角色在给定企业范围内不存在。
    #[error("企业自定义角色不存在")]
    NotFound,
    /// 创建者不是当前企业的活动成员。
    #[error("企业自定义角色创建者不可用")]
    CreatorUnavailable,
    /// 企业内名称或 CAS 版本与当前状态冲突。
    #[error("企业自定义角色状态冲突")]
    Conflict,
    /// 停用角色不可更新或再次停用。
    #[error("企业自定义角色状态转换无效")]
    InvalidTransition,
    /// 系统随机源无法生成角色公开标识。
    #[error("企业自定义角色公开标识生成失败")]
    EntropyUnavailable,
    /// 获取连接、事务或执行数据库语句失败。
    #[error("企业自定义角色数据库操作失败")]
    Query,
    /// 单次数据库操作超过硬截止时间。
    #[error("企业自定义角色数据库操作超时")]
    Timeout,
    /// 持久化数据违反企业范围或权限白名单约束。
    #[error("企业自定义角色持久化状态损坏")]
    Invariant,
}

/// 企业自定义角色目录与逐条权限事实仓储。
#[derive(Clone)]
pub struct OrganizationCustomRoleRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationCustomRoleRepository {
    /// 使用共享连接池和非零硬截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationCustomRoleRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationCustomRoleRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 在企业范围内创建活动角色及其逐条权限事实。
    pub async fn create_role(
        &self,
        organization_id: OrganizationId,
        write: OrganizationCustomRoleCreate,
    ) -> Result<OrganizationCustomRoleRecord, OrganizationCustomRoleRepositoryError> {
        self.run(self.create_role_inner(organization_id, write))
            .await
    }

    /// 按企业和公开标识读取角色；跨企业标识统一视为不存在。
    pub async fn role(
        &self,
        organization_id: OrganizationId,
        role_key: OrganizationCustomRoleKey,
    ) -> Result<OrganizationCustomRoleRecord, OrganizationCustomRoleRepositoryError> {
        self.run(self.role_inner(organization_id, role_key)).await
    }

    /// 按内部 ID 稳定游标列出企业角色目录。
    pub async fn list_roles(
        &self,
        organization_id: OrganizationId,
        after: Option<OrganizationCustomRoleId>,
        limit: usize,
    ) -> Result<OrganizationCustomRolePageRecord, OrganizationCustomRoleRepositoryError> {
        self.run(self.list_roles_inner(organization_id, after, limit))
            .await
    }

    /// 按 CAS 完整替换活动角色资料和权限集合。
    pub async fn update_role(
        &self,
        organization_id: OrganizationId,
        role_key: OrganizationCustomRoleKey,
        write: OrganizationCustomRoleUpdate,
    ) -> Result<OrganizationCustomRoleRecord, OrganizationCustomRoleRepositoryError> {
        self.run(self.update_role_inner(organization_id, role_key, write))
            .await
    }

    /// 按 CAS 不可逆停用角色，保留定义和权限历史事实。
    pub async fn disable_role(
        &self,
        organization_id: OrganizationId,
        role_key: OrganizationCustomRoleKey,
        expected_version: i64,
    ) -> Result<OrganizationCustomRoleRecord, OrganizationCustomRoleRepositoryError> {
        self.run(self.disable_role_inner(organization_id, role_key, expected_version))
            .await
    }

    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, OrganizationCustomRoleRepositoryError>>,
    ) -> Result<T, OrganizationCustomRoleRepositoryError> {
        match timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                OrganizationCustomRoleRepositoryError::Timeout,
            )),
        }
    }

    async fn create_role_inner(
        &self,
        organization_id: OrganizationId,
        write: OrganizationCustomRoleCreate,
    ) -> Result<OrganizationCustomRoleRecord, OrganizationCustomRoleRepositoryError> {
        let ValidatedRoleFields {
            name,
            description,
            permissions,
            expected_version: _,
        } = ValidatedRoleFields::new(write.name, write.description, write.permissions, None)?;
        let role_key = generate_role_key()?;
        let transaction = begin(&self.pool).await?;
        if !organization_exists(&transaction, organization_id).await? {
            rollback(transaction).await?;
            return Err(OrganizationCustomRoleRepositoryError::NotFound);
        }
        if !active_creator_exists(&transaction, organization_id, write.created_by_user_id).await? {
            rollback(transaction).await?;
            return Err(OrganizationCustomRoleRepositoryError::CreatorUnavailable);
        }

        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = match (organization_custom_roles::ActiveModel {
            organization_id: Set(organization_id.get()),
            role_key: Set(role_key.persistence_key()),
            name: Set(name),
            description: Set(description),
            status: Set(OrganizationCustomRoleStatus::Active.code()),
            version: Set(1),
            created_by_user_id: Set(write.created_by_user_id.get()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        })
        .insert(&transaction)
        .await
        {
            Ok(model) => model,
            Err(error) => {
                let mapped = map_write_error(error);
                rollback(transaction).await?;
                return Err(mapped);
            }
        };
        if let Err(error) = insert_permissions(&transaction, model.id, &permissions).await {
            rollback(transaction).await?;
            return Err(error);
        }
        let record = record_from_parts(model, permissions)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn role_inner(
        &self,
        organization_id: OrganizationId,
        role_key: OrganizationCustomRoleKey,
    ) -> Result<OrganizationCustomRoleRecord, OrganizationCustomRoleRepositoryError> {
        let model = organization_custom_roles::Entity::find()
            .filter(organization_custom_roles::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_custom_roles::Column::RoleKey.eq(role_key.persistence_key()))
            .one(self.pool.connection())
            .await
            .map_err(query_error)?
            .ok_or(OrganizationCustomRoleRepositoryError::NotFound)?;
        record_from_model(self.pool.connection(), model).await
    }

    async fn list_roles_inner(
        &self,
        organization_id: OrganizationId,
        after: Option<OrganizationCustomRoleId>,
        limit: usize,
    ) -> Result<OrganizationCustomRolePageRecord, OrganizationCustomRoleRepositoryError> {
        if limit == 0 || limit > MAX_ORGANIZATION_CUSTOM_ROLE_PAGE_SIZE {
            return Err(OrganizationCustomRoleRepositoryError::InvalidInput);
        }
        let fetch_limit = u64::try_from(limit)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(OrganizationCustomRoleRepositoryError::InvalidInput)?;
        let mut query = organization_custom_roles::Entity::find()
            .filter(organization_custom_roles::Column::OrganizationId.eq(organization_id.get()))
            .order_by_asc(organization_custom_roles::Column::Id)
            .limit(fetch_limit);
        if let Some(after) = after {
            query = query.filter(organization_custom_roles::Column::Id.gt(after.get()));
        }
        let mut models = query
            .all(self.pool.connection())
            .await
            .map_err(query_error)?;
        let has_more = models.len() > limit;
        models.truncate(limit);
        let next_cursor = if has_more {
            models
                .last()
                .map(|model| OrganizationCustomRoleId::new(model.id))
                .transpose()
                .map_err(|_| invariant())?
        } else {
            None
        };
        let mut items = Vec::with_capacity(models.len());
        for model in models {
            items.push(record_from_model(self.pool.connection(), model).await?);
        }
        Ok(OrganizationCustomRolePageRecord { items, next_cursor })
    }

    async fn update_role_inner(
        &self,
        organization_id: OrganizationId,
        role_key: OrganizationCustomRoleKey,
        write: OrganizationCustomRoleUpdate,
    ) -> Result<OrganizationCustomRoleRecord, OrganizationCustomRoleRepositoryError> {
        let fields = ValidatedRoleFields::new(
            write.name,
            write.description,
            write.permissions,
            Some(write.expected_version),
        )?;
        let transaction = begin(&self.pool).await?;
        let current = lock_role(&transaction, organization_id, role_key).await?;
        validate_active_cas(&current, fields.expected_version.expect("版本已经校验"))?;
        let next_version = current.version.checked_add(1).ok_or_else(invariant)?;
        let result = organization_custom_roles::Entity::update_many()
            .filter(organization_custom_roles::Column::Id.eq(current.id))
            .filter(organization_custom_roles::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_custom_roles::Column::Version.eq(current.version))
            .filter(
                organization_custom_roles::Column::Status
                    .eq(OrganizationCustomRoleStatus::Active.code()),
            )
            .col_expr(
                organization_custom_roles::Column::Name,
                Expr::value(fields.name),
            )
            .col_expr(
                organization_custom_roles::Column::Description,
                Expr::value(fields.description),
            )
            .col_expr(
                organization_custom_roles::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_custom_roles::Column::UpdatedAt,
                Expr::value(TimeDateTimeWithTimeZone::now_utc()),
            )
            .exec(&transaction)
            .await
            .map_err(map_write_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationCustomRoleRepositoryError::Conflict);
        }
        if let Err(error) = organization_custom_role_permissions::Entity::delete_many()
            .filter(organization_custom_role_permissions::Column::CustomRoleId.eq(current.id))
            .exec(&transaction)
            .await
            .map_err(query_error)
        {
            rollback(transaction).await?;
            return Err(error);
        }
        if let Err(error) = insert_permissions(&transaction, current.id, &fields.permissions).await
        {
            rollback(transaction).await?;
            return Err(error);
        }
        let model = organization_custom_roles::Entity::find_by_id(current.id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = record_from_parts(model, fields.permissions)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn disable_role_inner(
        &self,
        organization_id: OrganizationId,
        role_key: OrganizationCustomRoleKey,
        expected_version: i64,
    ) -> Result<OrganizationCustomRoleRecord, OrganizationCustomRoleRepositoryError> {
        validate_expected_version(expected_version)?;
        let transaction = begin(&self.pool).await?;
        let current = lock_role(&transaction, organization_id, role_key).await?;
        validate_active_cas(&current, expected_version)?;
        let next_version = current.version.checked_add(1).ok_or_else(invariant)?;
        let result = organization_custom_roles::Entity::update_many()
            .filter(organization_custom_roles::Column::Id.eq(current.id))
            .filter(organization_custom_roles::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_custom_roles::Column::Version.eq(current.version))
            .filter(
                organization_custom_roles::Column::Status
                    .eq(OrganizationCustomRoleStatus::Active.code()),
            )
            .col_expr(
                organization_custom_roles::Column::Status,
                Expr::value(OrganizationCustomRoleStatus::Disabled.code()),
            )
            .col_expr(
                organization_custom_roles::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_custom_roles::Column::UpdatedAt,
                Expr::value(TimeDateTimeWithTimeZone::now_utc()),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationCustomRoleRepositoryError::Conflict);
        }
        let model = organization_custom_roles::Entity::find_by_id(current.id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let permissions = load_permissions(&transaction, current.id).await?;
        let record = record_from_parts(model, permissions)?;
        commit(transaction).await?;
        Ok(record)
    }
}

impl fmt::Debug for OrganizationCustomRoleRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationCustomRoleRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

struct ValidatedRoleFields {
    name: String,
    description: Option<String>,
    permissions: Vec<OrganizationPermission>,
    expected_version: Option<i64>,
}

impl ValidatedRoleFields {
    fn new(
        name: String,
        description: Option<String>,
        permissions: Vec<OrganizationPermission>,
        expected_version: Option<i64>,
    ) -> Result<Self, OrganizationCustomRoleRepositoryError> {
        validate_text(&name, MAX_ORGANIZATION_CUSTOM_ROLE_NAME_BYTES, false)?;
        if let Some(description) = description.as_deref() {
            validate_text(
                description,
                MAX_ORGANIZATION_CUSTOM_ROLE_DESCRIPTION_BYTES,
                false,
            )?;
        }
        if let Some(expected_version) = expected_version {
            validate_expected_version(expected_version)?;
        }
        let permissions = validate_permissions(permissions)?;
        Ok(Self {
            name,
            description,
            permissions,
            expected_version,
        })
    }
}

async fn record_from_model<C>(
    connection: &C,
    model: organization_custom_roles::Model,
) -> Result<OrganizationCustomRoleRecord, OrganizationCustomRoleRepositoryError>
where
    C: ConnectionTrait,
{
    let permissions = load_permissions(connection, model.id).await?;
    record_from_parts(model, permissions)
}

fn record_from_parts(
    model: organization_custom_roles::Model,
    permissions: Vec<OrganizationPermission>,
) -> Result<OrganizationCustomRoleRecord, OrganizationCustomRoleRepositoryError> {
    validate_text(&model.name, MAX_ORGANIZATION_CUSTOM_ROLE_NAME_BYTES, false)
        .map_err(|_| invariant())?;
    if let Some(description) = model.description.as_deref() {
        validate_text(
            description,
            MAX_ORGANIZATION_CUSTOM_ROLE_DESCRIPTION_BYTES,
            false,
        )
        .map_err(|_| invariant())?;
    }
    let permissions = validate_permissions(permissions).map_err(|_| invariant())?;
    if model.version <= 0 {
        return Err(invariant());
    }
    Ok(OrganizationCustomRoleRecord {
        id: OrganizationCustomRoleId::new(model.id).map_err(|_| invariant())?,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        role_key: OrganizationCustomRoleKey::from_persistence_key(&model.role_key)
            .map_err(|_| invariant())?,
        name: model.name,
        description: model.description,
        status: OrganizationCustomRoleStatus::try_from(model.status).map_err(|_| invariant())?,
        permissions,
        version: model.version,
        created_by_user_id: UserId::new(model.created_by_user_id).map_err(|_| invariant())?,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
    })
}

async fn load_permissions<C>(
    connection: &C,
    custom_role_id: i64,
) -> Result<Vec<OrganizationPermission>, OrganizationCustomRoleRepositoryError>
where
    C: ConnectionTrait,
{
    let models = organization_custom_role_permissions::Entity::find()
        .filter(organization_custom_role_permissions::Column::CustomRoleId.eq(custom_role_id))
        .order_by_asc(organization_custom_role_permissions::Column::PermissionCode)
        .all(connection)
        .await
        .map_err(query_error)?;
    let mut permissions = Vec::with_capacity(models.len());
    let mut seen = HashSet::with_capacity(models.len());
    for model in models {
        let permission =
            OrganizationPermission::from_code(&model.permission_code).map_err(|_| invariant())?;
        if !permission.is_custom_role_grantable() || !seen.insert(permission) {
            return Err(invariant());
        }
        permissions.push(permission);
    }
    Ok(permissions)
}

async fn insert_permissions(
    transaction: &DatabaseTransaction,
    custom_role_id: i64,
    permissions: &[OrganizationPermission],
) -> Result<(), OrganizationCustomRoleRepositoryError> {
    if permissions.is_empty() {
        return Ok(());
    }
    let now = TimeDateTimeWithTimeZone::now_utc();
    let models =
        permissions.iter().map(
            |permission| organization_custom_role_permissions::ActiveModel {
                custom_role_id: Set(custom_role_id),
                permission_code: Set(permission.code().to_owned()),
                created_at: Set(now),
                ..Default::default()
            },
        );
    organization_custom_role_permissions::Entity::insert_many(models)
        .exec(transaction)
        .await
        .map_err(query_error)?;
    Ok(())
}

fn validate_permissions(
    mut permissions: Vec<OrganizationPermission>,
) -> Result<Vec<OrganizationPermission>, OrganizationCustomRoleRepositoryError> {
    if permissions.len() > OrganizationPermission::custom_role_grantable_permissions().len() {
        return Err(OrganizationCustomRoleRepositoryError::InvalidInput);
    }
    let mut seen = HashSet::with_capacity(permissions.len());
    if permissions
        .iter()
        .any(|permission| !permission.is_custom_role_grantable() || !seen.insert(*permission))
    {
        return Err(OrganizationCustomRoleRepositoryError::InvalidInput);
    }
    permissions.sort_unstable_by_key(|permission| permission.code());
    Ok(permissions)
}

fn validate_text(
    value: &str,
    maximum_bytes: usize,
    allow_empty: bool,
) -> Result<(), OrganizationCustomRoleRepositoryError> {
    if (!allow_empty && value.is_empty())
        || value.len() > maximum_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(OrganizationCustomRoleRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_expected_version(
    expected_version: i64,
) -> Result<(), OrganizationCustomRoleRepositoryError> {
    if expected_version <= 0 || expected_version == i64::MAX {
        return Err(OrganizationCustomRoleRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_active_cas(
    model: &organization_custom_roles::Model,
    expected_version: i64,
) -> Result<(), OrganizationCustomRoleRepositoryError> {
    if model.version != expected_version {
        return Err(OrganizationCustomRoleRepositoryError::Conflict);
    }
    let status = OrganizationCustomRoleStatus::try_from(model.status).map_err(|_| invariant())?;
    if !status.is_active() {
        return Err(OrganizationCustomRoleRepositoryError::InvalidTransition);
    }
    Ok(())
}

async fn organization_exists(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<bool, OrganizationCustomRoleRepositoryError> {
    organizations::Entity::find_by_id(organization_id.get())
        .count(transaction)
        .await
        .map(|count| count == 1)
        .map_err(query_error)
}

async fn active_creator_exists(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<bool, OrganizationCustomRoleRepositoryError> {
    organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(user_id.get()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .count(transaction)
        .await
        .map(|count| count == 1)
        .map_err(query_error)
}

async fn lock_role(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    role_key: OrganizationCustomRoleKey,
) -> Result<organization_custom_roles::Model, OrganizationCustomRoleRepositoryError> {
    let mut query = organization_custom_roles::Entity::find()
        .filter(organization_custom_roles::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_custom_roles::Column::RoleKey.eq(role_key.persistence_key()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationCustomRoleRepositoryError::NotFound)
}

fn generate_role_key() -> Result<OrganizationCustomRoleKey, OrganizationCustomRoleRepositoryError> {
    for _ in 0..4 {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|_| OrganizationCustomRoleRepositoryError::EntropyUnavailable)?;
        if let Ok(key) = OrganizationCustomRoleKey::new(bytes) {
            return Ok(key);
        }
    }
    Err(OrganizationCustomRoleRepositoryError::EntropyUnavailable)
}

async fn begin(
    pool: &DatabasePool,
) -> Result<DatabaseTransaction, OrganizationCustomRoleRepositoryError> {
    pool.connection().begin().await.map_err(query_error)
}

async fn commit(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationCustomRoleRepositoryError> {
    transaction.commit().await.map_err(query_error)
}

async fn rollback(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationCustomRoleRepositoryError> {
    transaction.rollback().await.map_err(query_error)
}

fn map_write_error(error: DbErr) -> OrganizationCustomRoleRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        OrganizationCustomRoleRepositoryError::Conflict
    } else {
        query_error(error)
    }
}

fn query_error(_error: DbErr) -> OrganizationCustomRoleRepositoryError {
    record_internal_error(OrganizationCustomRoleRepositoryError::Query)
}

fn invariant() -> OrganizationCustomRoleRepositoryError {
    record_internal_error(OrganizationCustomRoleRepositoryError::Invariant)
}

fn record_internal_error(
    error: OrganizationCustomRoleRepositoryError,
) -> OrganizationCustomRoleRepositoryError {
    let error_kind = match error {
        OrganizationCustomRoleRepositoryError::InvalidInput
        | OrganizationCustomRoleRepositoryError::NotFound
        | OrganizationCustomRoleRepositoryError::CreatorUnavailable
        | OrganizationCustomRoleRepositoryError::Conflict
        | OrganizationCustomRoleRepositoryError::InvalidTransition
        | OrganizationCustomRoleRepositoryError::EntropyUnavailable => return error,
        OrganizationCustomRoleRepositoryError::Query => "organization_custom_role_query",
        OrganizationCustomRoleRepositoryError::Timeout => "organization_custom_role_timeout",
        OrganizationCustomRoleRepositoryError::Invariant => "organization_custom_role_invariant",
    };
    tracing::error!(
        target: "af_db::organization_custom_role",
        error_kind,
        "企业自定义角色仓储发生内部错误"
    );
    error
}
