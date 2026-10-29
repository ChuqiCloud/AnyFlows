//! 个人 Key 自选分组策略，与用户默认组和网关运行时访问规则分别维护。

use std::{collections::BTreeSet, time::Duration};

use af_domain::{GroupId, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbBackend, DbErr, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, LockType, OnConflict, Query},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{group_key_policies, groups, user_group_grants, users},
};

/// 一次策略写入允许指定的最大用户数。
pub const MAX_GROUP_KEY_ALLOWED_USERS: usize = 1_000;
/// 一页用户可选分组的最大数量。
pub const MAX_USER_TOKEN_GROUP_PAGE_SIZE: usize = 100;

/// 个人 Key 的分组自选范围。缺失策略记录等同于仅默认组。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GroupKeySelectionMode {
    /// 仅该组为默认组的用户可选。
    DefaultOnly,
    /// 所有有效登录用户可选。
    AllUsers,
    /// 默认组用户及显式授权用户可选。
    SelectedUsers,
}

impl GroupKeySelectionMode {
    /// 数据库存储使用的稳定编码。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DefaultOnly => "default_only",
            Self::AllUsers => "all_users",
            Self::SelectedUsers => "selected_users",
        }
    }

    fn parse(value: &str) -> Result<Self, DbErr> {
        match value {
            "default_only" => Ok(Self::DefaultOnly),
            "all_users" => Ok(Self::AllUsers),
            "selected_users" => Ok(Self::SelectedUsers),
            _ => Err(DbErr::Custom("分组 Key 策略状态无效".to_owned())),
        }
    }
}

/// 管理端可读取的策略与规范化授权列表。
#[derive(Debug)]
pub struct GroupKeyPolicyRecord {
    /// 分组主键。
    pub group_id: GroupId,
    /// 自选范围。
    pub selection_mode: GroupKeySelectionMode,
    /// 升序且不重复的用户授权。
    pub allowed_user_ids: Vec<UserId>,
}

/// 用户端可公开的最小分组信息。
#[derive(Debug)]
pub struct UserTokenGroupRecord {
    /// 分组主键。
    pub group_id: GroupId,
    /// 公开显示名称。
    pub display_name: String,
    /// 基础倍率，百万分比整数。
    pub ratio_micros: i64,
    /// 是否为该用户当前默认组。
    pub is_default: bool,
}

/// 按分组 ID 游标分页的自选分组。
#[derive(Debug)]
pub struct UserTokenGroupPageRecord {
    /// 一页可选分组。
    pub groups: Vec<UserTokenGroupRecord>,
    /// 下一页游标。
    pub next_cursor: Option<GroupId>,
}

/// 不暴露数据库诊断的分组选择错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GroupKeyAccessError {
    /// 输入超出边界或用户授权引用无效。
    #[error("分组 Key 策略输入无效")]
    InvalidInput,
    /// 分组不存在或已删除。
    #[error("分组不存在")]
    NotFound,
    /// 用户不存在、停用或已删除。
    #[error("用户不可用")]
    OwnerUnavailable,
    /// 截止时间或数据库操作失败。
    #[error("分组 Key 策略内部失败")]
    Internal,
}

/// 独立管理个人 Key 分组策略的数据库仓储。
#[derive(Clone)]
pub struct GroupKeyAccessRepository {
    pool: DatabasePool,
    lookup_timeout: Duration,
}

impl GroupKeyAccessRepository {
    /// 构造有界数据库访问；拒绝零截止时间。
    pub fn new(pool: DatabasePool, lookup_timeout: Duration) -> Result<Self, GroupKeyAccessError> {
        if lookup_timeout.is_zero() {
            return Err(GroupKeyAccessError::InvalidInput);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 读取当前未删除分组策略；缺失记录返回默认策略。
    pub async fn get_policy(
        &self,
        group_id: GroupId,
    ) -> Result<GroupKeyPolicyRecord, GroupKeyAccessError> {
        self.bounded(async {
            let transaction = self.pool.connection().begin().await.map_err(internal)?;
            if !lock_active_group(&transaction, group_id)
                .await
                .map_err(internal)?
            {
                return Err(GroupKeyAccessError::NotFound);
            }
            let result = read_policy(&transaction, group_id)
                .await
                .map_err(internal)?;
            transaction.commit().await.map_err(internal)?;
            Ok(result)
        })
        .await
    }

    /// 原子替换策略与用户授权；与个人 Key 创建共享分组行串行点。
    pub async fn update_policy(
        &self,
        group_id: GroupId,
        selection_mode: GroupKeySelectionMode,
        allowed_user_ids: Vec<UserId>,
    ) -> Result<GroupKeyPolicyRecord, GroupKeyAccessError> {
        if allowed_user_ids.len() > MAX_GROUP_KEY_ALLOWED_USERS
            || (selection_mode != GroupKeySelectionMode::SelectedUsers
                && !allowed_user_ids.is_empty())
        {
            return Err(GroupKeyAccessError::InvalidInput);
        }
        let allowed_user_ids = allowed_user_ids
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        self.bounded(async {
            let transaction = self.pool.connection().begin().await.map_err(internal)?;
            // users -> group，与 Key 签发一致。外键插入也可能隐式锁用户行。
            for user_id in &allowed_user_ids {
                if transaction.get_database_backend() == DbBackend::Sqlite {
                    users::Entity::update_many()
                        .filter(users::Column::Id.eq(user_id.get()))
                        .filter(users::Column::DeletedAt.is_null())
                        .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
                        .exec(&transaction)
                        .await
                        .map_err(internal)?;
                }
                let mut query = users::Entity::find()
                    .select_only()
                    .column(users::Column::Id)
                    .filter(users::Column::Id.eq(user_id.get()))
                    .filter(users::Column::DeletedAt.is_null());
                if transaction.get_database_backend() != DbBackend::Sqlite {
                    query = query.lock(LockType::Update);
                }
                if query
                    .into_tuple::<i64>()
                    .one(&transaction)
                    .await
                    .map_err(internal)?
                    .is_none()
                {
                    return Err(GroupKeyAccessError::InvalidInput);
                }
            }
            if !lock_active_group(&transaction, group_id)
                .await
                .map_err(internal)?
            {
                return Err(GroupKeyAccessError::NotFound);
            }
            group_key_policies::Entity::insert(group_key_policies::ActiveModel {
                group_id: Set(group_id.get()),
                selection_mode: Set(selection_mode.as_str().to_owned()),
            })
            .on_conflict(
                OnConflict::column(group_key_policies::Column::GroupId)
                    .update_column(group_key_policies::Column::SelectionMode)
                    .to_owned(),
            )
            .exec_without_returning(&transaction)
            .await
            .map_err(internal)?;
            user_group_grants::Entity::delete_many()
                .filter(user_group_grants::Column::GroupId.eq(group_id.get()))
                .exec(&transaction)
                .await
                .map_err(internal)?;
            // 逐条插入避免 SQLite 绑定参数上限小于最大授权列表的两倍。
            for user_id in &allowed_user_ids {
                user_group_grants::ActiveModel {
                    group_id: Set(group_id.get()),
                    user_id: Set(user_id.get()),
                }
                .insert(&transaction)
                .await
                .map_err(internal)?;
            }
            transaction.commit().await.map_err(internal)?;
            Ok(GroupKeyPolicyRecord {
                group_id,
                selection_mode,
                allowed_user_ids,
            })
        })
        .await
    }

    /// 仅返回当前用户可用于创建个人 Key 的公开分组字段。
    pub async fn list_user_groups(
        &self,
        user_id: UserId,
        after: Option<GroupId>,
        limit: usize,
    ) -> Result<UserTokenGroupPageRecord, GroupKeyAccessError> {
        if !(1..=MAX_USER_TOKEN_GROUP_PAGE_SIZE).contains(&limit) {
            return Err(GroupKeyAccessError::InvalidInput);
        }
        self.bounded(async {
            let owner = users::Entity::find_by_id(user_id.get())
                .filter(users::Column::DeletedAt.is_null())
                .filter(users::Column::Status.eq(1_i16))
                .one(self.pool.connection())
                .await
                .map_err(internal)?
                .ok_or(GroupKeyAccessError::OwnerUnavailable)?;
            let public = Query::select()
                .column(group_key_policies::Column::GroupId)
                .from(group_key_policies::Entity)
                .and_where(Expr::col(group_key_policies::Column::SelectionMode).eq("all_users"))
                .to_owned();
            let selected = Query::select()
                .column((
                    user_group_grants::Entity,
                    user_group_grants::Column::GroupId,
                ))
                .from(user_group_grants::Entity)
                .inner_join(
                    group_key_policies::Entity,
                    Expr::col((
                        user_group_grants::Entity,
                        user_group_grants::Column::GroupId,
                    ))
                    .equals((
                        group_key_policies::Entity,
                        group_key_policies::Column::GroupId,
                    )),
                )
                .and_where(
                    Expr::col((user_group_grants::Entity, user_group_grants::Column::UserId))
                        .eq(user_id.get()),
                )
                .and_where(
                    Expr::col((
                        group_key_policies::Entity,
                        group_key_policies::Column::SelectionMode,
                    ))
                    .eq("selected_users"),
                )
                .to_owned();
            let mut query = groups::Entity::find()
                .select_only()
                .columns([
                    groups::Column::Id,
                    groups::Column::DisplayName,
                    groups::Column::RatioMicros,
                ])
                .filter(groups::Column::DeletedAt.is_null())
                .filter(
                    Condition::any()
                        .add(groups::Column::Id.eq(owner.default_group_id))
                        .add(Expr::col(groups::Column::Id).in_subquery(public))
                        .add(Expr::col(groups::Column::Id).in_subquery(selected)),
                )
                .order_by_asc(groups::Column::Id)
                .limit((limit + 1) as u64);
            if let Some(after) = after {
                query = query.filter(groups::Column::Id.gt(after.get()));
            }
            let mut rows = query
                .into_tuple::<(i64, String, i64)>()
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            let has_more = rows.len() > limit;
            rows.truncate(limit);
            let groups = rows
                .into_iter()
                .map(|(id, display_name, ratio_micros)| {
                    if ratio_micros < 0
                        || display_name.is_empty()
                        || display_name.len() > 128
                        || display_name.chars().any(char::is_control)
                    {
                        return Err(GroupKeyAccessError::Internal);
                    }
                    Ok(UserTokenGroupRecord {
                        group_id: GroupId::new(id).map_err(|_| GroupKeyAccessError::Internal)?,
                        display_name,
                        ratio_micros,
                        is_default: id == owner.default_group_id,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = if has_more {
                groups.last().map(|group| group.group_id)
            } else {
                None
            };
            Ok(UserTokenGroupPageRecord {
                groups,
                next_cursor,
            })
        })
        .await
    }

    async fn bounded<T>(
        &self,
        operation: impl std::future::Future<Output = Result<T, GroupKeyAccessError>>,
    ) -> Result<T, GroupKeyAccessError> {
        timeout(
            self.lookup_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| GroupKeyAccessError::Internal)?
    }
}

/// 必须在调用方事务中使用；先锁目标分组，再读取当前策略与授权。
pub(crate) async fn user_can_select_key_group<C: ConnectionTrait>(
    connection: &C,
    user_id: UserId,
    default_group_id: GroupId,
    group_id: GroupId,
) -> Result<bool, DbErr> {
    if !lock_active_group(connection, group_id).await? {
        return Ok(false);
    }
    if group_id == default_group_id {
        return Ok(true);
    }
    let mut query = group_key_policies::Entity::find_by_id(group_id.get());
    if connection.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let mode = query
        .one(connection)
        .await?
        .map(|policy| GroupKeySelectionMode::parse(&policy.selection_mode))
        .transpose()?
        .unwrap_or(GroupKeySelectionMode::DefaultOnly);
    match mode {
        GroupKeySelectionMode::DefaultOnly => Ok(false),
        GroupKeySelectionMode::AllUsers => Ok(true),
        GroupKeySelectionMode::SelectedUsers => {
            let mut query = user_group_grants::Entity::find_by_id((group_id.get(), user_id.get()));
            if connection.get_database_backend() != DbBackend::Sqlite {
                query = query.lock(LockType::Update);
            }
            Ok(query.one(connection).await?.is_some())
        }
    }
}

async fn lock_active_group<C: ConnectionTrait>(
    connection: &C,
    group_id: GroupId,
) -> Result<bool, DbErr> {
    if connection.get_database_backend() == DbBackend::Sqlite {
        groups::Entity::update_many()
            .filter(groups::Column::Id.eq(group_id.get()))
            .filter(groups::Column::DeletedAt.is_null())
            .col_expr(
                groups::Column::RatioMicros,
                Expr::col(groups::Column::RatioMicros).into(),
            )
            .exec(connection)
            .await?;
    }
    let mut query = groups::Entity::find_by_id(group_id.get())
        .select_only()
        .column(groups::Column::Id)
        .filter(groups::Column::DeletedAt.is_null());
    if connection.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    Ok(query.into_tuple::<i64>().one(connection).await?.is_some())
}

async fn read_policy<C: ConnectionTrait>(
    connection: &C,
    group_id: GroupId,
) -> Result<GroupKeyPolicyRecord, DbErr> {
    let policy = group_key_policies::Entity::find_by_id(group_id.get())
        .one(connection)
        .await?;
    let selection_mode = policy
        .map(|policy| GroupKeySelectionMode::parse(&policy.selection_mode))
        .transpose()?
        .unwrap_or(GroupKeySelectionMode::DefaultOnly);
    let ids = if selection_mode == GroupKeySelectionMode::SelectedUsers {
        user_group_grants::Entity::find()
            .select_only()
            .column(user_group_grants::Column::UserId)
            .filter(user_group_grants::Column::GroupId.eq(group_id.get()))
            .order_by_asc(user_group_grants::Column::UserId)
            .limit((MAX_GROUP_KEY_ALLOWED_USERS + 1) as u64)
            .into_tuple::<i64>()
            .all(connection)
            .await?
    } else {
        Vec::new()
    };
    if ids.len() > MAX_GROUP_KEY_ALLOWED_USERS {
        return Err(DbErr::Custom("分组授权用户数超出上限".to_owned()));
    }
    let allowed_user_ids = ids
        .into_iter()
        .map(|id| UserId::new(id).map_err(|_| DbErr::Custom("分组授权用户状态无效".to_owned())))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GroupKeyPolicyRecord {
        group_id,
        selection_mode,
        allowed_user_ids,
    })
}

fn internal(_: DbErr) -> GroupKeyAccessError {
    GroupKeyAccessError::Internal
}
