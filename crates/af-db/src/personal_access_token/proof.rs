use af_domain::UserId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter, QuerySelect, Set,
    TransactionTrait, sea_query::Expr,
};

use super::{
    PersonalAccessTokenMutationContext, PersonalAccessTokenProofClaim,
    PersonalAccessTokenProofRecord, PersonalAccessTokenRepository as Repository,
    PersonalAccessTokenRepositoryError as Error, valid_digest,
};
use crate::entity::{user_access_token_proofs as proofs, users};

pub(super) async fn issue(
    repository: &Repository,
    record: PersonalAccessTokenProofRecord,
    context: PersonalAccessTokenMutationContext,
) -> Result<(), Error> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    if !valid_digest(&record.proof_hash)
        || !valid_digest(&record.session_binding)
        || !valid_digest(&record.payload_digest)
        || record.session_version <= 0
        || !matches!(
            record.purpose.as_str(),
            "create" | "scope_expand" | "extend" | "enable"
        )
        || record.target_id.is_some_and(|id| id <= 0)
        || (record.purpose == "create") != record.target_id.is_none()
        || record.issued_at < now.saturating_sub(300)
        || record.issued_at > now + 30
        || record.expires_at <= now
        || record.expires_at > now + 300
    {
        return Err(Error::InvalidProof);
    }
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| Error::Query)?;
    let user = super::mutation::lock_owner(&transaction, record.user_id).await?;
    if user.status != 1
        || user.deleted_at.is_some()
        || user.session_version != record.session_version
    {
        return Err(Error::OwnerUnavailable);
    }
    let recent = proofs::Entity::find()
        .filter(proofs::Column::UserId.eq(record.user_id.get()))
        .filter(proofs::Column::IssuedAt.gt(now - 300))
        .all(&transaction)
        .await
        .map_err(|_| Error::Query)?;
    if recent.len() >= 20 {
        let oldest = recent
            .iter()
            .map(|proof| proof.issued_at)
            .min()
            .ok_or(Error::Invariant)?;
        let retry_after =
            u32::try_from(oldest.saturating_add(300).saturating_sub(now).clamp(1, 330))
                .map_err(|_| Error::Invariant)?;
        return Err(Error::RateLimited { retry_after });
    }
    proofs::ActiveModel {
        proof_hash: Set(record.proof_hash),
        user_id: Set(record.user_id.get()),
        session_version: Set(record.session_version),
        session_binding: Set(record.session_binding),
        purpose: Set(record.purpose),
        target_id: Set(record.target_id),
        payload_digest: Set(record.payload_digest),
        issued_at: Set(record.issued_at),
        expires_at: Set(record.expires_at),
        consumed_at: Set(None),
        ..Default::default()
    }
    .insert(&transaction)
    .await
    .map_err(|_| Error::Query)?;
    super::audit::mutation(
        &transaction,
        record.user_id,
        record.target_id,
        "proof_issue",
        context,
    )
    .await?;
    transaction.commit().await.map_err(|_| Error::Query)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn consume(
    _repository: &Repository,
    transaction: &DatabaseTransaction,
    owner: UserId,
    claim: &PersonalAccessTokenProofClaim,
    purpose: &str,
    target: Option<i64>,
    payload: &str,
    now: i64,
) -> Result<(), Error> {
    if !valid_digest(&claim.proof_hash)
        || !valid_digest(&claim.session_binding)
        || !valid_digest(&claim.payload_digest)
        || claim.session_version <= 0
        || claim.purpose != purpose
        || claim.target_id != target
        || claim.payload_digest != payload
    {
        return Err(Error::InvalidProof);
    }
    let Some(user) = users::Entity::find_by_id(owner.get())
        .one(transaction)
        .await
        .map_err(|_| Error::Query)?
    else {
        return Err(Error::OwnerUnavailable);
    };
    if user.status != 1
        || user.deleted_at.is_some()
        || user.session_version != claim.session_version
    {
        return Err(Error::OwnerUnavailable);
    }
    let mut update = proofs::Entity::update_many()
        .filter(proofs::Column::ProofHash.eq(&claim.proof_hash))
        .filter(proofs::Column::UserId.eq(owner.get()))
        .filter(proofs::Column::SessionVersion.eq(claim.session_version))
        .filter(proofs::Column::SessionBinding.eq(&claim.session_binding))
        .filter(proofs::Column::Purpose.eq(purpose))
        .filter(proofs::Column::PayloadDigest.eq(payload))
        .filter(proofs::Column::ExpiresAt.gt(now))
        .filter(proofs::Column::ConsumedAt.is_null())
        .col_expr(proofs::Column::ConsumedAt, Expr::value(now));
    update = if let Some(target) = target {
        update.filter(proofs::Column::TargetId.eq(target))
    } else {
        update.filter(proofs::Column::TargetId.is_null())
    };
    if update
        .exec(transaction)
        .await
        .map_err(|_| Error::Query)?
        .rows_affected
        != 1
    {
        return Err(Error::InvalidProof);
    }
    Ok(())
}

pub(super) async fn prune(repository: &Repository, now: i64, limit: usize) -> Result<u64, Error> {
    let ids = proofs::Entity::find()
        .select_only()
        .column(proofs::Column::Id)
        .filter(proofs::Column::ExpiresAt.lt(now))
        .limit(limit as u64)
        .into_tuple::<i64>()
        .all(repository.pool.connection())
        .await
        .map_err(|_| Error::Query)?;
    if ids.is_empty() {
        return Ok(0);
    }
    Ok(proofs::Entity::delete_many()
        .filter(proofs::Column::Id.is_in(ids))
        .exec(repository.pool.connection())
        .await
        .map_err(|_| Error::Query)?
        .rows_affected)
}
