//! User-owned links to external platform accounts and immutable balance observations.

use std::time::Duration;

use af_domain::UserId;
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, QueryResult, SqlErr, TransactionTrait,
    sea_query::{Alias, Expr, LockType, Order, Query},
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::time::timeout;
use url::Url;

use crate::{DatabasePool, EncryptedCredentialEnvelope};

const CONNECTIONS: &str = "external_account_connections";
const SNAPSHOTS: &str = "external_account_balance_snapshots";
pub const MAX_EXTERNAL_ACCOUNT_CONNECTIONS: u32 = 32;
/// Maximum number of balance snapshots retained for a linked account.
pub const MAX_EXTERNAL_BALANCE_SNAPSHOTS: u32 = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalAccountConnectionCreate {
    pub user_id: UserId,
    pub connector_id: String,
    pub external_subject: String,
    pub display_name: Option<String>,
    pub base_url: String,
    pub secret: EncryptedCredentialEnvelope,
    pub now: i64,
}

impl ExternalAccountConnectionCreate {
    pub fn validate(&self) -> Result<(), ExternalAccountRepositoryError> {
        let endpoint =
            Url::parse(&self.base_url).map_err(|_| ExternalAccountRepositoryError::InvalidInput)?;
        if self.connector_id.is_empty()
            || self.connector_id.len() > 128
            || !valid_identifier(&self.connector_id)
            || self.external_subject.is_empty()
            || self.external_subject.len() > 512
            || self.external_subject.trim() != self.external_subject
            || self.external_subject.chars().any(char::is_control)
            || self
                .display_name
                .as_ref()
                .is_some_and(|value| value.len() > 256 || value.chars().any(char::is_control))
            || self.base_url.is_empty()
            || self.base_url.len() > 2048
            || self.base_url.trim() != self.base_url
            || endpoint.scheme() != "https"
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || self.now <= 0
        {
            return Err(ExternalAccountRepositoryError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalAccountConnectionRecord {
    pub id: i64,
    pub user_id: UserId,
    pub connector_id: String,
    pub issuer_hash: String,
    pub external_subject: String,
    pub display_name: Option<String>,
    pub base_url: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Secret material is available only to the host service, never the user DTO.
pub struct ExternalAccountSecretRecord {
    pub connection: ExternalAccountConnectionRecord,
    pub envelope: EncryptedCredentialEnvelope,
}

impl std::fmt::Debug for ExternalAccountSecretRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExternalAccountSecretRecord")
            .field("connection_id", &self.connection.id)
            .field("connector_id", &self.connection.connector_id)
            .field("envelope", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalBalanceStatus {
    Available,
    Unavailable,
    AuthenticationRequired,
}

impl ExternalBalanceStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
            Self::AuthenticationRequired => "authentication_required",
        }
    }

    fn parse(value: &str) -> Result<Self, ExternalAccountRepositoryError> {
        match value {
            "available" => Ok(Self::Available),
            "unavailable" => Ok(Self::Unavailable),
            "authentication_required" => Ok(Self::AuthenticationRequired),
            _ => Err(ExternalAccountRepositoryError::Invariant),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalAccountBalanceSnapshotWrite {
    pub user_id: UserId,
    pub connection_id: i64,
    pub amount: Option<String>,
    pub unit: Option<String>,
    pub status: ExternalBalanceStatus,
    pub observed_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalAccountInitialBalanceSnapshot {
    pub amount: Option<String>,
    pub unit: Option<String>,
    pub status: ExternalBalanceStatus,
    pub observed_at: i64,
}

impl ExternalAccountBalanceSnapshotWrite {
    fn validate(&self) -> Result<(), ExternalAccountRepositoryError> {
        let amount_valid = self
            .amount
            .as_ref()
            .is_some_and(|amount| valid_decimal_amount(amount));
        let unit_valid = self.unit.as_ref().is_some_and(|unit| {
            !unit.is_empty()
                && unit.len() <= 64
                && unit.trim() == unit
                && !unit.chars().any(char::is_control)
        });
        let values_match_status = match self.status {
            ExternalBalanceStatus::Available => amount_valid && unit_valid,
            ExternalBalanceStatus::Unavailable | ExternalBalanceStatus::AuthenticationRequired => {
                self.amount.is_none() && self.unit.is_none()
            }
        };
        if self.connection_id <= 0 || self.observed_at <= 0 || !values_match_status {
            return Err(ExternalAccountRepositoryError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalAccountBalanceSnapshotRecord {
    pub id: i64,
    pub connection_id: i64,
    pub amount: Option<String>,
    pub unit: Option<String>,
    pub status: ExternalBalanceStatus,
    pub observed_at: i64,
}

#[derive(Clone)]
pub struct ExternalAccountConnectionRepository {
    pool: DatabasePool,
    timeout: Duration,
}

impl ExternalAccountConnectionRepository {
    #[must_use]
    pub const fn new(pool: DatabasePool, timeout: Duration) -> Self {
        Self { pool, timeout }
    }

    pub async fn create(
        &self,
        input: ExternalAccountConnectionCreate,
    ) -> Result<ExternalAccountConnectionRecord, ExternalAccountRepositoryError> {
        self.create_with_optional_snapshot(input, None).await
    }

    pub async fn create_with_snapshot(
        &self,
        input: ExternalAccountConnectionCreate,
        snapshot: ExternalAccountInitialBalanceSnapshot,
    ) -> Result<ExternalAccountConnectionRecord, ExternalAccountRepositoryError> {
        self.create_with_optional_snapshot(input, Some(snapshot))
            .await
    }

    async fn create_with_optional_snapshot(
        &self,
        input: ExternalAccountConnectionCreate,
        initial_snapshot: Option<ExternalAccountInitialBalanceSnapshot>,
    ) -> Result<ExternalAccountConnectionRecord, ExternalAccountRepositoryError> {
        input.validate()?;
        let canonical_issuer = canonical_issuer(&input.base_url)?;
        let issuer_hash = digest_issuer(&canonical_issuer);
        bounded(self.timeout, async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            // Serialize connection-count and per-provider replacement checks for this user.
            let mut lock_user = Query::select();
            lock_user
                .column(Alias::new("id"))
                .from(Alias::new("users"))
                .and_where(Expr::col(Alias::new("id")).eq(input.user_id.get()));
            if txn.get_database_backend() != DbBackend::Sqlite {
                lock_user.lock(LockType::Update);
            }
            let lock_user = lock_user.to_owned();
            if txn
                .query_one(txn.get_database_backend().build(&lock_user))
                .await
                .map_err(map_db)?
                .is_none()
            {
                return Err(ExternalAccountRepositoryError::NotFound);
            }

            let active_existing_query = connection_query()
                .and_where(Expr::col(Alias::new("user_id")).eq(input.user_id.get()))
                .and_where(Expr::col(Alias::new("connector_id")).eq(&input.connector_id))
                .and_where(Expr::col(Alias::new("base_url")).eq(&canonical_issuer))
                .and_where(Expr::col(Alias::new("active")).eq(true))
                .to_owned();
            if let Some(existing) = txn
                .query_one(txn.get_database_backend().build(&active_existing_query))
                .await
                .map_err(map_db)?
            {
                let external_subject: String =
                    existing.try_get("", "external_subject").map_err(map_db)?;
                if external_subject != input.external_subject {
                    // A subject change is a new link generation; revoke first so
                    // historical balance snapshots remain tied to the old subject.
                    return Err(ExternalAccountRepositoryError::Conflict);
                }
                let connection_id: i64 = existing.try_get("", "id").map_err(map_db)?;
                let update = Query::update()
                    .table(Alias::new(CONNECTIONS))
                    .value(Alias::new("display_name"), input.display_name.clone())
                    .value(Alias::new("base_url"), canonical_issuer.clone())
                    .value(
                        Alias::new("secret_key_id"),
                        input.secret.key_id().to_owned(),
                    )
                    .value(Alias::new("secret_nonce"), input.secret.nonce().to_vec())
                    .value(
                        Alias::new("secret_ciphertext"),
                        input.secret.ciphertext().to_vec(),
                    )
                    .value(Alias::new("updated_at"), input.now)
                    .and_where(Expr::col(Alias::new("id")).eq(connection_id))
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&update))
                    .await
                    .map_err(map_db)?;
                if let Some(snapshot) = &initial_snapshot {
                    insert_snapshot_in_transaction(&txn, input.user_id, connection_id, snapshot)
                        .await?;
                }
                let record = connection_from_row(&existing)?;
                txn.commit().await.map_err(map_db)?;
                return Ok(ExternalAccountConnectionRecord {
                    display_name: input.display_name,
                    base_url: canonical_issuer,
                    updated_at: input.now,
                    ..record
                });
            }

            let existing_query = connection_query()
                .and_where(Expr::col(Alias::new("user_id")).eq(input.user_id.get()))
                .and_where(Expr::col(Alias::new("connector_id")).eq(&input.connector_id))
                .and_where(Expr::col(Alias::new("base_url")).eq(&canonical_issuer))
                .and_where(Expr::col(Alias::new("external_subject")).eq(&input.external_subject))
                .and_where(Expr::col(Alias::new("active")).eq(false))
                .to_owned();
            if let Some(existing) = txn
                .query_one(txn.get_database_backend().build(&existing_query))
                .await
                .map_err(map_db)?
            {
                let active_connections = Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new(CONNECTIONS))
                    .and_where(Expr::col(Alias::new("user_id")).eq(input.user_id.get()))
                    .and_where(Expr::col(Alias::new("active")).eq(true))
                    .limit(u64::from(MAX_EXTERNAL_ACCOUNT_CONNECTIONS))
                    .to_owned();
                if txn
                    .query_all(txn.get_database_backend().build(&active_connections))
                    .await
                    .map_err(map_db)?
                    .len()
                    >= MAX_EXTERNAL_ACCOUNT_CONNECTIONS as usize
                {
                    return Err(ExternalAccountRepositoryError::LimitReached);
                }
                let connection_id: i64 = existing.try_get("", "id").map_err(map_db)?;
                claim_external_subject(
                    &txn,
                    &input.connector_id,
                    &issuer_hash,
                    &input.external_subject,
                    connection_id,
                )
                .await?;
                let update = Query::update()
                    .table(Alias::new(CONNECTIONS))
                    .value(Alias::new("display_name"), input.display_name.clone())
                    .value(Alias::new("base_url"), canonical_issuer.clone())
                    .value(
                        Alias::new("secret_key_id"),
                        input.secret.key_id().to_owned(),
                    )
                    .value(Alias::new("secret_nonce"), input.secret.nonce().to_vec())
                    .value(
                        Alias::new("secret_ciphertext"),
                        input.secret.ciphertext().to_vec(),
                    )
                    .value(Alias::new("active"), true)
                    .value(Alias::new("revoked_at"), Option::<i64>::None)
                    .value(Alias::new("updated_at"), input.now)
                    .and_where(Expr::col(Alias::new("id")).eq(connection_id))
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&update))
                    .await
                    .map_err(map_db)?;
                if let Some(snapshot) = &initial_snapshot {
                    insert_snapshot_in_transaction(&txn, input.user_id, connection_id, snapshot)
                        .await?;
                }
                let record = connection_from_row(&existing)?;
                txn.commit().await.map_err(map_db)?;
                return Ok(ExternalAccountConnectionRecord {
                    display_name: input.display_name,
                    base_url: canonical_issuer,
                    updated_at: input.now,
                    ..record
                });
            }

            let active_connections = Query::select()
                .column(Alias::new("id"))
                .from(Alias::new(CONNECTIONS))
                .and_where(Expr::col(Alias::new("user_id")).eq(input.user_id.get()))
                .and_where(Expr::col(Alias::new("active")).eq(true))
                .limit(u64::from(MAX_EXTERNAL_ACCOUNT_CONNECTIONS))
                .to_owned();
            if txn
                .query_all(txn.get_database_backend().build(&active_connections))
                .await
                .map_err(map_db)?
                .len()
                >= MAX_EXTERNAL_ACCOUNT_CONNECTIONS as usize
            {
                return Err(ExternalAccountRepositoryError::LimitReached);
            }

            let mut insert = Query::insert()
                .into_table(Alias::new(CONNECTIONS))
                .columns(
                    [
                        "user_id",
                        "connector_id",
                        "issuer_hash",
                        "external_subject",
                        "display_name",
                        "base_url",
                        "secret_key_id",
                        "secret_nonce",
                        "secret_ciphertext",
                        "created_at",
                        "updated_at",
                    ]
                    .map(Alias::new),
                )
                .values_panic([
                    input.user_id.get().into(),
                    input.connector_id.clone().into(),
                    issuer_hash.clone().into(),
                    input.external_subject.clone().into(),
                    input.display_name.clone().into(),
                    canonical_issuer.clone().into(),
                    input.secret.key_id().to_owned().into(),
                    input.secret.nonce().to_vec().into(),
                    input.secret.ciphertext().to_vec().into(),
                    input.now.into(),
                    input.now.into(),
                ])
                .to_owned();
            let id = if txn.get_database_backend() == DbBackend::Postgres {
                insert.returning(Query::returning().column(Alias::new("id")));
                txn.query_one(txn.get_database_backend().build(&insert))
                    .await
                    .map_err(map_db)?
                    .ok_or(ExternalAccountRepositoryError::Unavailable)?
                    .try_get::<i64>("", "id")
                    .map_err(map_db)?
            } else {
                i64::try_from(
                    txn.execute(txn.get_database_backend().build(&insert))
                        .await
                        .map_err(map_db)?
                        .last_insert_id(),
                )
                .map_err(|_| ExternalAccountRepositoryError::Invariant)?
            };
            if id <= 0 {
                return Err(ExternalAccountRepositoryError::Unavailable);
            }
            claim_external_subject(
                &txn,
                &input.connector_id,
                &issuer_hash,
                &input.external_subject,
                id,
            )
            .await?;
            if let Some(snapshot) = &initial_snapshot {
                insert_snapshot_in_transaction(&txn, input.user_id, id, snapshot).await?;
            }
            let record = ExternalAccountConnectionRecord {
                id,
                user_id: input.user_id,
                connector_id: input.connector_id,
                issuer_hash,
                external_subject: input.external_subject,
                display_name: input.display_name,
                base_url: canonical_issuer,
                created_at: input.now,
                updated_at: input.now,
            };
            txn.commit().await.map_err(map_db)?;
            Ok(record)
        })
        .await
    }

    pub async fn update_profile(
        &self,
        user_id: UserId,
        connection_id: i64,
        display_name: Option<String>,
        now: i64,
    ) -> Result<ExternalAccountConnectionRecord, ExternalAccountRepositoryError> {
        if connection_id <= 0
            || now <= 0
            || display_name
                .as_ref()
                .is_some_and(|name| name.len() > 256 || name.chars().any(char::is_control))
        {
            return Err(ExternalAccountRepositoryError::InvalidInput);
        }
        bounded(self.timeout, async {
            let query = Query::update()
                .table(Alias::new(CONNECTIONS))
                .value(Alias::new("display_name"), display_name.clone())
                .value(Alias::new("updated_at"), now)
                .and_where(Expr::col(Alias::new("user_id")).eq(user_id.get()))
                .and_where(Expr::col(Alias::new("id")).eq(connection_id))
                .and_where(Expr::col(Alias::new("active")).eq(true))
                .to_owned();
            let result = self
                .pool
                .connection()
                .execute(self.pool.connection().get_database_backend().build(&query))
                .await
                .map_err(map_db)?;
            if result.rows_affected() != 1 {
                return Err(ExternalAccountRepositoryError::NotFound);
            }
            let mut record = self.get(user_id, connection_id).await?;
            record.display_name = display_name;
            record.updated_at = now;
            Ok(record)
        })
        .await
    }

    pub async fn list(
        &self,
        user_id: UserId,
    ) -> Result<Vec<ExternalAccountConnectionRecord>, ExternalAccountRepositoryError> {
        bounded(self.timeout, async {
            let query = Query::select()
                .columns([
                    Alias::new("id"),
                    Alias::new("user_id"),
                    Alias::new("connector_id"),
                    Alias::new("issuer_hash"),
                    Alias::new("external_subject"),
                    Alias::new("display_name"),
                    Alias::new("base_url"),
                    Alias::new("created_at"),
                    Alias::new("updated_at"),
                ])
                .from(Alias::new(CONNECTIONS))
                .and_where(Expr::col(Alias::new("user_id")).eq(user_id.get()))
                .and_where(Expr::col(Alias::new("active")).eq(true))
                .order_by(Alias::new("id"), Order::Asc)
                .to_owned();
            self.pool
                .connection()
                .query_all(self.pool.connection().get_database_backend().build(&query))
                .await
                .map_err(map_db)?
                .iter()
                .map(connection_from_row)
                .collect()
        })
        .await
    }

    pub async fn get(
        &self,
        user_id: UserId,
        connection_id: i64,
    ) -> Result<ExternalAccountConnectionRecord, ExternalAccountRepositoryError> {
        bounded(self.timeout, async {
            let query = connection_query()
                .and_where(Expr::col(Alias::new("user_id")).eq(user_id.get()))
                .and_where(Expr::col(Alias::new("id")).eq(connection_id))
                .and_where(Expr::col(Alias::new("active")).eq(true))
                .to_owned();
            let row = self
                .pool
                .connection()
                .query_one(self.pool.connection().get_database_backend().build(&query))
                .await
                .map_err(map_db)?
                .ok_or(ExternalAccountRepositoryError::NotFound)?;
            connection_from_row(&row)
        })
        .await
    }

    pub async fn secret(
        &self,
        user_id: UserId,
        connection_id: i64,
    ) -> Result<ExternalAccountSecretRecord, ExternalAccountRepositoryError> {
        bounded(self.timeout, async {
            let query = connection_query()
                .columns([
                    Alias::new("secret_key_id"),
                    Alias::new("secret_nonce"),
                    Alias::new("secret_ciphertext"),
                ])
                .and_where(Expr::col(Alias::new("user_id")).eq(user_id.get()))
                .and_where(Expr::col(Alias::new("id")).eq(connection_id))
                .and_where(Expr::col(Alias::new("active")).eq(true))
                .to_owned();
            let row = self
                .pool
                .connection()
                .query_one(self.pool.connection().get_database_backend().build(&query))
                .await
                .map_err(map_db)?
                .ok_or(ExternalAccountRepositoryError::NotFound)?;
            let connection = connection_from_row(&row)?;
            let key_id: String = row.try_get("", "secret_key_id").map_err(map_db)?;
            let nonce: Vec<u8> = row.try_get("", "secret_nonce").map_err(map_db)?;
            let ciphertext: Vec<u8> = row.try_get("", "secret_ciphertext").map_err(map_db)?;
            let nonce: [u8; crate::CREDENTIAL_ENVELOPE_NONCE_BYTES] = nonce
                .try_into()
                .map_err(|_| ExternalAccountRepositoryError::Invariant)?;
            let envelope = EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
                .map_err(|_| ExternalAccountRepositoryError::Invariant)?;
            Ok(ExternalAccountSecretRecord {
                connection,
                envelope,
            })
        })
        .await
    }

    pub async fn delete(
        &self,
        user_id: UserId,
        connection_id: i64,
    ) -> Result<(), ExternalAccountRepositoryError> {
        self.revoke(user_id, connection_id, current_epoch_seconds()?)
            .await
    }

    pub async fn revoke(
        &self,
        user_id: UserId,
        connection_id: i64,
        revoked_at: i64,
    ) -> Result<(), ExternalAccountRepositoryError> {
        if revoked_at <= 0 {
            return Err(ExternalAccountRepositoryError::InvalidInput);
        }
        bounded(self.timeout, async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            let query = Query::update()
                .table(Alias::new(CONNECTIONS))
                .value(Alias::new("active"), false)
                .value(Alias::new("revoked_at"), revoked_at)
                .value(Alias::new("updated_at"), revoked_at)
                .value(Alias::new("secret_key_id"), Option::<String>::None)
                .value(Alias::new("secret_nonce"), Option::<Vec<u8>>::None)
                .value(Alias::new("secret_ciphertext"), Option::<Vec<u8>>::None)
                .and_where(Expr::col(Alias::new("user_id")).eq(user_id.get()))
                .and_where(Expr::col(Alias::new("id")).eq(connection_id))
                .and_where(Expr::col(Alias::new("active")).eq(true))
                .to_owned();
            let result = txn
                .execute(txn.get_database_backend().build(&query))
                .await
                .map_err(map_db)?;
            if result.rows_affected() != 1 {
                return Err(ExternalAccountRepositoryError::NotFound);
            }
            let release_claim = Query::delete()
                .from_table(Alias::new("external_account_subject_claims"))
                .and_where(Expr::col(Alias::new("connection_id")).eq(connection_id))
                .to_owned();
            txn.execute(txn.get_database_backend().build(&release_claim))
                .await
                .map_err(map_db)?;
            txn.commit().await.map_err(map_db)?;
            Ok(())
        })
        .await
    }

    pub async fn insert_snapshot(
        &self,
        input: ExternalAccountBalanceSnapshotWrite,
    ) -> Result<ExternalAccountBalanceSnapshotRecord, ExternalAccountRepositoryError> {
        input.validate()?;
        bounded(self.timeout, async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            let owner = Query::select()
                .column(Alias::new("id"))
                .from(Alias::new(CONNECTIONS))
                .and_where(Expr::col(Alias::new("id")).eq(input.connection_id))
                .and_where(Expr::col(Alias::new("user_id")).eq(input.user_id.get()))
                .and_where(Expr::col(Alias::new("active")).eq(true))
                .to_owned();
            if txn
                .query_one(txn.get_database_backend().build(&owner))
                .await
                .map_err(map_db)?
                .is_none()
            {
                return Err(ExternalAccountRepositoryError::NotFound);
            }
            let record = insert_snapshot_in_transaction(
                &txn,
                input.user_id,
                input.connection_id,
                &ExternalAccountInitialBalanceSnapshot {
                    amount: input.amount,
                    unit: input.unit,
                    status: input.status,
                    observed_at: input.observed_at,
                },
            )
            .await?;
            txn.commit().await.map_err(map_db)?;
            Ok(record)
        })
        .await
    }

    pub async fn list_snapshots(
        &self,
        user_id: UserId,
        connection_id: i64,
        limit: u32,
    ) -> Result<Vec<ExternalAccountBalanceSnapshotRecord>, ExternalAccountRepositoryError> {
        if limit == 0 || limit > MAX_EXTERNAL_BALANCE_SNAPSHOTS {
            return Err(ExternalAccountRepositoryError::InvalidInput);
        }
        bounded(self.timeout, async {
            let query = Query::select()
                .columns([
                    (Alias::new(SNAPSHOTS), Alias::new("id")),
                    (Alias::new(SNAPSHOTS), Alias::new("connection_id")),
                    (Alias::new(SNAPSHOTS), Alias::new("amount")),
                    (Alias::new(SNAPSHOTS), Alias::new("unit")),
                    (Alias::new(SNAPSHOTS), Alias::new("status")),
                    (Alias::new(SNAPSHOTS), Alias::new("observed_at")),
                ])
                .from(Alias::new(SNAPSHOTS))
                .inner_join(
                    Alias::new(CONNECTIONS),
                    Expr::col((Alias::new(SNAPSHOTS), Alias::new("connection_id")))
                        .equals((Alias::new(CONNECTIONS), Alias::new("id"))),
                )
                .and_where(
                    Expr::col((Alias::new(CONNECTIONS), Alias::new("user_id"))).eq(user_id.get()),
                )
                .and_where(Expr::col((Alias::new(CONNECTIONS), Alias::new("id"))).eq(connection_id))
                .order_by(
                    (Alias::new(SNAPSHOTS), Alias::new("observed_at")),
                    Order::Desc,
                )
                .order_by((Alias::new(SNAPSHOTS), Alias::new("id")), Order::Desc)
                .limit(u64::from(limit))
                .to_owned();
            self.pool
                .connection()
                .query_all(self.pool.connection().get_database_backend().build(&query))
                .await
                .map_err(map_db)?
                .iter()
                .map(snapshot_from_row)
                .collect()
        })
        .await
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ExternalAccountRepositoryError {
    #[error("external account request is invalid")]
    InvalidInput,
    #[error("external account was not found")]
    NotFound,
    #[error("external account conflicts with an existing binding")]
    Conflict,
    #[error("external account connection limit reached")]
    LimitReached,
    #[error("external account repository is unavailable")]
    Unavailable,
    #[error("external account record is inconsistent")]
    Invariant,
}

fn connection_query() -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("user_id"),
            Alias::new("connector_id"),
            Alias::new("issuer_hash"),
            Alias::new("external_subject"),
            Alias::new("display_name"),
            Alias::new("base_url"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .from(Alias::new(CONNECTIONS))
        .to_owned()
}

fn connection_from_row(
    row: &QueryResult,
) -> Result<ExternalAccountConnectionRecord, ExternalAccountRepositoryError> {
    let user_id = UserId::new(row.try_get("", "user_id").map_err(map_db)?)
        .map_err(|_| ExternalAccountRepositoryError::Invariant)?;
    Ok(ExternalAccountConnectionRecord {
        id: row.try_get("", "id").map_err(map_db)?,
        user_id,
        connector_id: row.try_get("", "connector_id").map_err(map_db)?,
        issuer_hash: row.try_get("", "issuer_hash").map_err(map_db)?,
        external_subject: row.try_get("", "external_subject").map_err(map_db)?,
        display_name: row.try_get("", "display_name").map_err(map_db)?,
        base_url: row.try_get("", "base_url").map_err(map_db)?,
        created_at: row.try_get("", "created_at").map_err(map_db)?,
        updated_at: row.try_get("", "updated_at").map_err(map_db)?,
    })
}

fn snapshot_from_row(
    row: &QueryResult,
) -> Result<ExternalAccountBalanceSnapshotRecord, ExternalAccountRepositoryError> {
    let status: String = row.try_get("", "status").map_err(map_db)?;
    Ok(ExternalAccountBalanceSnapshotRecord {
        id: row.try_get("", "id").map_err(map_db)?,
        connection_id: row.try_get("", "connection_id").map_err(map_db)?,
        amount: row.try_get("", "amount").map_err(map_db)?,
        unit: row.try_get("", "unit").map_err(map_db)?,
        status: ExternalBalanceStatus::parse(&status)?,
        observed_at: row.try_get("", "observed_at").map_err(map_db)?,
    })
}

async fn insert_snapshot_in_transaction(
    txn: &DatabaseTransaction,
    user_id: UserId,
    connection_id: i64,
    snapshot: &ExternalAccountInitialBalanceSnapshot,
) -> Result<ExternalAccountBalanceSnapshotRecord, ExternalAccountRepositoryError> {
    ExternalAccountBalanceSnapshotWrite {
        user_id,
        connection_id,
        amount: snapshot.amount.clone(),
        unit: snapshot.unit.clone(),
        status: snapshot.status,
        observed_at: snapshot.observed_at,
    }
    .validate()?;
    let mut insert = Query::insert()
        .into_table(Alias::new(SNAPSHOTS))
        .columns(["connection_id", "amount", "unit", "status", "observed_at"].map(Alias::new))
        .values_panic([
            connection_id.into(),
            snapshot.amount.clone().into(),
            snapshot.unit.clone().into(),
            snapshot.status.as_str().into(),
            snapshot.observed_at.into(),
        ])
        .to_owned();
    let id = if txn.get_database_backend() == DbBackend::Postgres {
        insert.returning(Query::returning().column(Alias::new("id")));
        txn.query_one(txn.get_database_backend().build(&insert))
            .await
            .map_err(map_db)?
            .ok_or(ExternalAccountRepositoryError::Unavailable)?
            .try_get::<i64>("", "id")
            .map_err(map_db)?
    } else {
        i64::try_from(
            txn.execute(txn.get_database_backend().build(&insert))
                .await
                .map_err(map_db)?
                .last_insert_id(),
        )
        .map_err(|_| ExternalAccountRepositoryError::Invariant)?
    };
    let retained_snapshots = Query::select()
        .column(Alias::new("id"))
        .from(Alias::new(SNAPSHOTS))
        .and_where(Expr::col(Alias::new("connection_id")).eq(connection_id))
        .order_by(Alias::new("observed_at"), Order::Desc)
        .order_by(Alias::new("id"), Order::Desc)
        .limit(u64::from(MAX_EXTERNAL_BALANCE_SNAPSHOTS))
        .to_owned();
    let retained_ids = txn
        .query_all(txn.get_database_backend().build(&retained_snapshots))
        .await
        .map_err(map_db)?
        .iter()
        .map(|row| row.try_get::<i64>("", "id").map_err(map_db))
        .collect::<Result<Vec<_>, _>>()?;
    let prune_snapshots = Query::delete()
        .from_table(Alias::new(SNAPSHOTS))
        .and_where(Expr::col(Alias::new("connection_id")).eq(connection_id))
        .and_where(Expr::col(Alias::new("id")).is_not_in(retained_ids))
        .to_owned();
    txn.execute(txn.get_database_backend().build(&prune_snapshots))
        .await
        .map_err(map_db)?;
    Ok(ExternalAccountBalanceSnapshotRecord {
        id,
        connection_id,
        amount: snapshot.amount.clone(),
        unit: snapshot.unit.clone(),
        status: snapshot.status,
        observed_at: snapshot.observed_at,
    })
}

async fn claim_external_subject(
    connection: &DatabaseTransaction,
    connector_id: &str,
    issuer_hash: &str,
    external_subject: &str,
    connection_id: i64,
) -> Result<(), ExternalAccountRepositoryError> {
    let query = Query::insert()
        .into_table(Alias::new("external_account_subject_claims"))
        .columns(
            [
                "connector_id",
                "issuer_hash",
                "external_subject",
                "connection_id",
            ]
            .map(Alias::new),
        )
        .values_panic([
            connector_id.to_owned().into(),
            issuer_hash.to_owned().into(),
            external_subject.to_owned().into(),
            connection_id.into(),
        ])
        .to_owned();
    connection
        .execute(connection.get_database_backend().build(&query))
        .await
        .map_err(map_db)?;
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    value.split('.').all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && segment.as_bytes()[0].is_ascii_alphanumeric()
            && segment.as_bytes()[segment.len() - 1].is_ascii_alphanumeric()
    })
}

fn canonical_issuer(value: &str) -> Result<String, ExternalAccountRepositoryError> {
    let endpoint = Url::parse(value).map_err(|_| ExternalAccountRepositoryError::InvalidInput)?;
    if endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(ExternalAccountRepositoryError::InvalidInput);
    }
    Ok(endpoint.to_string().trim_end_matches('/').to_owned())
}

fn digest_issuer(issuer: &str) -> String {
    format!("{:x}", Sha256::digest(issuer.as_bytes()))
}

fn valid_decimal_amount(value: &str) -> bool {
    if value.is_empty() || value.len() > 128 {
        return false;
    }
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(whole, fraction)| (whole, Some(fraction)));
    let whole_valid = whole == "0"
        || (!whole.is_empty()
            && !whole.starts_with('0')
            && whole.bytes().all(|byte| byte.is_ascii_digit()));
    whole_valid
        && fraction.is_none_or(|fraction| {
            !fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
}

async fn bounded<T>(
    timeout_duration: Duration,
    operation: impl std::future::Future<Output = Result<T, ExternalAccountRepositoryError>>,
) -> Result<T, ExternalAccountRepositoryError> {
    timeout(timeout_duration, operation)
        .await
        .map_err(|_| ExternalAccountRepositoryError::Unavailable)?
}

fn map_db(error: sea_orm::DbErr) -> ExternalAccountRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        ExternalAccountRepositoryError::Conflict
    } else {
        ExternalAccountRepositoryError::Unavailable
    }
}

fn current_epoch_seconds() -> Result<i64, ExternalAccountRepositoryError> {
    Ok(time::OffsetDateTime::now_utc().unix_timestamp())
}
