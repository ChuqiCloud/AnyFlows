use std::fmt;

use af_domain::{GroupId, UserId};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseTransaction, DbErr, EntityTrait, QueryFilter, Set,
    TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Condition, Expr, Order, Query, SelectStatement},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use zeroize::Zeroizing;

use crate::{
    AdminUserRecord, AdminUserRepository, AdminUserRepositoryError,
    admin_user::{detail_query, record_internal_error},
    entity::{groups, tokens, users},
    identity_secret::{IdentitySecretError, generate_aff_code, hash_password},
};

/// 管理员创建用户时允许写入的基础账户字段。
pub struct AdminUserCreateRecord {
    username: String,
    email: Option<String>,
    password: Option<Zeroizing<String>>,
    role: i16,
    status: i16,
    default_group_id: GroupId,
    quota: i64,
    rpm_limit: Option<i32>,
    concurrency: Option<i32>,
}

impl AdminUserCreateRecord {
    /// 组装已经由上层完成语义校验的用户创建记录。
    #[allow(clippy::too_many_arguments, reason = "字段与管理端写入契约一一对应")]
    #[must_use]
    pub fn new(
        username: String,
        email: Option<String>,
        password: Option<String>,
        role: i16,
        status: i16,
        default_group_id: GroupId,
        quota: i64,
        rpm_limit: Option<i32>,
        concurrency: Option<i32>,
    ) -> Self {
        Self {
            username,
            email,
            password: password.map(Zeroizing::new),
            role,
            status,
            default_group_id,
            quota,
            rpm_limit,
            concurrency,
        }
    }
}

impl fmt::Debug for AdminUserCreateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUserCreateRecord(<redacted>)")
    }
}

/// 管理员完整更新用户基础字段时使用的记录；密码缺省表示保持原值。
pub struct AdminUserUpdateRecord {
    username: String,
    email: Option<String>,
    password: Option<Zeroizing<String>>,
    role: i16,
    status: i16,
    default_group_id: GroupId,
    quota: i64,
    rpm_limit: Option<i32>,
    concurrency: Option<i32>,
}

impl AdminUserUpdateRecord {
    /// 组装已经由上层完成语义校验的用户更新记录。
    #[allow(clippy::too_many_arguments, reason = "字段与管理端写入契约一一对应")]
    #[must_use]
    pub fn new(
        username: String,
        email: Option<String>,
        password: Option<String>,
        role: i16,
        status: i16,
        default_group_id: GroupId,
        quota: i64,
        rpm_limit: Option<i32>,
        concurrency: Option<i32>,
    ) -> Self {
        Self {
            username,
            email,
            password: password.map(Zeroizing::new),
            role,
            status,
            default_group_id,
            quota,
            rpm_limit,
            concurrency,
        }
    }
}

impl fmt::Debug for AdminUserUpdateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUserUpdateRecord(<redacted>)")
    }
}

/// 用户更新结果；不存在和已软删除统一视为未找到。
pub enum AdminUserMutationOutcome {
    /// 用户已经写入，并返回最新非敏感快照。
    Mutated(AdminUserRecord),
    /// 用户不存在或已经被软删除。
    NotFound,
}

impl fmt::Debug for AdminUserMutationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mutated(_) => {
                formatter.write_str("AdminUserMutationOutcome::Mutated(<redacted>)")
            }
            Self::NotFound => formatter.write_str("AdminUserMutationOutcome::NotFound"),
        }
    }
}

/// 用户软删除结果；重复删除不会伪装成成功。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminUserDeleteOutcome {
    /// 用户及其直接令牌依赖已经写入软删除墓碑。
    Deleted,
    /// 用户不存在或已经被软删除。
    NotFound,
}

impl AdminUserRepository {
    /// 创建一个未软删除用户并返回非敏感管理快照。
    pub async fn create(
        &self,
        record: AdminUserCreateRecord,
    ) -> Result<AdminUserRecord, AdminUserRepositoryError> {
        match timeout(self.lookup_timeout, self.create_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminUserRepositoryError::Timeout)),
        }
    }

    /// 完整更新一个未软删除用户的基础字段并返回非敏感管理快照。
    pub async fn update(
        &self,
        user_id: UserId,
        record: AdminUserUpdateRecord,
    ) -> Result<AdminUserMutationOutcome, AdminUserRepositoryError> {
        match timeout(self.lookup_timeout, self.update_inner(user_id, record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminUserRepositoryError::Timeout)),
        }
    }

    /// 软删除用户，并在同一事务内软删除该用户已有令牌。
    pub async fn delete(
        &self,
        user_id: UserId,
    ) -> Result<AdminUserDeleteOutcome, AdminUserRepositoryError> {
        match timeout(self.lookup_timeout, self.delete_inner(user_id)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminUserRepositoryError::Timeout)),
        }
    }

    async fn create_inner(
        &self,
        record: AdminUserCreateRecord,
    ) -> Result<AdminUserRecord, AdminUserRepositoryError> {
        let transaction = begin_transaction(self).await?;
        ensure_group_exists(&transaction, record.default_group_id).await?;
        ensure_identity_available(
            &transaction,
            &record.username,
            record.email.as_deref(),
            None,
        )
        .await?;
        let password_hash = record
            .password
            .as_ref()
            .map(hash_password)
            .transpose()
            .map_err(map_identity_secret_error)?;
        let aff_code = generate_aff_code().map_err(map_identity_secret_error)?;
        let inserted = users::ActiveModel {
            username: Set(record.username),
            email: Set(record.email),
            password_hash: Set(password_hash),
            role: Set(record.role),
            status: Set(record.status),
            default_group_id: Set(record.default_group_id.get()),
            quota: Set(record.quota),
            aff_code: Set(aff_code),
            rpm_limit: Set(record.rpm_limit),
            concurrency: Set(record.concurrency),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
        let user_id = UserId::new(inserted.id)
            .map_err(|_| record_internal_error(AdminUserRepositoryError::Invariant))?;
        let snapshot = fetch_user_snapshot(&transaction, user_id).await?;
        commit_transaction(transaction).await?;
        Ok(snapshot)
    }

    async fn update_inner(
        &self,
        user_id: UserId,
        record: AdminUserUpdateRecord,
    ) -> Result<AdminUserMutationOutcome, AdminUserRepositoryError> {
        let transaction = begin_transaction(self).await?;
        if !active_user_exists(&transaction, user_id).await? {
            return Ok(AdminUserMutationOutcome::NotFound);
        }
        ensure_group_exists(&transaction, record.default_group_id).await?;
        ensure_identity_available(
            &transaction,
            &record.username,
            record.email.as_deref(),
            Some(user_id),
        )
        .await?;

        let now = TimeDateTimeWithTimeZone::now_utc();
        let mut update = users::Entity::update_many()
            .filter(Expr::col((users::Entity, users::Column::Id)).eq(user_id.get()))
            .filter(Expr::col((users::Entity, users::Column::DeletedAt)).is_null())
            .col_expr(users::Column::Username, Expr::value(record.username))
            .col_expr(users::Column::Email, Expr::value(record.email))
            .col_expr(users::Column::Role, Expr::value(record.role))
            .col_expr(users::Column::Status, Expr::value(record.status))
            .col_expr(
                users::Column::DefaultGroupId,
                Expr::value(record.default_group_id.get()),
            )
            .col_expr(users::Column::Quota, Expr::value(record.quota))
            .col_expr(users::Column::RpmLimit, Expr::value(record.rpm_limit))
            .col_expr(users::Column::Concurrency, Expr::value(record.concurrency))
            .col_expr(users::Column::UpdatedAt, Expr::value(now));
        if let Some(password) = record.password.as_ref() {
            update = update.col_expr(
                users::Column::PasswordHash,
                Expr::value(Some(
                    hash_password(password).map_err(map_identity_secret_error)?,
                )),
            );
        }
        let result = update
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        if result.rows_affected != 1 {
            return Err(record_internal_error(AdminUserRepositoryError::Invariant));
        }
        let snapshot = fetch_user_snapshot(&transaction, user_id).await?;
        commit_transaction(transaction).await?;
        Ok(AdminUserMutationOutcome::Mutated(snapshot))
    }

    async fn delete_inner(
        &self,
        user_id: UserId,
    ) -> Result<AdminUserDeleteOutcome, AdminUserRepositoryError> {
        let transaction = begin_transaction(self).await?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let user_result = users::Entity::update_many()
            .filter(Expr::col((users::Entity, users::Column::Id)).eq(user_id.get()))
            .filter(Expr::col((users::Entity, users::Column::DeletedAt)).is_null())
            .col_expr(users::Column::DeletedAt, Expr::value(now))
            .col_expr(users::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        if user_result.rows_affected == 0 {
            return Ok(AdminUserDeleteOutcome::NotFound);
        }
        if user_result.rows_affected != 1 {
            return Err(record_internal_error(AdminUserRepositoryError::Invariant));
        }

        tokens::Entity::update_many()
            .filter(Expr::col((tokens::Entity, tokens::Column::UserId)).eq(user_id.get()))
            .filter(Expr::col((tokens::Entity, tokens::Column::DeletedAt)).is_null())
            .col_expr(tokens::Column::DeletedAt, Expr::value(now))
            .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        commit_transaction(transaction).await?;
        Ok(AdminUserDeleteOutcome::Deleted)
    }
}

async fn begin_transaction(
    repository: &AdminUserRepository,
) -> Result<DatabaseTransaction, AdminUserRepositoryError> {
    repository
        .pool
        .connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminUserRepositoryError::Query))
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), AdminUserRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminUserRepositoryError::Query))
}

async fn fetch_user_snapshot(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<AdminUserRecord, AdminUserRepositoryError> {
    let statement = transaction
        .get_database_backend()
        .build(&detail_query(user_id));
    let mut results = transaction
        .query_all(statement)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminUserRepositoryError::Query))?;
    match results.len() {
        1 => AdminUserRecord::try_from_query_result(
            &results
                .pop()
                .ok_or_else(|| record_internal_error(AdminUserRepositoryError::Invariant))?,
        ),
        _ => Err(record_internal_error(AdminUserRepositoryError::Invariant)),
    }
}

async fn active_user_exists(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<bool, AdminUserRepositoryError> {
    query_exists(transaction, active_user_exists_query(user_id)).await
}

async fn ensure_group_exists(
    transaction: &DatabaseTransaction,
    group_id: GroupId,
) -> Result<(), AdminUserRepositoryError> {
    if query_exists(transaction, active_group_exists_query(group_id)).await? {
        Ok(())
    } else {
        Err(AdminUserRepositoryError::InvalidReference)
    }
}

async fn ensure_identity_available(
    transaction: &DatabaseTransaction,
    username: &str,
    email: Option<&str>,
    except_user_id: Option<UserId>,
) -> Result<(), AdminUserRepositoryError> {
    if query_exists(
        transaction,
        identity_conflict_query(username, email, except_user_id),
    )
    .await?
    {
        Err(AdminUserRepositoryError::Conflict)
    } else {
        Ok(())
    }
}

async fn query_exists(
    transaction: &DatabaseTransaction,
    query: SelectStatement,
) -> Result<bool, AdminUserRepositoryError> {
    let statement = transaction.get_database_backend().build(&query);
    let results = transaction
        .query_all(statement)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminUserRepositoryError::Query))?;
    Ok(!results.is_empty())
}

fn active_user_exists_query(user_id: UserId) -> SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((users::Entity, users::Column::Id)),
            Alias::new("id"),
        )
        .from(users::Entity)
        .and_where(Expr::col((users::Entity, users::Column::Id)).eq(user_id.get()))
        .and_where(Expr::col((users::Entity, users::Column::DeletedAt)).is_null())
        .limit(1)
        .to_owned()
}

fn active_group_exists_query(group_id: GroupId) -> SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((groups::Entity, groups::Column::Id)),
            Alias::new("id"),
        )
        .from(groups::Entity)
        .and_where(Expr::col((groups::Entity, groups::Column::Id)).eq(group_id.get()))
        .and_where(Expr::col((groups::Entity, groups::Column::DeletedAt)).is_null())
        .limit(1)
        .to_owned()
}

fn identity_conflict_query(
    username: &str,
    email: Option<&str>,
    except_user_id: Option<UserId>,
) -> SelectStatement {
    let mut identity =
        Condition::any().add(Expr::col((users::Entity, users::Column::Username)).eq(username));
    if let Some(email) = email {
        identity = identity.add(Expr::col((users::Entity, users::Column::Email)).eq(email));
    }
    let mut query = Query::select();
    query
        .expr_as(
            Expr::col((users::Entity, users::Column::Id)),
            Alias::new("id"),
        )
        .from(users::Entity)
        .and_where(identity.into())
        .and_where(Expr::col((users::Entity, users::Column::DeletedAt)).is_null())
        .order_by((users::Entity, users::Column::Id), Order::Asc)
        .limit(1);
    if let Some(user_id) = except_user_id {
        query.and_where(Expr::col((users::Entity, users::Column::Id)).ne(user_id.get()));
    }
    query.to_owned()
}

fn map_identity_secret_error(error: IdentitySecretError) -> AdminUserRepositoryError {
    match error {
        IdentitySecretError::Entropy => AdminUserRepositoryError::Entropy,
        IdentitySecretError::InvalidHash => {
            record_internal_error(AdminUserRepositoryError::Invariant)
        }
    }
}

fn map_write_db_error(error: DbErr) -> AdminUserRepositoryError {
    let rendered = error.to_string();
    if rendered.contains("uq_users_active_username")
        || rendered.contains("uq_users_active_email")
        || rendered.contains("_active_username")
        || rendered.contains("_active_email")
        || rendered.contains("users.username")
        || rendered.contains("users.email")
    {
        return AdminUserRepositoryError::Conflict;
    }
    if rendered.contains("FOREIGN KEY") || rendered.contains("foreign key") {
        return AdminUserRepositoryError::InvalidReference;
    }
    record_internal_error(AdminUserRepositoryError::Query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_helpers_keep_sensitive_contracts_closed() {
        let create = AdminUserCreateRecord::new(
            "new-user".to_owned(),
            Some("new@example.com".to_owned()),
            Some("secret-password".to_owned()),
            0,
            1,
            GroupId::new(1).unwrap(),
            0,
            None,
            None,
        );
        let update = AdminUserUpdateRecord::new(
            "updated-user".to_owned(),
            None,
            Some("next-secret".to_owned()),
            0,
            2,
            GroupId::new(1).unwrap(),
            10,
            Some(60),
            Some(2),
        );

        assert_eq!(format!("{create:?}"), "AdminUserCreateRecord(<redacted>)");
        assert_eq!(format!("{update:?}"), "AdminUserUpdateRecord(<redacted>)");
        assert!(!format!("{create:?}").contains("secret-password"));
        assert!(!format!("{update:?}").contains("next-secret"));
    }
}
