use af_domain::{TrustedClientIp, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    IntoActiveModel, QueryFilter, QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, LockType, OnConflict},
};
use sha2::{Digest, Sha256};

use super::{
    PersonalAccessTokenRepository as Repository, PersonalAccessTokenRepositoryError as Error,
};
use crate::entity::access_token_rate_limits as limits;

pub(super) async fn ip(repository: &Repository, address: TrustedClientIp) -> Result<(), Error> {
    let mut bytes = [0_u8; 17];
    let len = address.write_fingerprint_input(&mut bytes);
    admit(repository, vec![(subject(b"ip", &bytes[..len]), 600)]).await
}

pub(super) async fn identity(
    repository: &Repository,
    user: UserId,
    token: i64,
) -> Result<(), Error> {
    if token <= 0 {
        return Err(Error::InvalidInput);
    }
    admit(
        repository,
        vec![
            (subject(b"user", &user.get().to_be_bytes()), 300),
            (subject(b"token", &token.to_be_bytes()), 120),
        ],
    )
    .await
}

async fn admit(repository: &Repository, mut subjects: Vec<(String, i32)>) -> Result<(), Error> {
    subjects.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let window = now - now.rem_euclid(60);
    let retry_after = u32::try_from(window + 60 - now).map_err(|_| Error::Invariant)?;
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| Error::Query)?;
    // The first SQLite statement must acquire its writer lock before any read snapshot.
    for (key, cap) in subjects {
        seed(&transaction, &key, window).await?;
        let mut query = limits::Entity::find_by_id(key.clone());
        if transaction.get_database_backend() != DbBackend::Sqlite {
            query = query.lock(LockType::Update);
        }
        let current = query
            .one(&transaction)
            .await
            .map_err(|_| Error::Query)?
            .ok_or(Error::Invariant)?;
        if current.used_requests < 0 || current.window_started_at > window {
            return Err(Error::Invariant);
        }
        let used = if current.window_started_at == window {
            current.used_requests
        } else {
            0
        };
        if used >= cap {
            return Err(Error::RateLimited { retry_after });
        }
        let mut model = current.into_active_model();
        model.window_started_at = Set(window);
        model.used_requests = Set(used + 1);
        model.expires_at = Set(window + 120);
        model.update(&transaction).await.map_err(|_| Error::Query)?;
    }
    transaction.commit().await.map_err(|_| Error::Query)?;
    // Cleanup holds no counter locks, cannot roll back admission, and never delays a
    // successful request beyond this small best-effort budget.
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        cleanup_expired(repository, now),
    )
    .await;
    Ok(())
}

async fn cleanup_expired(repository: &Repository, now: i64) -> Result<(), Error> {
    let expired = limits::Entity::find()
        .select_only()
        .column(limits::Column::SubjectKey)
        .filter(limits::Column::ExpiresAt.lt(now))
        .limit(64)
        .into_tuple::<String>()
        .all(repository.pool.connection())
        .await
        .map_err(|_| Error::Query)?;
    delete_expired(repository, expired, now).await
}

pub(super) async fn delete_expired(
    repository: &Repository,
    candidates: Vec<String>,
    now: i64,
) -> Result<(), Error> {
    if !candidates.is_empty() {
        // A request may refresh a selected row before this statement reaches it.
        limits::Entity::delete_many()
            .filter(limits::Column::SubjectKey.is_in(candidates))
            .filter(limits::Column::ExpiresAt.lt(now))
            .exec(repository.pool.connection())
            .await
            .map_err(|_| Error::Query)?;
    }
    Ok(())
}

async fn seed(transaction: &DatabaseTransaction, key: &str, window: i64) -> Result<(), Error> {
    limits::Entity::insert(limits::ActiveModel {
        subject_key: Set(key.to_owned()),
        window_started_at: Set(window),
        used_requests: Set(0),
        expires_at: Set(window + 120),
    })
    .on_conflict(
        OnConflict::column(limits::Column::SubjectKey)
            .do_nothing_on([limits::Column::SubjectKey])
            .to_owned(),
    )
    .exec_without_returning(transaction)
    .await
    .map_err(|_| Error::Query)?;
    if transaction.get_database_backend() == DbBackend::Sqlite {
        limits::Entity::update_many()
            .filter(limits::Column::SubjectKey.eq(key))
            .col_expr(
                limits::Column::UsedRequests,
                Expr::col(limits::Column::UsedRequests).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| Error::Query)?;
    }
    Ok(())
}

fn subject(kind: &[u8], value: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"anyflows:personal-access-token:rate-limit:v1\0");
    hash.update(kind);
    hash.update([0]);
    hash.update(value);
    format!("{:x}", hash.finalize())
}
