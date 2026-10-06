use std::fmt;

use af_domain::{GroupId, Quota, UserId, WalletEventId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Condition, Expr, LockType, Order, Query, SelectStatement},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use zeroize::Zeroizing;

use crate::{
    AdminUserRecord, AdminUserRepository, AdminUserRepositoryError, AuthChallengeConsume,
    AuthChallengeConsumeOutcome, AuthChallengePurpose, AuthChallengeRepositoryError,
    InviteRebateGrant, InviteRebateGrantOutcome, InviteRebateRepositoryError,
    admin_user::{detail_query, record_internal_error},
    auth_challenge::consume_in_transaction,
    entity::{groups, tokens, users},
    identity_secret::{IdentitySecretError, generate_aff_code, hash_password},
    invite_rebate::grant_in_transaction,
};

const ENABLED_USER_STATUS: i16 = 1;
const REGISTRATION_REBATE_EVENT_NAMESPACE: [u8; 8] = *b"invreg01";

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
    inviter_id: Option<UserId>,
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
            inviter_id: None,
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

/// 邮箱挑战消费与用户创建的原子结果。
pub enum AdminUserVerifiedCreateOutcome {
    /// 挑战已消费且用户已在同一事务内创建。
    Created(AdminUserRecord),
    /// 挑战不存在、无效、过期、已消费或次数耗尽。
    Rejected,
}

/// 公开注册事务同时处理邮箱挑战、邀请码和返利后的闭合结果。
pub enum AdminUserRegistrationCreateOutcome {
    /// 用户、opening 账本及可选返利已经在同一事务内提交。
    Created(AdminUserRecord),
    /// 邮箱挑战不存在、无效、过期、已消费或次数耗尽。
    VerificationRejected,
    /// 邀请码不存在，或对应邀请人当前不可用。
    InvitationRejected,
}

impl fmt::Debug for AdminUserRegistrationCreateOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("AdminUserRegistrationCreateOutcome::Created(<redacted>)")
            }
            Self::VerificationRejected => {
                formatter.write_str("AdminUserRegistrationCreateOutcome::VerificationRejected")
            }
            Self::InvitationRejected => {
                formatter.write_str("AdminUserRegistrationCreateOutcome::InvitationRejected")
            }
        }
    }
}

impl fmt::Debug for AdminUserVerifiedCreateOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("AdminUserVerifiedCreateOutcome::Created(<redacted>)")
            }
            Self::Rejected => formatter.write_str("AdminUserVerifiedCreateOutcome::Rejected"),
        }
    }
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

    /// 在同一数据库事务内消费注册邮箱挑战并创建用户。
    pub async fn create_verified(
        &self,
        record: AdminUserCreateRecord,
        challenge: AuthChallengeConsume,
    ) -> Result<AdminUserVerifiedCreateOutcome, AdminUserRepositoryError> {
        if challenge.purpose() != AuthChallengePurpose::RegistrationEmail {
            return Err(record_internal_error(AdminUserRepositoryError::Invariant));
        }
        match timeout(
            self.lookup_timeout,
            self.create_verified_inner(record, challenge),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminUserRepositoryError::Timeout)),
        }
    }

    /// 在同一事务内消费可选邮箱挑战、解析邀请码、创建用户并发放注册返利。
    pub async fn create_registration(
        &self,
        record: AdminUserCreateRecord,
        challenge: Option<AuthChallengeConsume>,
        invite_code: Option<&str>,
        rebate_quota: Quota,
        credited_at: u64,
    ) -> Result<AdminUserRegistrationCreateOutcome, AdminUserRepositoryError> {
        if challenge
            .as_ref()
            .is_some_and(|value| value.purpose() != AuthChallengePurpose::RegistrationEmail)
        {
            return Err(record_internal_error(AdminUserRepositoryError::Invariant));
        }
        match timeout(
            self.lookup_timeout,
            self.create_registration_inner(
                record,
                challenge,
                invite_code,
                rebate_quota,
                credited_at,
            ),
        )
        .await
        {
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
        let snapshot = create_user_in_transaction(&transaction, record).await?;
        commit_transaction(transaction).await?;
        Ok(snapshot)
    }

    async fn create_verified_inner(
        &self,
        record: AdminUserCreateRecord,
        challenge: AuthChallengeConsume,
    ) -> Result<AdminUserVerifiedCreateOutcome, AdminUserRepositoryError> {
        let transaction = begin_transaction(self).await?;
        match consume_in_transaction(&transaction, &challenge)
            .await
            .map_err(map_auth_challenge_error)?
        {
            AuthChallengeConsumeOutcome::Rejected => {
                commit_transaction(transaction).await?;
                Ok(AdminUserVerifiedCreateOutcome::Rejected)
            }
            AuthChallengeConsumeOutcome::Consumed(consumption) => {
                if consumption.target_user_id().is_some() {
                    return Err(record_internal_error(AdminUserRepositoryError::Invariant));
                }
                let snapshot = create_user_in_transaction(&transaction, record).await?;
                commit_transaction(transaction).await?;
                Ok(AdminUserVerifiedCreateOutcome::Created(snapshot))
            }
        }
    }

    async fn create_registration_inner(
        &self,
        mut record: AdminUserCreateRecord,
        challenge: Option<AuthChallengeConsume>,
        invite_code: Option<&str>,
        rebate_quota: Quota,
        credited_at: u64,
    ) -> Result<AdminUserRegistrationCreateOutcome, AdminUserRepositoryError> {
        let transaction = begin_transaction(self).await?;
        if let Some(challenge) = challenge {
            match consume_in_transaction(&transaction, &challenge)
                .await
                .map_err(map_auth_challenge_error)?
            {
                AuthChallengeConsumeOutcome::Rejected => {
                    commit_transaction(transaction).await?;
                    return Ok(AdminUserRegistrationCreateOutcome::VerificationRejected);
                }
                AuthChallengeConsumeOutcome::Consumed(consumption) => {
                    if consumption.target_user_id().is_some() {
                        return Err(record_internal_error(AdminUserRepositoryError::Invariant));
                    }
                }
            }
        }

        record.inviter_id = match invite_code {
            Some(code) => match lock_inviter_by_code(&transaction, code).await? {
                Some(user_id) => Some(user_id),
                None => {
                    rollback_transaction(transaction).await?;
                    return Ok(AdminUserRegistrationCreateOutcome::InvitationRejected);
                }
            },
            None => None,
        };

        let snapshot = create_user_in_transaction(&transaction, record).await?;
        if let Some(inviter_id) = record_inviter(&transaction, snapshot.user_id()).await?
            && !rebate_quota.is_zero()
        {
            let grant = InviteRebateGrant::new(
                registration_rebate_event_id(snapshot.user_id())?,
                snapshot.user_id(),
                rebate_quota,
                credited_at,
            )
            .map_err(|_| record_internal_error(AdminUserRepositoryError::Invariant))?;
            match grant_in_transaction(&transaction, &grant)
                .await
                .map_err(map_invite_rebate_error)?
            {
                InviteRebateGrantOutcome::Applied(record)
                    if record.inviter_user_id() == inviter_id => {}
                _ => {
                    return Err(record_internal_error(AdminUserRepositoryError::Invariant));
                }
            }
        }
        commit_transaction(transaction).await?;
        Ok(AdminUserRegistrationCreateOutcome::Created(snapshot))
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

pub(crate) async fn create_user_in_transaction(
    transaction: &DatabaseTransaction,
    record: AdminUserCreateRecord,
) -> Result<AdminUserRecord, AdminUserRepositoryError> {
    ensure_group_exists(transaction, record.default_group_id).await?;
    ensure_identity_available(transaction, &record.username, record.email.as_deref(), None).await?;
    let password_hash = record
        .password
        .as_ref()
        .map(hash_password)
        .transpose()
        .map_err(map_identity_secret_error)?;
    let aff_code = generate_aff_code().map_err(map_identity_secret_error)?;
    let quota = Quota::new(record.quota)
        .map_err(|_| record_internal_error(AdminUserRepositoryError::Invariant))?;
    let inserted = users::ActiveModel {
        username: Set(record.username),
        email: Set(record.email),
        password_hash: Set(password_hash),
        role: Set(record.role),
        status: Set(record.status),
        default_group_id: Set(record.default_group_id.get()),
        quota: Set(quota.units()),
        aff_code: Set(aff_code),
        inviter_id: Set(record.inviter_id.map(UserId::get)),
        rpm_limit: Set(record.rpm_limit),
        concurrency: Set(record.concurrency),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(transaction)
    .with_subscriber(NoSubscriber::default())
    .await
    .map_err(map_write_db_error)?;
    let user_id = UserId::new(inserted.id)
        .map_err(|_| record_internal_error(AdminUserRepositoryError::Invariant))?;
    crate::wallet_ledger::insert_opening_entry(transaction, user_id, quota, inserted.created_at)
        .await
        .map_err(map_write_db_error)?;
    fetch_user_snapshot(transaction, user_id).await
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

async fn rollback_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), AdminUserRepositoryError> {
    transaction
        .rollback()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminUserRepositoryError::Query))
}

async fn lock_inviter_by_code(
    transaction: &DatabaseTransaction,
    invite_code: &str,
) -> Result<Option<UserId>, AdminUserRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，恒等更新用于在邀请码解析与创建之间固定邀请人状态。
        users::Entity::update_many()
            .filter(users::Column::AffCode.eq(invite_code))
            .filter(users::Column::Status.eq(ENABLED_USER_STATUS))
            .filter(users::Column::DeletedAt.is_null())
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
    }
    let mut query = users::Entity::find()
        .filter(users::Column::AffCode.eq(invite_code))
        .filter(users::Column::Status.eq(ENABLED_USER_STATUS))
        .filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?
        .map(|model| {
            UserId::new(model.id)
                .map_err(|_| record_internal_error(AdminUserRepositoryError::Invariant))
        })
        .transpose()
}

async fn record_inviter(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<Option<UserId>, AdminUserRepositoryError> {
    users::Entity::find_by_id(user_id.get())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?
        .ok_or_else(|| record_internal_error(AdminUserRepositoryError::Invariant))?
        .inviter_id
        .map(UserId::new)
        .transpose()
        .map_err(|_| record_internal_error(AdminUserRepositoryError::Invariant))
}

fn registration_rebate_event_id(
    invitee_user_id: UserId,
) -> Result<WalletEventId, AdminUserRepositoryError> {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&REGISTRATION_REBATE_EVENT_NAMESPACE);
    bytes[8..].copy_from_slice(&invitee_user_id.get().to_be_bytes());
    WalletEventId::new(bytes)
        .map_err(|_| record_internal_error(AdminUserRepositoryError::Invariant))
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

fn map_auth_challenge_error(error: AuthChallengeRepositoryError) -> AdminUserRepositoryError {
    match error {
        AuthChallengeRepositoryError::Query => AdminUserRepositoryError::Query,
        AuthChallengeRepositoryError::Timeout => AdminUserRepositoryError::Timeout,
        AuthChallengeRepositoryError::Invariant => AdminUserRepositoryError::Invariant,
    }
}

fn map_invite_rebate_error(error: InviteRebateRepositoryError) -> AdminUserRepositoryError {
    match error {
        InviteRebateRepositoryError::Query => AdminUserRepositoryError::Query,
        InviteRebateRepositoryError::Conflict
        | InviteRebateRepositoryError::OutcomeUnknown
        | InviteRebateRepositoryError::Invariant => {
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
            Some(60),
            Some(2),
        );

        assert_eq!(format!("{create:?}"), "AdminUserCreateRecord(<redacted>)");
        assert_eq!(format!("{update:?}"), "AdminUserUpdateRecord(<redacted>)");
        assert!(!format!("{create:?}").contains("secret-password"));
        assert!(!format!("{update:?}").contains("next-secret"));
    }
}
