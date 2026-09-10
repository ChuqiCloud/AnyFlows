use af_domain::{PLAYGROUND_TOKEN_NAME, TokenId, UserId};
use sea_orm::{
    ConnectionTrait, QueryResult,
    sea_query::{Expr, Order, SelectStatement},
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{admin_token::base_query, entity::tokens};

use super::{
    UserTokenLookupOutcome, UserTokenPageRecord, UserTokenRecord, UserTokenRepository,
    UserTokenRepositoryError, record_internal_error,
};

pub(super) async fn list(
    repository: &UserTokenRepository,
    owner_user_id: UserId,
    after: Option<TokenId>,
    limit: usize,
) -> Result<UserTokenPageRecord, UserTokenRepositoryError> {
    let mut query = base_query(repository.pool.connection().get_database_backend());
    query
        .and_where(Expr::col((tokens::Entity, tokens::Column::UserId)).eq(owner_user_id.get()))
        .and_where(Expr::col((tokens::Entity, tokens::Column::OrganizationId)).is_null())
        .and_where(Expr::col((tokens::Entity, tokens::Column::Name)).ne(PLAYGROUND_TOKEN_NAME))
        .and_where(Expr::col((tokens::Entity, tokens::Column::DeletedAt)).is_null())
        .order_by((tokens::Entity, tokens::Column::Id), Order::Asc)
        .limit((limit + 1) as u64);
    if let Some(after) = after {
        query.and_where(Expr::col((tokens::Entity, tokens::Column::Id)).gt(after.get()));
    }
    let mut rows = query_all(repository, query).await?;
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }
    let tokens = rows
        .iter()
        .map(|row| record_from_row(row, owner_user_id))
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = has_more
        .then(|| tokens.last().map(UserTokenRecord::token_id))
        .flatten();
    Ok(UserTokenPageRecord {
        tokens,
        next_cursor,
    })
}

pub(super) async fn get(
    repository: &UserTokenRepository,
    owner_user_id: UserId,
    token_id: TokenId,
) -> Result<UserTokenLookupOutcome, UserTokenRepositoryError> {
    let query = detail_query(
        repository.pool.connection().get_database_backend(),
        owner_user_id,
        token_id,
    );
    let mut rows = query_all(repository, query).await?;
    match rows.len() {
        0 => Ok(UserTokenLookupOutcome::NotFound),
        1 => Ok(UserTokenLookupOutcome::Found(Box::new(record_from_row(
            &rows
                .pop()
                .ok_or_else(|| record_internal_error(UserTokenRepositoryError::Invariant))?,
            owner_user_id,
        )?))),
        _ => Err(record_internal_error(UserTokenRepositoryError::Invariant)),
    }
}

pub(super) fn detail_query(
    database_backend: sea_orm::DbBackend,
    owner_user_id: UserId,
    token_id: TokenId,
) -> SelectStatement {
    base_query(database_backend)
        .and_where(Expr::col((tokens::Entity, tokens::Column::Id)).eq(token_id.get()))
        .and_where(Expr::col((tokens::Entity, tokens::Column::UserId)).eq(owner_user_id.get()))
        .and_where(Expr::col((tokens::Entity, tokens::Column::OrganizationId)).is_null())
        .and_where(Expr::col((tokens::Entity, tokens::Column::Name)).ne(PLAYGROUND_TOKEN_NAME))
        .and_where(Expr::col((tokens::Entity, tokens::Column::DeletedAt)).is_null())
        .limit(2)
        .to_owned()
}

pub(super) fn record_from_row(
    row: &QueryResult,
    owner_user_id: UserId,
) -> Result<UserTokenRecord, UserTokenRepositoryError> {
    let record = crate::AdminTokenRecord::try_from_query_result(row)
        .map_err(|_| record_internal_error(UserTokenRepositoryError::Invariant))?;
    UserTokenRecord::from_admin_record(record, owner_user_id)
}

async fn query_all(
    repository: &UserTokenRepository,
    query: SelectStatement,
) -> Result<Vec<QueryResult>, UserTokenRepositoryError> {
    let connection = repository.pool.connection();
    let statement = connection.get_database_backend().build(&query);
    connection
        .query_all(statement)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(UserTokenRepositoryError::Query))
}
