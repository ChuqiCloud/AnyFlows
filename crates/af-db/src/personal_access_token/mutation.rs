use af_domain::UserId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, LockType},
};

use super::{
    PersonalAccessTokenCreateRecord, PersonalAccessTokenMutationContext,
    PersonalAccessTokenPolicyRecord, PersonalAccessTokenPolicyUpdateRecord,
    PersonalAccessTokenProofClaim, PersonalAccessTokenRecord,
    PersonalAccessTokenRepository as Repository, PersonalAccessTokenRepositoryError as Error,
    PersonalAccessTokenUpdateRecord, audit, read, validation,
};
use crate::entity::{access_token_policies as policies, user_access_tokens as tokens, users};

pub(super) async fn create(
    repository: &Repository,
    record: PersonalAccessTokenCreateRecord,
    proof: PersonalAccessTokenProofClaim,
    context: PersonalAccessTokenMutationContext,
) -> Result<PersonalAccessTokenRecord, Error> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    validation::fields(
        &record.name,
        &record.scopes,
        record.expires_at,
        record.allow_ips.as_deref(),
        now,
    )?;
    if !super::valid_digest(&record.secret_hash)
        || !record.token_hint.starts_with("af_pat_")
        || record.token_hint.len() != 19
        || record.expires_at.is_some_and(|expiry| expiry <= now)
    {
        return Err(Error::InvalidInput);
    }
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| Error::Query)?;
    let owner = lock_owner(&transaction, record.user_id).await?;
    validation::grant(owner.role, &record.scopes)?;
    let policy = lock_policy(&transaction).await?;
    if !policy.enabled {
        return Err(Error::Disabled);
    }
    if !policy.issuance_enabled {
        return Err(Error::IssuanceDisabled);
    }
    let count = tokens::Entity::find()
        .filter(tokens::Column::UserId.eq(record.user_id.get()))
        .filter(tokens::Column::RevokedAt.is_null())
        .count(&transaction)
        .await
        .map_err(|_| Error::Query)?;
    if count >= policy.max_tokens_per_user as u64 {
        return Err(Error::LimitReached);
    }
    let payload = validation::payload_digest(
        &record.name,
        &record.scopes,
        record.expires_at,
        record.allow_ips.as_deref(),
        true,
        None,
    )?;
    repository
        .consume_proof(
            &transaction,
            record.user_id,
            &proof,
            "create",
            None,
            &payload,
            now,
        )
        .await?;
    let token = tokens::ActiveModel {
        user_id: Set(record.user_id.get()),
        secret_hash: Set(record.secret_hash),
        token_hint: Set(record.token_hint),
        name: Set(record.name),
        scopes: Set(serde_json::json!(record.scopes)),
        enabled: Set(true),
        expires_at: Set(record.expires_at),
        revoked_at: Set(None),
        allow_ips: Set(record.allow_ips.map(|ips| serde_json::json!(ips))),
        created_at: Set(now),
        updated_at: Set(now),
        last_used_at: Set(None),
        last_used_ip: Set(None),
        version: Set(1),
        ..Default::default()
    }
    .insert(&transaction)
    .await
    .map_err(|_| Error::Query)?;
    audit::mutation(
        &transaction,
        record.user_id,
        Some(token.id),
        "create",
        context,
    )
    .await?;
    let snapshot = read::record(token)?;
    transaction.commit().await.map_err(|_| Error::Query)?;
    Ok(snapshot)
}

pub(super) async fn update(
    repository: &Repository,
    owner_id: UserId,
    id: i64,
    update: PersonalAccessTokenUpdateRecord,
    proof: Option<PersonalAccessTokenProofClaim>,
    context: PersonalAccessTokenMutationContext,
) -> Result<PersonalAccessTokenRecord, Error> {
    if id <= 0 || update.expected_version <= 0 {
        return Err(Error::InvalidInput);
    }
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    validation::fields(
        &update.name,
        &update.scopes,
        update.expires_at,
        update.allow_ips.as_deref(),
        now,
    )?;
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| Error::Query)?;
    let owner = lock_owner(&transaction, owner_id).await?;
    validation::grant(owner.role, &update.scopes)?;
    let current = token_for_owner(&transaction, owner_id, id).await?;
    if current.version != update.expected_version {
        return Err(Error::Conflict);
    }
    let current_scopes = validation::json_strings(&current.scopes)?;
    let current_ips = validation::json_ips(current.allow_ips.as_ref())?;
    let scope_expansion = update
        .scopes
        .iter()
        .any(|scope| !current_scopes.contains(scope))
        || ip_expansion(current_ips.as_deref(), update.allow_ips.as_deref());
    let extending = match (current.expires_at, update.expires_at) {
        (Some(_), None) => true,
        (Some(old), Some(new)) => new > old,
        _ => false,
    };
    let enabling = !current.enabled && update.enabled;
    let required = if scope_expansion {
        Some("scope_expand")
    } else if extending {
        Some("extend")
    } else if enabling {
        Some("enable")
    } else {
        None
    };
    if let Some(purpose) = required {
        let policy = lock_policy(&transaction).await?;
        if !policy.enabled {
            return Err(Error::Disabled);
        }
        if !policy.issuance_enabled {
            return Err(Error::IssuanceDisabled);
        }
        let payload = validation::payload_digest(
            &update.name,
            &update.scopes,
            update.expires_at,
            update.allow_ips.as_deref(),
            update.enabled,
            Some(update.expected_version),
        )?;
        repository
            .consume_proof(
                &transaction,
                owner_id,
                proof.as_ref().ok_or(Error::InvalidProof)?,
                purpose,
                Some(id),
                &payload,
                now,
            )
            .await?;
    }
    let mut model = current.into_active_model();
    model.name = Set(update.name);
    model.scopes = Set(serde_json::json!(update.scopes));
    model.expires_at = Set(update.expires_at);
    model.allow_ips = Set(update.allow_ips.map(|ips| serde_json::json!(ips)));
    model.enabled = Set(update.enabled);
    model.version = Set(update
        .expected_version
        .checked_add(1)
        .ok_or(Error::Invariant)?);
    model.updated_at = Set(now);
    let snapshot = read::record(model.update(&transaction).await.map_err(|_| Error::Query)?)?;
    audit::mutation(&transaction, owner_id, Some(id), "update", context).await?;
    transaction.commit().await.map_err(|_| Error::Query)?;
    Ok(snapshot)
}

pub(super) async fn revoke(
    repository: &Repository,
    owner: UserId,
    id: i64,
    context: PersonalAccessTokenMutationContext,
) -> Result<(), Error> {
    if id <= 0 {
        return Err(Error::InvalidInput);
    }
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| Error::Query)?;
    lock_owner(&transaction, owner).await?;
    let token = tokens::Entity::find_by_id(id)
        .one(&transaction)
        .await
        .map_err(|_| Error::Query)?
        .filter(|token| token.user_id == owner.get())
        .ok_or(Error::NotFound)?;
    if token.revoked_at.is_none() {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let mut model = token.into_active_model();
        model.revoked_at = Set(Some(now));
        model.enabled = Set(false);
        model.updated_at = Set(now);
        model.update(&transaction).await.map_err(|_| Error::Query)?;
        audit::mutation(&transaction, owner, Some(id), "revoke", context).await?;
    }
    transaction.commit().await.map_err(|_| Error::Query)?;
    Ok(())
}

pub(super) async fn revoke_all(
    repository: &Repository,
    actor: UserId,
    target: UserId,
    context: PersonalAccessTokenMutationContext,
) -> Result<u64, Error> {
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| Error::Query)?;
    let (first, second) = if actor.get() <= target.get() {
        (actor, target)
    } else {
        (target, actor)
    };
    lock_user(&transaction, first, first == actor).await?;
    if first != second {
        lock_user(&transaction, second, second == actor).await?;
    }
    let caller = users::Entity::find_by_id(actor.get())
        .one(&transaction)
        .await
        .map_err(|_| Error::Query)?
        .ok_or(Error::OwnerUnavailable)?;
    if actor != target && caller.role != 1 {
        return Err(Error::Forbidden);
    }
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let changed = tokens::Entity::update_many()
        .filter(tokens::Column::UserId.eq(target.get()))
        .filter(tokens::Column::RevokedAt.is_null())
        .col_expr(tokens::Column::RevokedAt, Expr::value(now))
        .col_expr(tokens::Column::Enabled, Expr::value(false))
        .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
        .exec(&transaction)
        .await
        .map_err(|_| Error::Query)?
        .rows_affected;
    audit::mutation(&transaction, actor, None, "revoke_all", context).await?;
    transaction.commit().await.map_err(|_| Error::Query)?;
    Ok(changed)
}

pub(super) async fn update_policy(
    repository: &Repository,
    actor: UserId,
    update: PersonalAccessTokenPolicyUpdateRecord,
    context: PersonalAccessTokenMutationContext,
) -> Result<PersonalAccessTokenPolicyRecord, Error> {
    if !(1..=1000).contains(&update.max_tokens_per_user) || update.expected_version <= 0 {
        return Err(Error::InvalidInput);
    }
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| Error::Query)?;
    if lock_owner(&transaction, actor).await?.role != 1 {
        return Err(Error::Forbidden);
    }
    let current = lock_policy(&transaction).await?;
    if current.version != update.expected_version {
        return Err(Error::Conflict);
    }
    let model = policies::ActiveModel {
        id: Set(1),
        enabled: Set(update.enabled),
        issuance_enabled: Set(update.issuance_enabled),
        max_tokens_per_user: Set(update.max_tokens_per_user as i32),
        version: Set(current.version.checked_add(1).ok_or(Error::Invariant)?),
    }
    .update(&transaction)
    .await
    .map_err(|_| Error::Query)?;
    audit::mutation(&transaction, actor, None, "policy_update", context).await?;
    let snapshot = read::policy_record(model)?;
    transaction.commit().await.map_err(|_| Error::Query)?;
    Ok(snapshot)
}

pub(super) async fn lock_owner(
    transaction: &DatabaseTransaction,
    id: UserId,
) -> Result<users::Model, Error> {
    lock_user(transaction, id, true).await
}

async fn lock_user(
    transaction: &DatabaseTransaction,
    id: UserId,
    require_active: bool,
) -> Result<users::Model, Error> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let changed = users::Entity::update_many()
            .filter(users::Column::Id.eq(id.get()))
            .col_expr(
                users::Column::SessionVersion,
                Expr::col(users::Column::SessionVersion).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| Error::Query)?;
        if changed.rows_affected != 1 {
            return Err(Error::OwnerUnavailable);
        }
    }
    let mut query = users::Entity::find_by_id(id.get());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let user = query
        .one(transaction)
        .await
        .map_err(|_| Error::Query)?
        .ok_or(Error::OwnerUnavailable)?;
    if require_active && (user.status != 1 || user.deleted_at.is_some()) {
        return Err(Error::OwnerUnavailable);
    }
    if !matches!(user.role, 0 | 1) {
        return Err(Error::Invariant);
    }
    Ok(user)
}

async fn lock_policy(
    transaction: &DatabaseTransaction,
) -> Result<PersonalAccessTokenPolicyRecord, Error> {
    let mut query = policies::Entity::find_by_id(1_i64);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    read::policy_record(
        query
            .one(transaction)
            .await
            .map_err(|_| Error::Query)?
            .ok_or(Error::Invariant)?,
    )
}

async fn token_for_owner(
    transaction: &DatabaseTransaction,
    owner: UserId,
    id: i64,
) -> Result<tokens::Model, Error> {
    tokens::Entity::find_by_id(id)
        .filter(tokens::Column::UserId.eq(owner.get()))
        .filter(tokens::Column::RevokedAt.is_null())
        .one(transaction)
        .await
        .map_err(|_| Error::Query)?
        .ok_or(Error::NotFound)
}

fn ip_expansion(old: Option<&[String]>, new: Option<&[String]>) -> bool {
    match (old, new) {
        (Some(_), None) => true,
        (Some(old), Some(new)) => new.iter().any(|ip| !old.contains(ip)),
        _ => false,
    }
}
