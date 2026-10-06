use af_domain::UserId;
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use crate::entity::{
    PlaygroundConversationKey, SensitiveJson, SensitiveString, playground_conversations,
};

use super::{
    MAX_PLAYGROUND_CONVERSATIONS_PER_USER, PlaygroundConversationDeleteOutcome,
    PlaygroundConversationRecord, PlaygroundConversationRepository,
    PlaygroundConversationRepositoryError, PlaygroundConversationSummaryRecord,
    record_internal_error, validate_models, validate_snapshot, validate_title,
};

type SummaryTuple = (
    PlaygroundConversationKey,
    SensitiveString,
    SensitiveJson,
    i64,
    TimeDateTimeWithTimeZone,
    TimeDateTimeWithTimeZone,
);

pub(super) async fn list(
    repository: &PlaygroundConversationRepository,
    owner_user_id: UserId,
) -> Result<Vec<PlaygroundConversationSummaryRecord>, PlaygroundConversationRepositoryError> {
    // 摘要查询显式排除 snapshot，避免列表路径加载最多 50 份大正文。
    let rows = playground_conversations::Entity::find()
        .select_only()
        .column(playground_conversations::Column::ConversationId)
        .column(playground_conversations::Column::Title)
        .column(playground_conversations::Column::Models)
        .column(playground_conversations::Column::Revision)
        .column(playground_conversations::Column::CreatedAt)
        .column(playground_conversations::Column::UpdatedAt)
        .filter(playground_conversations::Column::OwnerUserId.eq(owner_user_id.get()))
        .order_by_desc(playground_conversations::Column::UpdatedAt)
        .order_by_desc(playground_conversations::Column::ConversationId)
        .limit(MAX_PLAYGROUND_CONVERSATIONS_PER_USER as u64)
        .into_tuple::<SummaryTuple>()
        .all(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))?;
    rows.into_iter().map(try_from_summary_tuple).collect()
}

pub(super) async fn find(
    repository: &PlaygroundConversationRepository,
    owner_user_id: UserId,
    conversation_id: PlaygroundConversationKey,
) -> Result<Option<PlaygroundConversationRecord>, PlaygroundConversationRepositoryError> {
    playground_conversations::Entity::find()
        .filter(playground_conversations::Column::OwnerUserId.eq(owner_user_id.get()))
        .filter(playground_conversations::Column::ConversationId.eq(conversation_id))
        .one(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))?
        .map(try_from_model)
        .transpose()
}

pub(super) async fn delete(
    repository: &PlaygroundConversationRepository,
    owner_user_id: UserId,
    conversation_id: PlaygroundConversationKey,
) -> Result<PlaygroundConversationDeleteOutcome, PlaygroundConversationRepositoryError> {
    let result = playground_conversations::Entity::delete_many()
        .filter(playground_conversations::Column::OwnerUserId.eq(owner_user_id.get()))
        .filter(playground_conversations::Column::ConversationId.eq(conversation_id))
        .exec(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(PlaygroundConversationRepositoryError::Query))?;
    match result.rows_affected {
        0 => Ok(PlaygroundConversationDeleteOutcome::NotFound),
        1 => Ok(PlaygroundConversationDeleteOutcome::Deleted),
        _ => Err(record_internal_error(
            PlaygroundConversationRepositoryError::Invariant,
        )),
    }
}

pub(super) fn try_from_model(
    model: playground_conversations::Model,
) -> Result<PlaygroundConversationRecord, PlaygroundConversationRepositoryError> {
    if model.owner_user_id <= 0 {
        return Err(internal_invariant());
    }
    let conversation_id = model.conversation_id.as_str().to_owned();
    let title = model.title.as_str().to_owned();
    let models = parse_models(model.models.into_inner())?;
    let snapshot = model.snapshot.into_inner();
    let (created_at, updated_at) = validate_common(
        &title,
        &models,
        model.revision,
        model.created_at,
        model.updated_at,
    )?;
    validate_snapshot(&snapshot).map_err(|_| internal_invariant())?;
    Ok(PlaygroundConversationRecord {
        conversation_id,
        title,
        models,
        snapshot,
        revision: model.revision,
        created_at,
        updated_at,
    })
}

fn try_from_summary_tuple(
    (conversation_id, title, models, revision, created_at, updated_at): SummaryTuple,
) -> Result<PlaygroundConversationSummaryRecord, PlaygroundConversationRepositoryError> {
    let conversation_id = conversation_id.as_str().to_owned();
    let title = title.as_str().to_owned();
    let models = parse_models(models.into_inner())?;
    let (created_at, updated_at) =
        validate_common(&title, &models, revision, created_at, updated_at)?;
    Ok(PlaygroundConversationSummaryRecord {
        conversation_id,
        title,
        models,
        revision,
        created_at,
        updated_at,
    })
}

fn parse_models(
    models: serde_json::Value,
) -> Result<Vec<String>, PlaygroundConversationRepositoryError> {
    let models = serde_json::from_value::<Vec<String>>(models).map_err(|_| internal_invariant())?;
    validate_models(&models).map_err(|_| internal_invariant())?;
    Ok(models)
}

fn validate_common(
    title: &str,
    models: &[String],
    revision: i64,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
) -> Result<(i64, i64), PlaygroundConversationRepositoryError> {
    validate_title(title).map_err(|_| internal_invariant())?;
    validate_models(models).map_err(|_| internal_invariant())?;
    let created_at = created_at.unix_timestamp();
    let updated_at = updated_at.unix_timestamp();
    if revision <= 0 || created_at <= 0 || updated_at < created_at {
        return Err(internal_invariant());
    }
    Ok((created_at, updated_at))
}

fn internal_invariant() -> PlaygroundConversationRepositoryError {
    record_internal_error(PlaygroundConversationRepositoryError::Invariant)
}
