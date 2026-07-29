use af_domain::UserId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QuerySelect, Set, SqlErr,
    TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};

use crate::entity::{
    PlaygroundConversationKey, SensitiveJson, SensitiveString, playground_conversations, users,
};

use super::{
    MAX_PLAYGROUND_CONVERSATIONS_PER_USER, PlaygroundConversationRecord,
    PlaygroundConversationRepository, PlaygroundConversationRepositoryError,
    PlaygroundConversationWrite, record_internal_error, validate_models, validate_snapshot,
    validate_title,
};

pub(super) async fn save(
    repository: &PlaygroundConversationRepository,
    owner_user_id: UserId,
    write: PlaygroundConversationWrite,
) -> Result<PlaygroundConversationRecord, PlaygroundConversationRepositoryError> {
    let conversation_id = PlaygroundConversationKey::parse(&write.conversation_id)
        .map_err(|_| PlaygroundConversationRepositoryError::InvalidInput)?;
    validate_title(&write.title)?;
    validate_models(&write.models)?;
    validate_snapshot(&write.snapshot)?;
    if write
        .expected_revision
        .is_some_and(|revision| revision <= 0)
    {
        return Err(PlaygroundConversationRepositoryError::InvalidInput);
    }
    let models = serde_json::to_value(&write.models)
        .map_err(|_| PlaygroundConversationRepositoryError::InvalidInput)?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))?;

    if !lock_active_owner(&transaction, owner_user_id).await? {
        rollback(transaction).await?;
        return Err(PlaygroundConversationRepositoryError::OwnerUnavailable);
    }

    if let Some(existing) =
        lock_existing(&transaction, owner_user_id, conversation_id.clone()).await?
    {
        let validated = super::read::try_from_model(existing.clone())?;
        let payload_matches = existing.title.as_str() == write.title
            && existing.models.clone().into_inner() == models
            && existing.snapshot.clone().into_inner() == write.snapshot;
        if is_idempotent_replay(existing.revision, write.expected_revision, payload_matches) {
            transaction
                .commit()
                .await
                .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))?;
            return Ok(validated);
        }

        if write.expected_revision != Some(existing.revision) {
            rollback(transaction).await?;
            return Err(PlaygroundConversationRepositoryError::Conflict);
        }
        let revision = existing.revision.checked_add(1).ok_or_else(|| {
            record_internal_error(PlaygroundConversationRepositoryError::Invariant)
        })?;
        let mut active = existing.into_active_model();
        active.title = Set(SensitiveString::from(write.title));
        active.models = Set(SensitiveJson::from(models));
        active.snapshot = Set(SensitiveJson::from(write.snapshot));
        active.revision = Set(revision);
        active.updated_at = Set(now);
        let updated = active
            .update(&transaction)
            .await
            .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))?;
        let record = super::read::try_from_model(updated)?;
        transaction
            .commit()
            .await
            .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))?;
        return Ok(record);
    }

    if write.expected_revision.is_some() {
        rollback(transaction).await?;
        return Err(PlaygroundConversationRepositoryError::NotFound);
    }
    if conversation_count(&transaction, owner_user_id).await?
        >= MAX_PLAYGROUND_CONVERSATIONS_PER_USER as u64
    {
        rollback(transaction).await?;
        return Err(PlaygroundConversationRepositoryError::LimitReached);
    }

    let inserted = playground_conversations::ActiveModel {
        conversation_id: Set(conversation_id),
        owner_user_id: Set(owner_user_id.get()),
        title: Set(SensitiveString::from(write.title)),
        models: Set(SensitiveJson::from(models)),
        snapshot: Set(SensitiveJson::from(write.snapshot)),
        revision: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&transaction)
    .await;
    let inserted = match inserted {
        Ok(model) => model,
        Err(error) => {
            let mapped = if is_unique_violation(&error) {
                PlaygroundConversationRepositoryError::Conflict
            } else {
                record_internal_error(PlaygroundConversationRepositoryError::Query)
            };
            rollback(transaction).await?;
            return Err(mapped);
        }
    };
    let record = super::read::try_from_model(inserted)?;
    transaction
        .commit()
        .await
        .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))?;
    Ok(record)
}

fn is_idempotent_replay(
    current_revision: i64,
    expected_revision: Option<i64>,
    payload_matches: bool,
) -> bool {
    if !payload_matches {
        return false;
    }
    match expected_revision {
        None => current_revision == 1,
        Some(expected) => {
            current_revision == expected
                || expected
                    .checked_add(1)
                    .is_some_and(|revision| revision == current_revision)
        }
    }
}

async fn lock_active_owner(
    transaction: &DatabaseTransaction,
    owner_user_id: UserId,
) -> Result<bool, PlaygroundConversationRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，先取得数据库写锁，再读取用户有效状态。
        let result = users::Entity::update_many()
            .filter(users::Column::Id.eq(owner_user_id.get()))
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
            .exec(transaction)
            .await
            .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))?;
        if result.rows_affected != 1 {
            return Ok(false);
        }
    }
    let mut query = users::Entity::find()
        .select_only()
        .column(users::Column::Id)
        .filter(users::Column::Id.eq(owner_user_id.get()))
        .filter(users::Column::Status.eq(1_i16))
        .filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .into_tuple::<i64>()
        .one(transaction)
        .await
        .map(|row| row.is_some())
        .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))
}

async fn lock_existing(
    transaction: &DatabaseTransaction,
    owner_user_id: UserId,
    conversation_id: PlaygroundConversationKey,
) -> Result<Option<playground_conversations::Model>, PlaygroundConversationRepositoryError> {
    let mut query = playground_conversations::Entity::find()
        .filter(playground_conversations::Column::OwnerUserId.eq(owner_user_id.get()))
        .filter(playground_conversations::Column::ConversationId.eq(conversation_id));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))
}

async fn conversation_count(
    transaction: &DatabaseTransaction,
    owner_user_id: UserId,
) -> Result<u64, PlaygroundConversationRepositoryError> {
    playground_conversations::Entity::find()
        .filter(playground_conversations::Column::OwnerUserId.eq(owner_user_id.get()))
        .count(transaction)
        .await
        .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))
}

async fn rollback(
    transaction: DatabaseTransaction,
) -> Result<(), PlaygroundConversationRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

#[cfg(test)]
mod tests {
    use super::is_idempotent_replay;

    #[test]
    fn idempotent_replay_accepts_only_matching_create_or_adjacent_update() {
        assert!(is_idempotent_replay(1, None, true));
        assert!(is_idempotent_replay(2, Some(1), true));
        assert!(is_idempotent_replay(2, Some(2), true));
        assert!(!is_idempotent_replay(2, None, true));
        assert!(!is_idempotent_replay(3, Some(1), true));
        assert!(!is_idempotent_replay(2, Some(1), false));
    }
}
