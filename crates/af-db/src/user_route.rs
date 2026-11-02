//! 用户自建路由组，成员授权复用个人 Key 选组规则。

use std::{collections::BTreeSet, time::Duration};

use af_domain::{
    GroupId, MAX_USER_ROUTE_MEMBERS, MAX_USER_ROUTE_WEIGHT, MAX_USER_ROUTES, TokenId, UserId,
    UserRouteStrategy,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{tokens, user_route_members, user_routes, users},
    group_key_access::user_can_select_key_group,
    token_owner_guard::lock_non_deleted_owner,
};

mod runtime;
pub use runtime::UserRouteModelHealth;

/// 已验证的用户路由成员，不包含渠道或凭据详情。
#[derive(Clone, Debug)]
pub struct UserRouteMemberRecord {
    pub group_id: GroupId,
    pub priority: i32,
    pub weight: u32,
    pub last_selected_at_millis: Option<i64>,
}

/// 当前所有者范围内的路由快照。
#[derive(Clone, Debug)]
pub struct UserRouteRecord {
    pub id: i64,
    pub owner_user_id: UserId,
    pub name: String,
    pub strategy: UserRouteStrategy,
    pub enabled: bool,
    pub max_group_attempts: u32,
    pub max_price_ratio_micros: Option<i64>,
    pub members: Vec<UserRouteMemberRecord>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 保存路由时允许由用户控制的成员字段。
#[derive(Clone, Debug)]
pub struct UserRouteMemberWrite {
    pub group_id: GroupId,
    pub priority: i32,
    pub weight: u32,
}

/// 用户路由的完整替换配置。
#[derive(Clone, Debug)]
pub struct UserRouteWrite {
    pub name: String,
    pub strategy: UserRouteStrategy,
    pub enabled: bool,
    pub max_group_attempts: u32,
    pub max_price_ratio_micros: Option<i64>,
    pub members: Vec<UserRouteMemberWrite>,
}

impl UserRouteWrite {
    /// 限制输入、成员数量及无嵌套的实体组标识。
    pub fn validate(&self) -> Result<(), UserRouteError> {
        let mut groups = BTreeSet::new();
        if self.name.is_empty()
            || self.name.len() > 128
            || self.name.trim() != self.name
            || self.name.chars().any(char::is_control)
            || !(1..=MAX_USER_ROUTE_MEMBERS).contains(&self.members.len())
            || !(1..=MAX_USER_ROUTE_MEMBERS as u32).contains(&self.max_group_attempts)
            || self.max_price_ratio_micros.is_some_and(|ratio| ratio < 0)
            || self.members.iter().any(|member| {
                member.priority < 0
                    || !(1..=MAX_USER_ROUTE_WEIGHT).contains(&member.weight)
                    || !groups.insert(member.group_id)
            })
        {
            return Err(UserRouteError::InvalidInput);
        }
        Ok(())
    }
}

/// 闭合错误分类；其他用户的资源统一映射为未找到。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserRouteError {
    #[error("用户智能路由参数无效")]
    InvalidInput,
    #[error("登录用户不可用")]
    InvalidSession,
    #[error("用户智能路由不存在")]
    NotFound,
    #[error("用户智能路由数量已达上限")]
    LimitReached,
    #[error("用户智能路由或成员分组不可用")]
    Unavailable,
    #[error("用户智能路由数据库操作失败")]
    Internal,
}

/// 有界的用户所有者范围仓储，同时为网关提供成员与健康状态。
#[derive(Clone)]
pub struct UserRouteRepository {
    pool: DatabasePool,
    lookup_timeout: Duration,
}

impl UserRouteRepository {
    /// 零截止时间无法建立有效数据库边界。
    pub fn new(pool: DatabasePool, lookup_timeout: Duration) -> Result<Self, UserRouteError> {
        if lookup_timeout.is_zero() {
            return Err(UserRouteError::InvalidInput);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 返回当前用户的全部路由，数量由写入事务严格限制。
    pub async fn list(&self, owner: UserId) -> Result<Vec<UserRouteRecord>, UserRouteError> {
        self.bounded(async {
            ensure_owner(self.pool.connection(), owner).await?;
            let rows = user_routes::Entity::find()
                .filter(user_routes::Column::OwnerUserId.eq(owner.get()))
                .filter(user_routes::Column::DeletedAt.is_null())
                .order_by_asc(user_routes::Column::Id)
                .limit((MAX_USER_ROUTES + 1) as u64)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            if rows.len() > MAX_USER_ROUTES {
                return Err(UserRouteError::Internal);
            }
            let mut result = Vec::with_capacity(rows.len());
            for row in rows {
                result.push(snapshot(self.pool.connection(), row).await?);
            }
            Ok(result)
        })
        .await
    }

    /// 不暴露其他用户路由的存在性。
    pub async fn get(&self, owner: UserId, id: i64) -> Result<UserRouteRecord, UserRouteError> {
        self.bounded(async {
            ensure_owner(self.pool.connection(), owner).await?;
            let row = owned_route(self.pool.connection(), owner, id).await?;
            snapshot(self.pool.connection(), row).await
        })
        .await
    }

    /// 锁住用户行后执行容量检查和全体成员授权检查。
    pub async fn create(
        &self,
        owner: UserId,
        write: UserRouteWrite,
    ) -> Result<UserRouteRecord, UserRouteError> {
        write.validate()?;
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let default_group = lock_owner(&txn, owner).await?;
            let count = user_routes::Entity::find()
                .filter(user_routes::Column::OwnerUserId.eq(owner.get()))
                .filter(user_routes::Column::DeletedAt.is_null())
                .count(&txn)
                .await
                .map_err(internal)?;
            if count >= MAX_USER_ROUTES as u64 {
                return Err(UserRouteError::LimitReached);
            }
            validate_member_access(&txn, owner, default_group, &write.members).await?;
            let now = TimeDateTimeWithTimeZone::now_utc();
            let row = user_routes::ActiveModel {
                owner_user_id: Set(owner.get()),
                name: Set(write.name),
                strategy: Set(write.strategy.as_str().to_owned()),
                enabled: Set(write.enabled),
                max_group_attempts: Set(write.max_group_attempts as i32),
                max_price_ratio_micros: Set(write.max_price_ratio_micros),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                ..Default::default()
            }
            .insert(&txn)
            .await
            .map_err(internal)?;
            sync_members(&txn, row.id, &write.members).await?;
            let result = snapshot(&txn, row).await?;
            txn.commit().await.map_err(internal)?;
            Ok(result)
        })
        .await
    }

    /// 保留已有成员健康和轮询状态，移除成员的统计由外键级联清理。
    pub async fn update(
        &self,
        owner: UserId,
        id: i64,
        write: UserRouteWrite,
    ) -> Result<UserRouteRecord, UserRouteError> {
        write.validate()?;
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let default_group = lock_owner(&txn, owner).await?;
            let row = owned_route(&txn, owner, id).await?;
            let row = lock_route(&txn, row.id).await?;
            if row.owner_user_id != owner.get() {
                return Err(UserRouteError::NotFound);
            }
            validate_member_access(&txn, owner, default_group, &write.members).await?;
            let mut row: user_routes::ActiveModel = row.into();
            row.name = Set(write.name);
            row.strategy = Set(write.strategy.as_str().to_owned());
            row.enabled = Set(write.enabled);
            row.max_group_attempts = Set(write.max_group_attempts as i32);
            row.max_price_ratio_micros = Set(write.max_price_ratio_micros);
            row.updated_at = Set(TimeDateTimeWithTimeZone::now_utc());
            let row = row.update(&txn).await.map_err(internal)?;
            sync_members(&txn, row.id, &write.members).await?;
            let result = snapshot(&txn, row).await?;
            txn.commit().await.map_err(internal)?;
            Ok(result)
        })
        .await
    }

    /// 保留墓碑及 Key 关联，删除后永远不会回落到默认分组。
    pub async fn delete(&self, owner: UserId, id: i64) -> Result<(), UserRouteError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_owner(&txn, owner).await?;
            owned_route(&txn, owner, id).await?;
            let row = lock_route(&txn, id).await?;
            if row.owner_user_id != owner.get() {
                return Err(UserRouteError::NotFound);
            }
            let now = TimeDateTimeWithTimeZone::now_utc();
            user_routes::Entity::update_many()
                .filter(user_routes::Column::Id.eq(id))
                .filter(user_routes::Column::OwnerUserId.eq(owner.get()))
                .filter(user_routes::Column::DeletedAt.is_null())
                .col_expr(user_routes::Column::DeletedAt, Expr::value(now))
                .col_expr(user_routes::Column::UpdatedAt, Expr::value(now))
                .col_expr(user_routes::Column::Enabled, Expr::value(false))
                .exec(&txn)
                .await
                .map_err(internal)?;
            txn.commit().await.map_err(internal)
        })
        .await
    }

    /// `None` 仅表示 Key 未绑定路由；失效绑定返回 Unavailable。
    pub async fn lookup_for_token(
        &self,
        owner: UserId,
        token: TokenId,
    ) -> Result<Option<UserRouteRecord>, UserRouteError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let default_group = lock_owner(&txn, owner).await?;
            let token = tokens::Entity::find_by_id(token.get())
                .filter(tokens::Column::UserId.eq(owner.get()))
                .filter(tokens::Column::OrganizationId.is_null())
                .filter(tokens::Column::DeletedAt.is_null())
                .filter(tokens::Column::Status.eq(1_i16))
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(UserRouteError::Unavailable)?;
            let Some(id) = token.user_route_id else {
                txn.commit().await.map_err(internal)?;
                return Ok(None);
            };
            if token.group_id.is_some() || id <= 0 {
                return Err(UserRouteError::Unavailable);
            }
            let row = lock_route(&txn, id).await?;
            if row.owner_user_id != owner.get() {
                return Err(UserRouteError::Unavailable);
            }
            if !row.enabled {
                return Err(UserRouteError::Unavailable);
            }
            let mut route = snapshot_locked(&txn, row).await?;
            // 始终按分组 ID 加锁，避免不同用户的相反优先级形成锁环。
            let groups = route
                .members
                .iter()
                .map(|member| member.group_id)
                .collect::<BTreeSet<_>>();
            let mut eligible = BTreeSet::new();
            for group in groups {
                if group_access(&txn, owner, default_group, group).await? {
                    eligible.insert(group);
                }
            }
            if eligible.is_empty() {
                return Err(UserRouteError::Unavailable);
            }
            // 授权过滤保留用户配置的成员顺序。
            route
                .members
                .retain(|member| eligible.contains(&member.group_id));
            txn.commit().await.map_err(internal)?;
            Ok(Some(route))
        })
        .await
    }

    /// 在实际尝试前再次确认当前分组授权。
    pub async fn check_group_access(
        &self,
        owner: UserId,
        group: GroupId,
    ) -> Result<bool, UserRouteError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let default = lock_owner(&txn, owner).await?;
            let result = group_access(&txn, owner, default, group).await?;
            txn.commit().await.map_err(internal)?;
            Ok(result)
        })
        .await
    }

    /// 尝试前重新核对当前路由启停、所有权、成员关系及分组授权。
    pub async fn check_route_member(
        &self,
        owner: UserId,
        route_id: i64,
        group: GroupId,
    ) -> Result<bool, UserRouteError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let default = lock_owner(&txn, owner).await?;
            let route = match lock_route(&txn, route_id).await {
                Err(UserRouteError::Unavailable) => return Ok(false),
                Ok(route) => route,
                Err(error) => return Err(error),
            };
            if route.owner_user_id != owner.get() {
                return Ok(false);
            }
            if !route.enabled {
                return Ok(false);
            }
            let mut member_query = user_route_members::Entity::find_by_id((route_id, group.get()));
            if txn.get_database_backend() != DbBackend::Sqlite {
                member_query = member_query.lock(LockType::Update);
            }
            if member_query.one(&txn).await.map_err(internal)?.is_none() {
                return Ok(false);
            }
            let allowed = group_access(&txn, owner, default, group).await?;
            txn.commit().await.map_err(internal)?;
            Ok(allowed)
        })
        .await
    }

    async fn bounded<T>(
        &self,
        operation: impl std::future::Future<Output = Result<T, UserRouteError>>,
    ) -> Result<T, UserRouteError> {
        timeout(
            self.lookup_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| UserRouteError::Internal)?
    }
}

/// Key 签发必须在同一事务内持有用户及路由锁，并检查全部成员。
pub(crate) async fn validate_token_route<C: ConnectionTrait>(
    connection: &C,
    owner: UserId,
    default_group: GroupId,
    id: i64,
) -> Result<(), UserRouteError> {
    let row = lock_route(connection, id).await?;
    if row.owner_user_id != owner.get() {
        return Err(UserRouteError::Unavailable);
    }
    if !row.enabled {
        return Err(UserRouteError::Unavailable);
    }
    let route = snapshot_locked(connection, row).await?;
    let members = route
        .members
        .iter()
        .map(|member| UserRouteMemberWrite {
            group_id: member.group_id,
            priority: member.priority,
            weight: member.weight,
        })
        .collect::<Vec<_>>();
    validate_member_access(connection, owner, default_group, &members).await
}

async fn lock_owner(
    connection: &DatabaseTransaction,
    owner: UserId,
) -> Result<GroupId, UserRouteError> {
    let row = lock_non_deleted_owner(connection, owner)
        .await
        .map_err(|_| UserRouteError::Internal)?
        .ok_or(UserRouteError::InvalidSession)?;
    if row.status() != 1 {
        return Err(UserRouteError::InvalidSession);
    }
    Ok(row.default_group_id())
}

async fn ensure_owner<C: ConnectionTrait>(
    connection: &C,
    owner: UserId,
) -> Result<users::Model, UserRouteError> {
    users::Entity::find_by_id(owner.get())
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::Status.eq(1_i16))
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(UserRouteError::InvalidSession)
}

async fn owned_route<C: ConnectionTrait>(
    connection: &C,
    owner: UserId,
    id: i64,
) -> Result<user_routes::Model, UserRouteError> {
    user_routes::Entity::find_by_id(id)
        .filter(user_routes::Column::OwnerUserId.eq(owner.get()))
        .filter(user_routes::Column::DeletedAt.is_null())
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(UserRouteError::NotFound)
}

async fn lock_route<C: ConnectionTrait>(
    connection: &C,
    id: i64,
) -> Result<user_routes::Model, UserRouteError> {
    if connection.get_database_backend() == DbBackend::Sqlite {
        user_routes::Entity::update_many()
            .filter(user_routes::Column::Id.eq(id))
            .col_expr(
                user_routes::Column::UpdatedAt,
                Expr::col(user_routes::Column::UpdatedAt).into(),
            )
            .exec(connection)
            .await
            .map_err(internal)?;
    }
    let mut query =
        user_routes::Entity::find_by_id(id).filter(user_routes::Column::DeletedAt.is_null());
    if connection.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(UserRouteError::Unavailable)
}

async fn group_access<C: ConnectionTrait>(
    connection: &C,
    owner: UserId,
    default: GroupId,
    group: GroupId,
) -> Result<bool, UserRouteError> {
    user_can_select_key_group(connection, owner, default, group)
        .await
        .map_err(internal)
}

async fn validate_member_access<C: ConnectionTrait>(
    connection: &C,
    owner: UserId,
    default: GroupId,
    members: &[UserRouteMemberWrite],
) -> Result<(), UserRouteError> {
    // 一致的分组锁顺序，避免两条路由以相反顺序保存时产生死锁。
    let groups = members
        .iter()
        .map(|member| member.group_id)
        .collect::<BTreeSet<_>>();
    for group in groups {
        if !group_access(connection, owner, default, group).await? {
            return Err(UserRouteError::Unavailable);
        }
    }
    Ok(())
}

async fn sync_members(
    connection: &DatabaseTransaction,
    id: i64,
    members: &[UserRouteMemberWrite],
) -> Result<(), UserRouteError> {
    let existing = user_route_members::Entity::find()
        .filter(user_route_members::Column::RouteId.eq(id))
        .all(connection)
        .await
        .map_err(internal)?;
    let desired = members
        .iter()
        .map(|member| member.group_id.get())
        .collect::<BTreeSet<_>>();
    for row in &existing {
        if !desired.contains(&row.group_id) {
            user_route_members::Entity::delete_by_id((id, row.group_id))
                .exec(connection)
                .await
                .map_err(internal)?;
        }
    }
    for member in members {
        let row = user_route_members::ActiveModel {
            route_id: Set(id),
            group_id: Set(member.group_id.get()),
            priority: Set(member.priority),
            weight: Set(member.weight as i32),
            last_selected_at_millis: Set(None),
        };
        if existing
            .iter()
            .any(|row| row.group_id == member.group_id.get())
        {
            user_route_members::Entity::update_many()
                .filter(user_route_members::Column::RouteId.eq(id))
                .filter(user_route_members::Column::GroupId.eq(member.group_id.get()))
                .col_expr(
                    user_route_members::Column::Priority,
                    Expr::value(member.priority),
                )
                .col_expr(
                    user_route_members::Column::Weight,
                    Expr::value(member.weight as i32),
                )
                .exec(connection)
                .await
                .map_err(internal)?;
        } else {
            row.insert(connection).await.map_err(internal)?;
        }
    }
    Ok(())
}

async fn snapshot<C: ConnectionTrait>(
    connection: &C,
    row: user_routes::Model,
) -> Result<UserRouteRecord, UserRouteError> {
    snapshot_with_member_lock(connection, row, false).await
}

async fn snapshot_locked<C: ConnectionTrait>(
    connection: &C,
    row: user_routes::Model,
) -> Result<UserRouteRecord, UserRouteError> {
    snapshot_with_member_lock(connection, row, true).await
}

async fn snapshot_with_member_lock<C: ConnectionTrait>(
    connection: &C,
    row: user_routes::Model,
    lock_members: bool,
) -> Result<UserRouteRecord, UserRouteError> {
    let mut query = user_route_members::Entity::find()
        .filter(user_route_members::Column::RouteId.eq(row.id))
        .order_by_asc(user_route_members::Column::Priority)
        .order_by_asc(user_route_members::Column::GroupId)
        .limit((MAX_USER_ROUTE_MEMBERS + 1) as u64);
    if lock_members && connection.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let members = query.all(connection).await.map_err(internal)?;
    let members = members
        .into_iter()
        .map(|member| {
            Ok(UserRouteMemberRecord {
                group_id: GroupId::new(member.group_id).map_err(|_| UserRouteError::Internal)?,
                priority: member.priority,
                weight: u32::try_from(member.weight).map_err(|_| UserRouteError::Internal)?,
                last_selected_at_millis: member.last_selected_at_millis,
            })
        })
        .collect::<Result<Vec<_>, UserRouteError>>()?;
    let strategy = UserRouteStrategy::parse(&row.strategy).ok_or(UserRouteError::Internal)?;
    let write = UserRouteWrite {
        name: row.name.clone(),
        strategy,
        enabled: row.enabled,
        max_group_attempts: u32::try_from(row.max_group_attempts)
            .map_err(|_| UserRouteError::Internal)?,
        max_price_ratio_micros: row.max_price_ratio_micros,
        members: members
            .iter()
            .map(|member| UserRouteMemberWrite {
                group_id: member.group_id,
                priority: member.priority,
                weight: member.weight,
            })
            .collect(),
    };
    write.validate().map_err(|_| UserRouteError::Internal)?;
    Ok(UserRouteRecord {
        id: row.id,
        owner_user_id: UserId::new(row.owner_user_id).map_err(|_| UserRouteError::Internal)?,
        name: row.name,
        strategy,
        enabled: row.enabled,
        max_group_attempts: write.max_group_attempts,
        max_price_ratio_micros: row.max_price_ratio_micros,
        members,
        created_at: row.created_at.unix_timestamp(),
        updated_at: row.updated_at.unix_timestamp(),
    })
}

fn internal(_: sea_orm::DbErr) -> UserRouteError {
    UserRouteError::Internal
}
