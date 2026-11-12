use super::{
    PersonalAccessTokenAccessAuditRecord, PersonalAccessTokenAuditCleanupResult,
    PersonalAccessTokenRepository as Repository, PersonalAccessTokenRepositoryError as Error,
    validation,
};
use crate::entity::{access_token_audit_logs as logs, user_access_tokens as tokens};
use af_domain::UserId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, Set,
};

pub(super) async fn record(
    repository: &Repository,
    record: PersonalAccessTokenAccessAuditRecord,
) -> Result<(), Error> {
    if !validation::text(&record.request_id, 128)
        || !validation::text(&record.method, 16)
        || !record.method.bytes().all(|byte| byte.is_ascii_uppercase())
        || !validation::text(&record.route, 256)
        || record.route.contains('?')
        || record.required_scope.as_deref().is_some_and(|scopes| {
            scopes.len() > 1024 || !scopes.split_ascii_whitespace().all(validation::valid_scope)
        })
        || record
            .resource_id
            .as_deref()
            .is_some_and(|id| !validation::text(id, 128))
        || !(100..=599).contains(&record.status_code)
        || record.token_id.is_some_and(|id| id <= 0)
    {
        return Err(Error::InvalidInput);
    }
    if let (Some(user), Some(token)) = (record.user_id, record.token_id) {
        let existing = tokens::Entity::find_by_id(token)
            .one(repository.pool.connection())
            .await
            .map_err(|_| Error::Query)?
            .ok_or(Error::NotFound)?;
        if existing.user_id != user.get() {
            return Err(Error::InvalidInput);
        }
    } else if record.allowed || record.token_id.is_some() {
        return Err(Error::InvalidInput);
    }
    insert(
        repository.pool.connection(),
        record.user_id,
        record.token_id,
        "request",
        record.resource_id,
        record.request_id,
        record.method,
        record.route,
        record.required_scope,
        i32::from(record.status_code),
        record.allowed,
        Some(record.client_ip.as_ip().to_string()),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn insert<C: ConnectionTrait>(
    connection: &C,
    user: Option<UserId>,
    token: Option<i64>,
    operation: &str,
    resource_id: Option<String>,
    request_id: String,
    method: String,
    route: String,
    required_scope: Option<String>,
    status_code: i32,
    allowed: bool,
    client_ip: Option<String>,
) -> Result<(), Error> {
    logs::ActiveModel {
        user_id: Set(user.map(UserId::get)),
        access_token_id: Set(token),
        resource_id: Set(resource_id),
        operation: Set(operation.to_owned()),
        request_id: Set(request_id),
        method: Set(method),
        route: Set(route),
        required_scope: Set(required_scope),
        status_code: Set(status_code),
        allowed: Set(allowed),
        client_ip: Set(client_ip),
        created_at: Set(time::OffsetDateTime::now_utc().unix_timestamp()),
        ..Default::default()
    }
    .insert(connection)
    .await
    .map_err(|_| Error::Query)?;
    Ok(())
}

pub(super) async fn mutation<C: ConnectionTrait>(
    connection: &C,
    actor: UserId,
    token: Option<i64>,
    operation: &str,
    context: super::PersonalAccessTokenMutationContext,
) -> Result<(), Error> {
    if !validation::text(&context.request_id, 128)
        || !validation::text(&context.method, 16)
        || !validation::text(&context.route, 256)
        || context.route.contains('?')
        || context
            .resource_id
            .as_deref()
            .is_some_and(|id| !validation::text(id, 128))
        || !validation::text(operation, 32)
    {
        return Err(Error::InvalidInput);
    }
    insert(
        connection,
        Some(actor),
        token,
        operation,
        context.resource_id,
        context.request_id,
        context.method,
        context.route,
        None,
        200,
        true,
        Some(context.client_ip.as_ip().to_string()),
    )
    .await
}

pub(super) async fn prune(
    repository: &Repository,
    before: i64,
    limit: usize,
) -> Result<PersonalAccessTokenAuditCleanupResult, Error> {
    let ids = logs::Entity::find()
        .select_only()
        .column(logs::Column::Id)
        .filter(logs::Column::CreatedAt.lt(before))
        .limit(limit as u64)
        .into_tuple::<i64>()
        .all(repository.pool.connection())
        .await
        .map_err(|_| Error::Query)?;
    let deleted = if ids.is_empty() {
        0
    } else {
        logs::Entity::delete_many()
            .filter(logs::Column::Id.is_in(ids))
            .exec(repository.pool.connection())
            .await
            .map_err(|_| Error::Query)?
            .rows_affected
    };
    Ok(PersonalAccessTokenAuditCleanupResult {
        deleted,
        cutoff: before,
    })
}
