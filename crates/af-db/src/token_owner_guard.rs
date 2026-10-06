use af_domain::{GroupId, UserId};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait, PaginatorTrait,
    QueryFilter, QuerySelect,
    sea_query::{Expr, LockType},
};

use crate::entity::{tokens, users};

/// 已锁定的未软删除用户快照，仅暴露令牌容量校验需要的字段。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LockedTokenOwner {
    status: i16,
    default_group_id: GroupId,
    concurrency: Option<i32>,
    rpm_limit: Option<i32>,
}

impl LockedTokenOwner {
    /// 返回用户当前状态，调用方按自身权限语义决定是否允许继续。
    pub(crate) const fn status(self) -> i16 {
        self.status
    }

    /// 返回数据库中的默认分组。
    pub(crate) const fn default_group_id(self) -> GroupId {
        self.default_group_id
    }

    /// 返回用户级并发限制原始值，由认证边界统一校验。
    pub(crate) const fn concurrency(self) -> Option<i32> {
        self.concurrency
    }

    /// 返回用户级 RPM 限制原始值，由认证边界统一校验。
    pub(crate) const fn rpm_limit(self) -> Option<i32> {
        self.rpm_limit
    }
}

/// 用户行锁与令牌计数的内部错误分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TokenOwnerGuardError {
    Query,
    Invariant,
}

/// 锁定未软删除用户行，保证所有令牌签发入口共享同一容量串行点。
pub(crate) async fn lock_non_deleted_owner(
    transaction: &DatabaseTransaction,
    owner_user_id: UserId,
) -> Result<Option<LockedTokenOwner>, TokenOwnerGuardError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 不支持 FOR UPDATE，以无变化 UPDATE 先取得数据库写锁。
        let result = users::Entity::update_many()
            .filter(users::Column::Id.eq(owner_user_id.get()))
            .filter(users::Column::DeletedAt.is_null())
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
            .exec(transaction)
            .await
            .map_err(|_| TokenOwnerGuardError::Query)?;
        if result.rows_affected != 1 {
            return Ok(None);
        }
    }

    let mut query = users::Entity::find()
        .select_only()
        .column(users::Column::Status)
        .column(users::Column::DefaultGroupId)
        .column(users::Column::Concurrency)
        .column(users::Column::RpmLimit)
        .filter(users::Column::Id.eq(owner_user_id.get()))
        .filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let Some((status, default_group_id, concurrency, rpm_limit)) = query
        .into_tuple::<(i16, i64, Option<i32>, Option<i32>)>()
        .one(transaction)
        .await
        .map_err(|_| TokenOwnerGuardError::Query)?
    else {
        return Ok(None);
    };
    Ok(Some(LockedTokenOwner {
        status,
        default_group_id: GroupId::new(default_group_id)
            .map_err(|_| TokenOwnerGuardError::Invariant)?,
        concurrency,
        rpm_limit,
    }))
}

/// 统计用户当前所有未软删除 Key；停用和过期 Key 仍占用容量。
pub(crate) async fn non_deleted_token_count(
    transaction: &DatabaseTransaction,
    owner_user_id: UserId,
) -> Result<u64, TokenOwnerGuardError> {
    tokens::Entity::find()
        .filter(tokens::Column::UserId.eq(owner_user_id.get()))
        .filter(tokens::Column::OrganizationId.is_null())
        .filter(tokens::Column::Name.ne(af_domain::PLAYGROUND_TOKEN_NAME))
        .filter(tokens::Column::DeletedAt.is_null())
        .count(transaction)
        .await
        .map_err(|_| TokenOwnerGuardError::Query)
}
