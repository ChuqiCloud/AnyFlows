//! Durable, once-only payout authorization and reconciliation. No plaintext secrets.
use crate::merchant_finance::{
    all, append_entry, checked_add, db_error, get, load_account, now, save_account, select,
    withdrawal_by_id,
};
use crate::{DatabasePool, EncryptedCredentialEnvelope, MerchantFinanceError, MerchantFinancePage};
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Cond, Expr, LockType, OnConflict, Order, Query},
};
use serde::{Deserialize, Serialize};
use std::{future::Future, time::Duration};

const SETTINGS: &str = "merchant_payout_settings";
const ACCOUNTS: &str = "merchant_payout_accounts";
const PAYOUTS: &str = "merchant_payouts";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantPayoutConfig {
    pub enabled: bool,
    pub auto_approve_enabled: bool,
    pub gateway_url: String,
    pub currency: String,
    pub quota_per_unit: i64,
    pub min_amount_minor: i64,
    pub max_amount_minor: i64,
    pub daily_limit_minor: i64,
}
#[derive(Clone)]
pub struct MerchantPayoutSettings {
    pub version: i64,
    pub config: MerchantPayoutConfig,
    pub secret: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct MerchantPayoutAccount {
    pub configured: bool,
    pub label: String,
    pub version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MerchantPayout {
    pub withdrawal_id: i64,
    pub merchant_id: i64,
    pub event_key: String,
    pub status: String,
    pub amount_minor: i64,
    pub currency: String,
    pub reference: String,
    pub reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
pub struct MerchantPayoutClaim {
    pub payout: MerchantPayout,
    pub submit: bool,
    pub initiator_user_id: Option<i64>,
    pub initiator_access_token_id: Option<i64>,
    pub initiator_request_id: Option<String>,
    pub initiator_scope: Option<String>,
    pub gateway_url: String,
    pub gateway_secret: String,
    pub account_secret: String,
    pub lease_owner: String,
}

/// The administrator identity accepted with a manual payout request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantPayoutInitiator {
    user_id: i64,
    access_token_id: Option<i64>,
    request_id: String,
    scope: Option<String>,
}

impl MerchantPayoutInitiator {
    pub fn new(
        user_id: i64,
        access_token_id: Option<i64>,
        request_id: String,
        scope: Option<String>,
    ) -> Result<Self, MerchantFinanceError> {
        if user_id <= 0
            || access_token_id.is_some_and(|id| id <= 0)
            || request_id.is_empty()
            || request_id.len() > 128
            || request_id.chars().any(char::is_control)
            || scope.as_deref().is_some_and(|value| {
                value.is_empty() || value.len() > 128 || value.chars().any(char::is_control)
            })
        {
            return Err(MerchantFinanceError::InvalidInput);
        }
        Ok(Self {
            user_id,
            access_token_id,
            request_id,
            scope,
        })
    }
}
#[derive(Clone)]
pub struct MerchantPayoutRepository {
    pool: DatabasePool,
    timeout: Duration,
}

// Serialized envelopes are never public DTOs; nonce and ciphertext remain opaque.
pub fn encode_payout_secret(
    secret: &EncryptedCredentialEnvelope,
) -> Result<String, MerchantFinanceError> {
    serde_json::to_string(&(secret.key_id(), secret.nonce(), secret.ciphertext()))
        .map_err(|_| MerchantFinanceError::Invariant)
}
pub fn decode_payout_secret(
    value: &str,
) -> Result<EncryptedCredentialEnvelope, MerchantFinanceError> {
    let (key, nonce, ciphertext): (String, [u8; 24], Vec<u8>) =
        serde_json::from_str(value).map_err(|_| MerchantFinanceError::Invariant)?;
    EncryptedCredentialEnvelope::new(key, nonce, ciphertext)
        .map_err(|_| MerchantFinanceError::Invariant)
}

impl MerchantPayoutRepository {
    pub const fn new(pool: DatabasePool, timeout: Duration) -> Self {
        Self { pool, timeout }
    }
    async fn bounded<T>(
        &self,
        operation: impl Future<Output = Result<T, MerchantFinanceError>>,
    ) -> Result<T, MerchantFinanceError> {
        tokio::time::timeout(self.timeout, operation)
            .await
            .map_err(|_| MerchantFinanceError::Unavailable)?
    }
    pub async fn settings(&self) -> Result<MerchantPayoutSettings, MerchantFinanceError> {
        self.bounded(read_settings(self.pool.connection())).await
    }
    pub async fn save_settings(
        &self,
        expected: i64,
        config: MerchantPayoutConfig,
        secret: Option<String>,
        clear: bool,
    ) -> Result<MerchantPayoutSettings, MerchantFinanceError> {
        self.bounded(async {
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            lock_settings(&tx).await?;
            let previous = read_settings(&tx).await?;
            if previous.version != expected {
                return Err(MerchantFinanceError::Conflict);
            }
            let secret = if clear {
                None
            } else {
                secret.or(previous.secret)
            };
            if config.enabled && secret.is_none() {
                return Err(MerchantFinanceError::InvalidInput);
            }
            let version = expected
                .checked_add(1)
                .ok_or(MerchantFinanceError::Invariant)?;
            let query = Query::update()
                .table(Alias::new(SETTINGS))
                .values([
                    (Alias::new("version"), version.into()),
                    (
                        Alias::new("config_json"),
                        serde_json::to_string(&config)
                            .map_err(|_| MerchantFinanceError::Invariant)?
                            .into(),
                    ),
                    (Alias::new("secret_json"), secret.clone().into()),
                ])
                .and_where(Expr::col(Alias::new("id")).eq(1_i16))
                .to_owned();
            tx.execute(tx.get_database_backend().build(&query))
                .await
                .map_err(db_error)?;
            tx.commit().await.map_err(db_error)?;
            Ok(MerchantPayoutSettings {
                version,
                config,
                secret,
            })
        })
        .await
    }
    pub async fn account(
        &self,
        merchant: i64,
    ) -> Result<MerchantPayoutAccount, MerchantFinanceError> {
        self.bounded(async {
            let row = account_row(self.pool.connection(), merchant).await?;
            row.as_ref().map(account_from_row).transpose().map(|value| {
                value.unwrap_or(MerchantPayoutAccount {
                    configured: false,
                    label: String::new(),
                    version: 0,
                })
            })
        })
        .await
    }
    pub async fn save_account(
        &self,
        merchant: i64,
        expected: i64,
        label: String,
        secret: Option<String>,
    ) -> Result<MerchantPayoutAccount, MerchantFinanceError> {
        self.bounded(async {
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            lock_settings(&tx).await?;
            let old = account_row(&tx, merchant).await?;
            let version = old
                .as_ref()
                .map(|row| get::<i64>(row, "version"))
                .transpose()?
                .unwrap_or(0);
            if version != expected {
                return Err(MerchantFinanceError::Conflict);
            }
            let version = checked_add(version, 1)?;
            let query = Query::insert()
                .into_table(Alias::new(ACCOUNTS))
                .columns(["merchant_id", "version", "label", "secret_json"].map(Alias::new))
                .values_panic([
                    merchant.into(),
                    version.into(),
                    label.clone().into(),
                    secret.clone().into(),
                ])
                .on_conflict(
                    OnConflict::column(Alias::new("merchant_id"))
                        .update_columns(["version", "label", "secret_json"].map(Alias::new))
                        .to_owned(),
                )
                .to_owned();
            tx.execute(tx.get_database_backend().build(&query))
                .await
                .map_err(db_error)?;
            tx.commit().await.map_err(db_error)?;
            Ok(MerchantPayoutAccount {
                configured: secret.is_some(),
                label,
                version,
            })
        })
        .await
    }
    /// Serializes aggregate daily authorizations and snapshots secrets before any IO.
    pub async fn enqueue(
        &self,
        withdrawal: i64,
        automatic: bool,
    ) -> Result<MerchantPayout, MerchantFinanceError> {
        self.enqueue_with_initiator(withdrawal, automatic, None)
            .await
    }

    /// Manual API requests retain their accepted actor for execution-time authorization.
    pub async fn enqueue_with_initiator(
        &self,
        withdrawal: i64,
        automatic: bool,
        initiator: Option<&MerchantPayoutInitiator>,
    ) -> Result<MerchantPayout, MerchantFinanceError> {
        self.bounded(async {
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            lock_settings(&tx).await?;
            if let Some(initiator) = initiator
                && !current_admin(&tx, initiator.user_id).await?
            {
                return Err(MerchantFinanceError::Forbidden);
            }
            let settings = read_settings(&tx).await?;
            let config = &settings.config;
            if !config.enabled || (automatic && !config.auto_approve_enabled) {
                return Err(MerchantFinanceError::Conflict);
            }
            let observed = withdrawal_by_id(&tx, withdrawal, false)
                .await?
                .ok_or(MerchantFinanceError::NotFound)?;
            let account = load_account(&tx, observed.merchant_id, true).await?;
            let current = withdrawal_by_id(&tx, withdrawal, true)
                .await?
                .ok_or(MerchantFinanceError::NotFound)?;
            if let Some(row) = payout_row(&tx, withdrawal).await? {
                return payout_from_row(&row);
            }
            if current.status != "pending"
                || account.debt_quota != 0
                || account.frozen_quota != 0
                || account.withdrawing_quota < current.amount_quota
            {
                return Err(MerchantFinanceError::Conflict);
            }
            let profile = select("merchant_profiles")
                .and_where(Expr::col(Alias::new("id")).eq(current.merchant_id))
                .to_owned();
            let profile = tx
                .query_one(tx.get_database_backend().build(&profile))
                .await
                .map_err(db_error)?
                .ok_or(MerchantFinanceError::NotFound)?;
            if get::<String>(&profile, "status")? != "approved" {
                return Err(MerchantFinanceError::Forbidden);
            }
            let scaled = i128::from(current.amount_quota) * 100;
            if config.quota_per_unit <= 0 || scaled % i128::from(config.quota_per_unit) != 0 {
                return Err(MerchantFinanceError::InvalidInput);
            }
            let amount = i64::try_from(scaled / i128::from(config.quota_per_unit))
                .map_err(|_| MerchantFinanceError::InvalidInput)?;
            if amount < config.min_amount_minor || amount > config.max_amount_minor {
                return Err(MerchantFinanceError::InvalidInput);
            }
            let at = now();
            let mut daily = select(PAYOUTS);
            daily
                .cond_where(
                    Cond::any()
                        .add(Expr::col(Alias::new("created_at")).gte(at - at.rem_euclid(86_400)))
                        .add(Expr::col(Alias::new("status")).is_in([
                            "pending",
                            "processing",
                            "unknown",
                        ])),
                )
                .and_where(Expr::col(Alias::new("currency")).eq(config.currency.clone()))
                .and_where(Expr::col(Alias::new("status")).ne("failed"));
            let total =
                all(&tx, daily)
                    .await?
                    .iter()
                    .try_fold(i128::from(amount), |sum, row| {
                        Ok::<_, MerchantFinanceError>(
                            sum + i128::from(get::<i64>(row, "amount_minor")?),
                        )
                    })?;
            if total > i128::from(config.daily_limit_minor) {
                return Err(MerchantFinanceError::InsufficientBalance);
            }
            let destination = account_row(&tx, current.merchant_id)
                .await?
                .ok_or(MerchantFinanceError::InvalidInput)?;
            let account_secret = get::<Option<String>>(&destination, "secret_json")?
                .ok_or(MerchantFinanceError::InvalidInput)?;
            let gateway_secret = settings.secret.ok_or(MerchantFinanceError::InvalidInput)?;
            let insert = Query::insert()
                .into_table(Alias::new(PAYOUTS))
                .columns(
                    [
                        "withdrawal_id",
                        "merchant_id",
                        "event_key",
                        "status",
                        "amount_minor",
                        "currency",
                        "gateway_url",
                        "gateway_secret",
                        "account_secret",
                        "initiated_by_user_id",
                        "initiated_by_access_token_id",
                        "initiated_request_id",
                        "initiated_scope",
                        "created_at",
                        "updated_at",
                        "next_at",
                    ]
                    .map(Alias::new),
                )
                .values_panic([
                    withdrawal.into(),
                    current.merchant_id.into(),
                    current.event_key.into(),
                    "pending".into(),
                    amount.into(),
                    config.currency.clone().into(),
                    config.gateway_url.clone().into(),
                    gateway_secret.into(),
                    account_secret.into(),
                    initiator.map(|actor| actor.user_id).into(),
                    initiator.and_then(|actor| actor.access_token_id).into(),
                    initiator.map(|actor| actor.request_id.clone()).into(),
                    initiator.and_then(|actor| actor.scope.clone()).into(),
                    at.into(),
                    at.into(),
                    at.into(),
                ])
                .to_owned();
            tx.execute(tx.get_database_backend().build(&insert))
                .await
                .map_err(db_error)?;
            let result = payout_from_row(
                &payout_row(&tx, withdrawal)
                    .await?
                    .ok_or(MerchantFinanceError::Invariant)?,
            )?;
            tx.commit().await.map_err(db_error)?;
            Ok(result)
        })
        .await
    }
    pub async fn pending_withdrawals(&self, after: i64) -> Result<Vec<i64>, MerchantFinanceError> {
        self.bounded(async {
            let mut query = select("merchant_withdrawals");
            let queued = Query::select()
                .column(Alias::new("withdrawal_id"))
                .from(Alias::new(PAYOUTS))
                .to_owned();
            query
                .and_where(Expr::col(Alias::new("status")).eq("pending"))
                .and_where(Expr::col(Alias::new("id")).gt(after))
                .and_where(Expr::col(Alias::new("id")).not_in_subquery(queued))
                .order_by(Alias::new("id"), Order::Asc)
                .limit(100);
            all(self.pool.connection(), query)
                .await?
                .iter()
                .map(|row| get(row, "id"))
                .collect()
        })
        .await
    }
    pub async fn payouts(
        &self,
        merchant: Option<i64>,
        limit: u32,
        before: Option<i64>,
    ) -> Result<MerchantFinancePage<MerchantPayout>, MerchantFinanceError> {
        if !(1..=100).contains(&limit) {
            return Err(MerchantFinanceError::InvalidInput);
        }
        self.bounded(async {
            let mut query = select(PAYOUTS);
            if let Some(id) = merchant {
                query.and_where(Expr::col(Alias::new("merchant_id")).eq(id));
            }
            if let Some(id) = before {
                query.and_where(Expr::col(Alias::new("withdrawal_id")).lt(id));
            }
            query
                .order_by(Alias::new("withdrawal_id"), Order::Desc)
                .limit(u64::from(limit) + 1);
            let mut items = all(self.pool.connection(), query)
                .await?
                .iter()
                .map(payout_from_row)
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = if items.len() > limit as usize {
                items.pop();
                items.last().map(|item| item.withdrawal_id)
            } else {
                None
            };
            Ok(MerchantFinancePage { items, next_cursor })
        })
        .await
    }
    pub async fn reconcile(&self, id: i64) -> Result<(), MerchantFinanceError> {
        let query = Query::update()
            .table(Alias::new(PAYOUTS))
            .value(Alias::new("next_at"), now())
            .and_where(Expr::col(Alias::new("withdrawal_id")).eq(id))
            .and_where(Expr::col(Alias::new("status")).is_in(["processing", "unknown"]))
            .to_owned();
        let result = self
            .pool
            .connection()
            .execute(self.pool.connection().get_database_backend().build(&query))
            .await
            .map_err(db_error)?;
        if result.rows_affected() == 0 {
            return Err(MerchantFinanceError::Conflict);
        }
        Ok(())
    }
    pub async fn claim(
        &self,
        owner: &str,
    ) -> Result<Option<MerchantPayoutClaim>, MerchantFinanceError> {
        if owner.is_empty()
            || owner.len() > 64
            || !owner
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || value == b'-')
        {
            return Err(MerchantFinanceError::InvalidInput);
        }
        self.bounded(async {
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            lock_settings(&tx).await?;
            let enabled = read_settings(&tx).await?.config.enabled;
            let at = now();
            let mut query = select(PAYOUTS);
            query
                .and_where(Expr::col(Alias::new("status")).is_in(if enabled {
                    vec!["pending", "processing", "unknown"]
                } else {
                    vec!["processing", "unknown"]
                }))
                .and_where(Expr::col(Alias::new("next_at")).lte(at))
                .and_where(Expr::col(Alias::new("lease_until")).lte(at))
                .order_by(Alias::new("next_at"), Order::Asc)
                .limit(1)
                .lock(LockType::Update);
            let Some(row) = tx
                .query_one(tx.get_database_backend().build(&query))
                .await
                .map_err(db_error)?
            else {
                return Ok(None);
            };
            let payout = payout_from_row(&row)?;
            let submit = payout.status == "pending";
            if submit {
                let initiator_user_id = get::<Option<i64>>(&row, "initiated_by_user_id")?;
                if let Some(user_id) = initiator_user_id
                    && !current_admin(&tx, user_id).await?
                {
                    cancel_unsubmitted_payout(&tx, &payout).await?;
                    tx.commit().await.map_err(db_error)?;
                    return Ok(None);
                }
                let account = load_account(&tx, payout.merchant_id, true).await?;
                let withdrawal = withdrawal_by_id(&tx, payout.withdrawal_id, true)
                    .await?
                    .ok_or(MerchantFinanceError::Invariant)?;
                if account.debt_quota != 0
                    || account.frozen_quota != 0
                    || withdrawal.status != "pending"
                    || account.withdrawing_quota < withdrawal.amount_quota
                {
                    let postpone = Query::update()
                        .table(Alias::new(PAYOUTS))
                        .value(Alias::new("next_at"), at + 60)
                        .and_where(Expr::col(Alias::new("withdrawal_id")).eq(payout.withdrawal_id))
                        .to_owned();
                    tx.execute(tx.get_database_backend().build(&postpone))
                        .await
                        .map_err(db_error)?;
                    tx.commit().await.map_err(db_error)?;
                    return Ok(None);
                }
            }
            let update = Query::update()
                .table(Alias::new(PAYOUTS))
                .values([
                    (Alias::new("status"), "processing".into()),
                    (Alias::new("lease_owner"), owner.into()),
                    (Alias::new("lease_until"), (at + 120).into()),
                    (Alias::new("updated_at"), at.into()),
                ])
                .and_where(Expr::col(Alias::new("withdrawal_id")).eq(payout.withdrawal_id))
                .and_where(Expr::col(Alias::new("lease_until")).lte(at))
                .to_owned();
            if tx
                .execute(tx.get_database_backend().build(&update))
                .await
                .map_err(db_error)?
                .rows_affected()
                != 1
            {
                return Ok(None);
            }
            let claim = MerchantPayoutClaim {
                payout,
                submit,
                initiator_user_id: get(&row, "initiated_by_user_id")?,
                initiator_access_token_id: get(&row, "initiated_by_access_token_id")?,
                initiator_request_id: get(&row, "initiated_request_id")?,
                initiator_scope: get(&row, "initiated_scope")?,
                gateway_url: get(&row, "gateway_url")?,
                gateway_secret: get(&row, "gateway_secret")?,
                account_secret: get(&row, "account_secret")?,
                lease_owner: owner.to_owned(),
            };
            tx.commit().await.map_err(db_error)?;
            Ok(Some(claim))
        })
        .await
    }

    /// Recheck the live actor and lease immediately before the first gateway submission.
    pub async fn authorize_first_submit(
        &self,
        claim: &MerchantPayoutClaim,
    ) -> Result<bool, MerchantFinanceError> {
        if !claim.submit {
            return Err(MerchantFinanceError::InvalidInput);
        }
        self.bounded(async {
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            lock_settings(&tx).await?;
            let mut query = select(PAYOUTS);
            query.and_where(Expr::col(Alias::new("withdrawal_id")).eq(claim.payout.withdrawal_id));
            if tx.get_database_backend() != DbBackend::Sqlite {
                query.lock(LockType::Update);
            }
            let Some(row) = tx
                .query_one(tx.get_database_backend().build(&query.to_owned()))
                .await
                .map_err(db_error)?
            else {
                tx.commit().await.map_err(db_error)?;
                return Ok(false);
            };
            let payout = payout_from_row(&row)?;
            if payout.merchant_id != claim.payout.merchant_id
                || payout.event_key != claim.payout.event_key
                || payout.amount_minor != claim.payout.amount_minor
                || payout.currency != claim.payout.currency
            {
                return Err(MerchantFinanceError::Conflict);
            }
            if payout.status != "processing"
                || get::<String>(&row, "lease_owner")? != claim.lease_owner
                || get::<i64>(&row, "lease_until")? <= now()
                || get::<Option<i64>>(&row, "submit_authorized_at")?.is_some()
            {
                tx.commit().await.map_err(db_error)?;
                return Ok(false);
            }
            if let Some(initiator_user_id) = get::<Option<i64>>(&row, "initiated_by_user_id")?
                && !current_admin(&tx, initiator_user_id).await?
            {
                cancel_unsubmitted_payout(&tx, &payout).await?;
                tx.commit().await.map_err(db_error)?;
                return Ok(false);
            }
            let at = now();
            let authorize = Query::update()
                .table(Alias::new(PAYOUTS))
                .values([
                    (Alias::new("submit_authorized_at"), at.into()),
                    (Alias::new("updated_at"), at.into()),
                ])
                .and_where(Expr::col(Alias::new("withdrawal_id")).eq(payout.withdrawal_id))
                .and_where(Expr::col(Alias::new("status")).eq("processing"))
                .and_where(Expr::col(Alias::new("lease_owner")).eq(claim.lease_owner.clone()))
                .and_where(Expr::col(Alias::new("lease_until")).gt(at))
                .and_where(Expr::col(Alias::new("submit_authorized_at")).is_null())
                .to_owned();
            let authorized = tx
                .execute(tx.get_database_backend().build(&authorize))
                .await
                .map_err(db_error)?
                .rows_affected()
                == 1;
            tx.commit().await.map_err(db_error)?;
            Ok(authorized)
        })
        .await
    }

    /// A verified external terminal outcome and the finance ledger commit together.
    pub async fn complete(
        &self,
        claim: &MerchantPayoutClaim,
        status: &str,
        reference: &str,
        reason: Option<&str>,
    ) -> Result<(), MerchantFinanceError> {
        if !["paid", "failed", "processing", "unknown"].contains(&status)
            || reference.len() > 256
            || (status == "paid" && reference.trim().is_empty())
            || reason.is_some_and(|value| value.len() > 64)
        {
            return Err(MerchantFinanceError::InvalidInput);
        }
        self.bounded(async {
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            lock_settings(&tx).await?;
            let mut account = load_account(&tx, claim.payout.merchant_id, true).await?;
            let withdrawal = withdrawal_by_id(&tx, claim.payout.withdrawal_id, true)
                .await?
                .ok_or(MerchantFinanceError::Invariant)?;
            let row = payout_row(&tx, claim.payout.withdrawal_id)
                .await?
                .ok_or(MerchantFinanceError::Invariant)?;
            let captured = payout_from_row(&row)?;
            if captured.merchant_id != withdrawal.merchant_id
                || captured.event_key != withdrawal.event_key
                || captured.merchant_id != claim.payout.merchant_id
                || captured.event_key != claim.payout.event_key
                || captured.amount_minor != claim.payout.amount_minor
                || captured.currency != claim.payout.currency
            {
                return Err(MerchantFinanceError::Invariant);
            }
            if get::<String>(&row, "lease_owner")? != claim.lease_owner
                || get::<i64>(&row, "lease_until")? <= now()
                || get::<String>(&row, "status")? != "processing"
            {
                return Err(MerchantFinanceError::Conflict);
            }
            if withdrawal.status != "pending" {
                return Err(MerchantFinanceError::Conflict);
            }
            if ["paid", "failed"].contains(&status) {
                if account.withdrawing_quota < withdrawal.amount_quota {
                    return Err(MerchantFinanceError::Invariant);
                }
                account.withdrawing_quota -= withdrawal.amount_quota;
                if status == "paid" {
                    account.total_paid_quota =
                        checked_add(account.total_paid_quota, withdrawal.amount_quota)?;
                } else {
                    let debt = account.debt_quota.min(withdrawal.amount_quota);
                    account.debt_quota -= debt;
                    account.available_quota =
                        checked_add(account.available_quota, withdrawal.amount_quota - debt)?;
                }
                save_account(&tx, &account, now()).await?;
                let update = Query::update()
                    .table(Alias::new("merchant_withdrawals"))
                    .values([
                        (
                            Alias::new("status"),
                            if status == "paid" { "paid" } else { "rejected" }.into(),
                        ),
                        (Alias::new("reference"), reference.into()),
                        (Alias::new("reason"), reason.map(str::to_owned).into()),
                        (Alias::new("updated_at"), now().into()),
                    ])
                    .and_where(Expr::col(Alias::new("id")).eq(withdrawal.id))
                    .and_where(Expr::col(Alias::new("status")).eq("pending"))
                    .to_owned();
                if tx
                    .execute(tx.get_database_backend().build(&update))
                    .await
                    .map_err(db_error)?
                    .rows_affected()
                    != 1
                {
                    return Err(MerchantFinanceError::Conflict);
                }
                append_entry(
                    &tx,
                    withdrawal.merchant_id,
                    &format!("payout-{status}:{}", withdrawal.event_key),
                    if status == "paid" {
                        "withdrawal_paid"
                    } else {
                        "withdrawal_rejected"
                    },
                    reference,
                    0,
                    0,
                    if status == "paid" {
                        -withdrawal.amount_quota
                    } else {
                        withdrawal.amount_quota
                    },
                    now(),
                )
                .await?;
            }
            let update = Query::update()
                .table(Alias::new(PAYOUTS))
                .values([
                    (Alias::new("status"), status.into()),
                    (Alias::new("reference"), reference.into()),
                    (Alias::new("reason"), reason.map(str::to_owned).into()),
                    (Alias::new("lease_owner"), "".into()),
                    (Alias::new("lease_until"), 0_i64.into()),
                    (Alias::new("next_at"), (now() + 60).into()),
                    (Alias::new("updated_at"), now().into()),
                ])
                .and_where(Expr::col(Alias::new("withdrawal_id")).eq(withdrawal.id))
                .to_owned();
            tx.execute(tx.get_database_backend().build(&update))
                .await
                .map_err(db_error)?;
            tx.commit().await.map_err(db_error)?;
            Ok(())
        })
        .await
    }
}

async fn current_admin(
    tx: &DatabaseTransaction,
    user_id: i64,
) -> Result<bool, MerchantFinanceError> {
    let mut query = select("users");
    query.and_where(Expr::col(Alias::new("id")).eq(user_id));
    if tx.get_database_backend() != DbBackend::Sqlite {
        query.lock(LockType::Update);
    }
    let Some(row) = tx
        .query_one(tx.get_database_backend().build(&query.to_owned()))
        .await
        .map_err(db_error)?
    else {
        return Ok(false);
    };
    Ok(get::<i16>(&row, "role")? == 1
        && get::<i16>(&row, "status")? == 1
        && get::<Option<TimeDateTimeWithTimeZone>>(&row, "deleted_at")?.is_none())
}

/// Cancel a payout only before its first external submission and release its reservation atomically.
async fn cancel_unsubmitted_payout(
    tx: &DatabaseTransaction,
    payout: &MerchantPayout,
) -> Result<(), MerchantFinanceError> {
    if !["pending", "processing"].contains(&payout.status.as_str()) {
        return Err(MerchantFinanceError::Conflict);
    }
    let mut account = load_account(tx, payout.merchant_id, true).await?;
    let withdrawal = withdrawal_by_id(tx, payout.withdrawal_id, true)
        .await?
        .ok_or(MerchantFinanceError::Invariant)?;
    if withdrawal.status != "pending" || account.withdrawing_quota < withdrawal.amount_quota {
        return Err(MerchantFinanceError::Invariant);
    }
    account.withdrawing_quota -= withdrawal.amount_quota;
    let debt = account.debt_quota.min(withdrawal.amount_quota);
    account.debt_quota -= debt;
    account.available_quota = checked_add(account.available_quota, withdrawal.amount_quota - debt)?;
    let at = now();
    save_account(tx, &account, at).await?;
    let reject = Query::update()
        .table(Alias::new("merchant_withdrawals"))
        .values([
            (Alias::new("status"), "rejected".into()),
            (Alias::new("reason"), "initiator_permission_revoked".into()),
            (Alias::new("updated_at"), at.into()),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(withdrawal.id))
        .and_where(Expr::col(Alias::new("status")).eq("pending"))
        .to_owned();
    if tx
        .execute(tx.get_database_backend().build(&reject))
        .await
        .map_err(db_error)?
        .rows_affected()
        != 1
    {
        return Err(MerchantFinanceError::Conflict);
    }
    append_entry(
        tx,
        withdrawal.merchant_id,
        &format!("withdraw-rejected:{}", withdrawal.event_key),
        "withdrawal_rejected",
        "initiator_permission_revoked",
        0,
        0,
        withdrawal.amount_quota,
        at,
    )
    .await?;
    let update = Query::update()
        .table(Alias::new(PAYOUTS))
        .values([
            (Alias::new("status"), "failed".into()),
            (Alias::new("reason"), "initiator_permission_revoked".into()),
            (Alias::new("lease_owner"), "".into()),
            (Alias::new("lease_until"), 0_i64.into()),
            (Alias::new("updated_at"), at.into()),
        ])
        .and_where(Expr::col(Alias::new("withdrawal_id")).eq(payout.withdrawal_id))
        .and_where(Expr::col(Alias::new("status")).is_in(["pending", "processing"]))
        .and_where(Expr::col(Alias::new("submit_authorized_at")).is_null())
        .to_owned();
    if tx
        .execute(tx.get_database_backend().build(&update))
        .await
        .map_err(db_error)?
        .rows_affected()
        != 1
    {
        return Err(MerchantFinanceError::Conflict);
    }
    Ok(())
}

async fn lock_settings(tx: &DatabaseTransaction) -> Result<(), MerchantFinanceError> {
    let query = Query::update()
        .table(Alias::new(SETTINGS))
        .value(Alias::new("version"), Expr::col(Alias::new("version")))
        .and_where(Expr::col(Alias::new("id")).eq(1_i16))
        .to_owned();
    tx.execute(tx.get_database_backend().build(&query))
        .await
        .map_err(db_error)?;
    Ok(())
}
async fn read_settings<C: ConnectionTrait>(
    connection: &C,
) -> Result<MerchantPayoutSettings, MerchantFinanceError> {
    let query = select(SETTINGS)
        .and_where(Expr::col(Alias::new("id")).eq(1_i16))
        .to_owned();
    let row = connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(db_error)?
        .ok_or(MerchantFinanceError::Invariant)?;
    Ok(MerchantPayoutSettings {
        version: get(&row, "version")?,
        config: serde_json::from_str(&get::<String>(&row, "config_json")?)
            .map_err(|_| MerchantFinanceError::Invariant)?,
        secret: get(&row, "secret_json")?,
    })
}
async fn account_row<C: ConnectionTrait>(
    connection: &C,
    merchant: i64,
) -> Result<Option<sea_orm::QueryResult>, MerchantFinanceError> {
    let query = select(ACCOUNTS)
        .and_where(Expr::col(Alias::new("merchant_id")).eq(merchant))
        .to_owned();
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(db_error)
}
async fn payout_row<C: ConnectionTrait>(
    connection: &C,
    id: i64,
) -> Result<Option<sea_orm::QueryResult>, MerchantFinanceError> {
    let query = select(PAYOUTS)
        .and_where(Expr::col(Alias::new("withdrawal_id")).eq(id))
        .to_owned();
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(db_error)
}
fn account_from_row(
    row: &sea_orm::QueryResult,
) -> Result<MerchantPayoutAccount, MerchantFinanceError> {
    Ok(MerchantPayoutAccount {
        configured: get::<Option<String>>(row, "secret_json")?.is_some(),
        label: get(row, "label")?,
        version: get(row, "version")?,
    })
}
fn payout_from_row(row: &sea_orm::QueryResult) -> Result<MerchantPayout, MerchantFinanceError> {
    Ok(MerchantPayout {
        withdrawal_id: get(row, "withdrawal_id")?,
        merchant_id: get(row, "merchant_id")?,
        event_key: get(row, "event_key")?,
        status: get(row, "status")?,
        amount_minor: get(row, "amount_minor")?,
        currency: get(row, "currency")?,
        reference: get(row, "reference")?,
        reason: get(row, "reason")?,
        created_at: get(row, "created_at")?,
        updated_at: get(row, "updated_at")?,
    })
}
pub(crate) async fn find_public<C: ConnectionTrait>(
    connection: &C,
    id: i64,
) -> Result<Option<MerchantPayout>, MerchantFinanceError> {
    payout_row(connection, id)
        .await?
        .as_ref()
        .map(payout_from_row)
        .transpose()
}
