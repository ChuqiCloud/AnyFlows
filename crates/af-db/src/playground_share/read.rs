use af_domain::UserId;
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::Expr,
};

use crate::entity::{TokenHash, playground_shares};

use super::{
    PlaygroundShareRecord, PlaygroundShareRepository, PlaygroundShareRepositoryError,
    PlaygroundShareRevokeOutcome, record_internal_error, validate_snapshot,
};

pub(super) async fn find_active(
    repository: &PlaygroundShareRepository,
    token_hash: TokenHash,
) -> Result<Option<PlaygroundShareRecord>, PlaygroundShareRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    playground_shares::Entity::find()
        .filter(playground_shares::Column::TokenHash.eq(token_hash))
        .filter(playground_shares::Column::RevokedAt.is_null())
        .filter(playground_shares::Column::ExpiresAt.gt(now))
        .one(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(PlaygroundShareRepositoryError::Query))?
        .map(try_from_model)
        .transpose()
}

pub(super) async fn revoke(
    repository: &PlaygroundShareRepository,
    owner_user_id: UserId,
    token_hash: TokenHash,
) -> Result<PlaygroundShareRevokeOutcome, PlaygroundShareRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let result = playground_shares::Entity::update_many()
        .filter(playground_shares::Column::OwnerUserId.eq(owner_user_id.get()))
        .filter(playground_shares::Column::TokenHash.eq(token_hash))
        .filter(playground_shares::Column::RevokedAt.is_null())
        .filter(playground_shares::Column::ExpiresAt.gt(now))
        .col_expr(playground_shares::Column::RevokedAt, Expr::value(now))
        .exec(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(PlaygroundShareRepositoryError::Query))?;
    match result.rows_affected {
        0 => Ok(PlaygroundShareRevokeOutcome::NotFound),
        1 => Ok(PlaygroundShareRevokeOutcome::Revoked),
        _ => Err(record_internal_error(
            PlaygroundShareRepositoryError::Invariant,
        )),
    }
}

fn try_from_model(
    model: playground_shares::Model,
) -> Result<PlaygroundShareRecord, PlaygroundShareRepositoryError> {
    let snapshot = model.snapshot.into_inner();
    if model.id <= 0
        || model.owner_user_id <= 0
        || model.revoked_at.is_some()
        || model.expires_at <= model.created_at
    {
        return Err(record_internal_error(
            PlaygroundShareRepositoryError::Invariant,
        ));
    }
    validate_snapshot(&snapshot)
        .map_err(|_| record_internal_error(PlaygroundShareRepositoryError::Invariant))?;
    Ok(PlaygroundShareRecord {
        snapshot,
        created_at: model.created_at.unix_timestamp(),
        expires_at: model.expires_at.unix_timestamp(),
    })
}
