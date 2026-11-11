//! Merchant proceeds are durable liabilities, separate from consumer wallet balances.

use std::time::Duration;

use af_domain::{GatewayPrincipal, MerchantBillingSnapshot, Quota, UserId};
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, QueryResult, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, LockType, OnConflict, Order, Query, SelectStatement},
};
use serde::Serialize;
use thiserror::Error;
use time::{Date, PrimitiveDateTime, format_description};
use tokio::time::timeout;

use crate::{DatabasePool, QuotaRepositoryError};

const ACCOUNTS: &str = "merchant_finance_accounts";
const RESERVATIONS: &str = "merchant_billing_reservations";
const ENTRIES: &str = "merchant_finance_entries";
const WITHDRAWALS: &str = "merchant_withdrawals";
const REPORT_MAX_DAYS: i64 = 366;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MerchantFinanceError {
    #[error("商户资金请求无效")]
    InvalidInput,
    #[error("商户资金记录不存在")]
    NotFound,
    #[error("商户资金状态冲突")]
    Conflict,
    #[error("商户可提现余额不足或存在待补金额")]
    InsufficientBalance,
    #[error("商户资金服务暂不可用")]
    Unavailable,
    #[error("商户资金记录异常")]
    Invariant,
    #[error("报表结果超过限制")]
    LimitReached,
    #[error("无权访问商户资金")]
    Forbidden,
}

#[derive(Clone, Debug, Serialize)]
pub struct MerchantFinanceBalance {
    pub merchant_id: i64,
    pub pending_quota: i64,
    pub available_quota: i64,
    pub withdrawing_quota: i64,
    pub frozen_quota: i64,
    pub debt_quota: i64,
    pub total_gross_quota: i64,
    pub total_commission_quota: i64,
    pub total_paid_quota: i64,
    pub quota_per_usd: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MerchantFinanceEntry {
    pub id: i64,
    pub kind: String,
    pub reference: String,
    pub gross_quota: i64,
    pub commission_quota: i64,
    pub net_quota: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MerchantWithdrawal {
    pub id: i64,
    pub event_key: String,
    pub merchant_id: i64,
    pub amount_quota: i64,
    pub status: String,
    pub reference: String,
    pub reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub payout: Option<crate::MerchantPayout>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MerchantFinancePage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MerchantFinanceReport {
    pub granularity: String,
    pub from_date: String,
    pub to_date: String,
    pub totals: MerchantFinanceReportTotals,
    pub rows: Vec<MerchantFinanceReportRow>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MerchantFinanceReportTotals {
    pub sales_gross_quota: i64,
    pub commission_quota: i64,
    pub merchant_net_quota: i64,
    pub refunded_gross_quota: i64,
    pub refunded_commission_quota: i64,
    pub refunded_net_quota: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MerchantFinanceReportRow {
    /// UTC calendar date, or UTC year-month for monthly reports.
    pub period: String,
    pub service_id: Option<i64>,
    pub service_name: String,
    pub model: String,
    /// Signed ledger movement; reversals are negative.
    pub gross_quota: i64,
    pub commission_quota: i64,
    pub net_quota: i64,
    /// Positive magnitudes of reversed sales and commissions.
    pub refunded_gross_quota: i64,
    pub refunded_commission_quota: i64,
    pub refunded_net_quota: i64,
}

#[derive(Clone)]
pub struct MerchantFinanceRepository {
    pool: DatabasePool,
    timeout: Duration,
}

impl MerchantFinanceRepository {
    #[must_use]
    pub const fn new(pool: DatabasePool, timeout: Duration) -> Self {
        Self { pool, timeout }
    }

    pub async fn balance(
        &self,
        merchant_id: i64,
    ) -> Result<MerchantFinanceBalance, MerchantFinanceError> {
        timeout(self.timeout, async {
            self.release_due(merchant_id).await?;
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            ensure_account(&tx, merchant_id).await?;
            let account = load_account(&tx, merchant_id, false).await?;
            tx.commit().await.map_err(db_error)?;
            Ok(account)
        })
        .await
        .map_err(|_| MerchantFinanceError::Unavailable)?
    }

    pub async fn entries(
        &self,
        merchant_id: i64,
        limit: u32,
        before_id: Option<i64>,
    ) -> Result<MerchantFinancePage<MerchantFinanceEntry>, MerchantFinanceError> {
        let limit = page_limit(limit)?;
        timeout(self.timeout, async {
            let mut query = select(ENTRIES);
            query.and_where(Expr::col(Alias::new("merchant_id")).eq(merchant_id));
            page_query(&mut query, limit, before_id)?;
            let rows = all(self.pool.connection(), query).await?;
            let mut items = rows
                .iter()
                .map(entry_from_row)
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = if items.len() > limit as usize {
                items.pop();
                items.last().map(|item| item.id)
            } else {
                None
            };
            Ok(MerchantFinancePage { items, next_cursor })
        })
        .await
        .map_err(|_| MerchantFinanceError::Unavailable)?
    }

    pub async fn report(
        &self,
        merchant_id: i64,
        from_date: &str,
        to_date: &str,
        granularity: &str,
    ) -> Result<MerchantFinanceReport, MerchantFinanceError> {
        if merchant_id <= 0 || !["day", "month"].contains(&granularity) {
            return Err(MerchantFinanceError::InvalidInput);
        }
        let format = format_description::parse_borrowed::<2>("[year]-[month]-[day]")
            .map_err(|_| MerchantFinanceError::Invariant)?;
        let from = parse_report_date(from_date, &format)?;
        let to = parse_report_date(to_date, &format)?;
        let span_days = (to - from).whole_days();
        if !(0..REPORT_MAX_DAYS).contains(&span_days) {
            return Err(MerchantFinanceError::InvalidInput);
        }
        let start = PrimitiveDateTime::new(from, time::Time::MIDNIGHT)
            .assume_utc()
            .unix_timestamp();
        let end = PrimitiveDateTime::new(
            to.next_day().ok_or(MerchantFinanceError::InvalidInput)?,
            time::Time::MIDNIGHT,
        )
        .assume_utc()
        .unix_timestamp();
        timeout(self.timeout, async {
            let backend = self.pool.connection().get_database_backend();
            let bucket = report_bucket_expression(backend, granularity);
            let mut query = Query::select();
            query
                .expr_as(Expr::cust(bucket.clone()), Alias::new("period"))
                .expr_as(
                    Expr::cust("merchant_billing_reservations.service_id"),
                    Alias::new("service_id"),
                )
                .expr_as(
                    Expr::cust("COALESCE(merchant_services.name, '未关联')"),
                    Alias::new("service_name"),
                )
                .expr_as(
                    Expr::cust("COALESCE(usage_logs.model, '未关联')"),
                    Alias::new("model"),
                )
                .expr_as(
                    Expr::cust("SUM(e.gross_quota)"),
                    Alias::new("gross_quota"),
                )
                .expr_as(
                    Expr::cust("SUM(e.commission_quota)"),
                    Alias::new("commission_quota"),
                )
                .expr_as(Expr::cust("SUM(e.net_quota)"), Alias::new("net_quota"))
                .expr_as(
                    Expr::cust("SUM(CASE WHEN e.kind = 'sale_reversed' THEN -e.gross_quota ELSE 0 END)"),
                    Alias::new("refunded_gross_quota"),
                )
                .expr_as(
                    Expr::cust("SUM(CASE WHEN e.kind = 'sale_reversed' THEN -e.commission_quota ELSE 0 END)"),
                    Alias::new("refunded_commission_quota"),
                )
                .expr_as(
                    Expr::cust("SUM(CASE WHEN e.kind = 'sale_reversed' THEN -e.net_quota ELSE 0 END)"),
                    Alias::new("refunded_net_quota"),
                )
                .from_as(Alias::new(ENTRIES), Alias::new("e"))
                .left_join(
                    Alias::new(RESERVATIONS),
                    Expr::col((Alias::new("e"), Alias::new("reference")))
                        .equals((Alias::new("merchant_billing_reservations"), Alias::new("reservation_key"))),
                )
                .left_join(
                    Alias::new("usage_logs"),
                    Expr::col((Alias::new("merchant_billing_reservations"), Alias::new("reservation_key")))
                        .equals((Alias::new("usage_logs"), Alias::new("event_id"))),
                )
                .left_join(
                    Alias::new("merchant_services"),
                    Expr::col((Alias::new("merchant_billing_reservations"), Alias::new("service_id")))
                        .equals((Alias::new("merchant_services"), Alias::new("id"))),
                )
                .and_where(Expr::col((Alias::new("e"), Alias::new("merchant_id"))).eq(merchant_id))
                .and_where(Expr::col((Alias::new("e"), Alias::new("created_at"))).gte(start))
                .and_where(Expr::col((Alias::new("e"), Alias::new("created_at"))).lt(end))
                .and_where(Expr::col((Alias::new("e"), Alias::new("kind"))).is_in(["income_pending", "sale_reversed"]))
                .add_group_by([
                    Expr::cust(bucket.clone()),
                    Expr::cust("merchant_billing_reservations.service_id"),
                    Expr::cust("COALESCE(merchant_services.name, '未关联')"),
                    Expr::cust("COALESCE(usage_logs.model, '未关联')"),
                ])
                .order_by(Alias::new("period"), Order::Asc)
                .order_by(Alias::new("service_id"), Order::Asc)
                .order_by(Alias::new("model"), Order::Asc)
                .limit(20_001);
            let rows = all(self.pool.connection(), query).await?;
            if rows.len() > 20_000 {
                return Err(MerchantFinanceError::LimitReached);
            }
            let rows = rows
                .iter()
                .map(|row| {
                    let gross_quota: i64 = get(row, "gross_quota")?;
                    let commission_quota: i64 = get(row, "commission_quota")?;
                    let net_quota: i64 = get(row, "net_quota")?;
                    Ok(MerchantFinanceReportRow {
                        period: get(row, "period")?,
                        service_id: get(row, "service_id")?,
                        service_name: get(row, "service_name")?,
                        model: get(row, "model")?,
                        gross_quota,
                        commission_quota,
                        net_quota,
                        refunded_gross_quota: get(row, "refunded_gross_quota")?,
                        refunded_commission_quota: get(row, "refunded_commission_quota")?,
                        refunded_net_quota: get(row, "refunded_net_quota")?,
                    })
                })
                .collect::<Result<Vec<MerchantFinanceReportRow>, MerchantFinanceError>>()?;
            let totals = rows.iter().try_fold(
                MerchantFinanceReportTotals {
                    sales_gross_quota: 0,
                    commission_quota: 0,
                    merchant_net_quota: 0,
                    refunded_gross_quota: 0,
                    refunded_commission_quota: 0,
                    refunded_net_quota: 0,
                },
                |mut totals, row| {
                    totals.sales_gross_quota = checked_add(totals.sales_gross_quota, row.gross_quota)?;
                    totals.commission_quota = checked_add(totals.commission_quota, row.commission_quota)?;
                    totals.merchant_net_quota = checked_add(totals.merchant_net_quota, row.net_quota)?;
                    totals.refunded_gross_quota = checked_add(totals.refunded_gross_quota, row.refunded_gross_quota)?;
                    totals.refunded_commission_quota = checked_add(totals.refunded_commission_quota, row.refunded_commission_quota)?;
                    totals.refunded_net_quota = checked_add(totals.refunded_net_quota, row.refunded_net_quota)?;
                    Ok::<_, MerchantFinanceError>(totals)
                },
            )?;
            Ok(MerchantFinanceReport {
                granularity: granularity.to_owned(),
                from_date: from_date.to_owned(),
                to_date: to_date.to_owned(),
                totals,
                rows,
            })
        })
        .await
        .map_err(|_| MerchantFinanceError::Unavailable)?
    }

    pub async fn withdrawals(
        &self,
        merchant_id: Option<i64>,
        status: Option<&str>,
        limit: u32,
        before_id: Option<i64>,
    ) -> Result<MerchantFinancePage<MerchantWithdrawal>, MerchantFinanceError> {
        let limit = page_limit(limit)?;
        if status.is_some_and(|value| !["pending", "paid", "rejected"].contains(&value)) {
            return Err(MerchantFinanceError::InvalidInput);
        }
        timeout(self.timeout, async {
            let mut query = select(WITHDRAWALS);
            if let Some(id) = merchant_id {
                query.and_where(Expr::col(Alias::new("merchant_id")).eq(id));
            }
            if let Some(status) = status {
                query.and_where(Expr::col(Alias::new("status")).eq(status));
            }
            page_query(&mut query, limit, before_id)?;
            let rows = all(self.pool.connection(), query).await?;
            let mut items = rows
                .iter()
                .map(withdrawal_from_row)
                .collect::<Result<Vec<_>, _>>()?;
            for item in &mut items {
                item.payout =
                    crate::merchant_payout::find_public(self.pool.connection(), item.id).await?;
            }
            let next_cursor = if items.len() > limit as usize {
                items.pop();
                items.last().map(|item| item.id)
            } else {
                None
            };
            Ok(MerchantFinancePage { items, next_cursor })
        })
        .await
        .map_err(|_| MerchantFinanceError::Unavailable)?
    }

    pub async fn request_withdrawal(
        &self,
        merchant_id: i64,
        event_key: &str,
        amount: i64,
    ) -> Result<MerchantWithdrawal, MerchantFinanceError> {
        if merchant_id <= 0
            || amount <= 0
            || event_key.len() != 32
            || !event_key.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(MerchantFinanceError::InvalidInput);
        }
        timeout(self.timeout, async {
            self.release_due(merchant_id).await?;
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            ensure_account(&tx, merchant_id).await?;
            let account = load_account(&tx, merchant_id, true).await?;
            if let Some(existing) = withdrawal_by_event(&tx, event_key).await? {
                if existing.merchant_id != merchant_id || existing.amount_quota != amount {
                    return Err(MerchantFinanceError::Conflict);
                }
                return Ok(existing);
            }
            if account.available_quota < amount
                || account.debt_quota != 0
                || account.frozen_quota != 0
            {
                return Err(MerchantFinanceError::InsufficientBalance);
            }
            let now = now();
            let mut changed = account.clone();
            changed.available_quota -= amount;
            changed.withdrawing_quota = checked_add(changed.withdrawing_quota, amount)?;
            save_account(&tx, &changed, now).await?;
            let insert = Query::insert()
                .into_table(Alias::new(WITHDRAWALS))
                .columns(
                    [
                        "event_key",
                        "merchant_id",
                        "amount_quota",
                        "status",
                        "reference",
                        "created_at",
                        "updated_at",
                    ]
                    .map(Alias::new),
                )
                .values_panic([
                    event_key.into(),
                    merchant_id.into(),
                    amount.into(),
                    "pending".into(),
                    "".into(),
                    now.into(),
                    now.into(),
                ])
                .to_owned();
            tx.execute(tx.get_database_backend().build(&insert))
                .await
                .map_err(db_error)?;
            append_entry(
                &tx,
                merchant_id,
                &format!("withdraw:{event_key}"),
                "withdrawal_requested",
                event_key,
                0,
                0,
                -amount,
                now,
            )
            .await?;
            let result = withdrawal_by_event(&tx, event_key)
                .await?
                .ok_or(MerchantFinanceError::Invariant)?;
            tx.commit().await.map_err(db_error)?;
            Ok(result)
        })
        .await
        .map_err(|_| MerchantFinanceError::Unavailable)?
    }

    pub async fn decide_withdrawal(
        &self,
        id: i64,
        actor: UserId,
        status: &str,
        reference: &str,
        reason: Option<&str>,
    ) -> Result<MerchantWithdrawal, MerchantFinanceError> {
        if id <= 0
            || !["paid", "rejected"].contains(&status)
            || reference.len() > 256
            || (status == "paid" && reference.trim().is_empty())
            || reason.is_some_and(|value| value.len() > 512)
        {
            return Err(MerchantFinanceError::InvalidInput);
        }
        timeout(self.timeout, async {
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            let observed = withdrawal_by_id(&tx, id, false)
                .await?
                .ok_or(MerchantFinanceError::NotFound)?;
            let mut account = load_account(&tx, observed.merchant_id, true).await?;
            let current = withdrawal_by_id(&tx, id, true)
                .await?
                .ok_or(MerchantFinanceError::NotFound)?;
            let mut payout_query = select("merchant_payouts");
            payout_query.and_where(Expr::col(Alias::new("withdrawal_id")).eq(id));
            if tx
                .query_one(tx.get_database_backend().build(&payout_query))
                .await
                .map_err(db_error)?
                .is_some()
            {
                // A transfer may already exist remotely. Manual decisions must never
                // compete with submission or reconciliation, even while disabled.
                return Err(MerchantFinanceError::Conflict);
            }
            if current.status != "pending" {
                if current.status == status
                    && current.reference == reference
                    && current.reason.as_deref() == reason
                {
                    return Ok(current);
                }
                return Err(MerchantFinanceError::Conflict);
            }
            if account.withdrawing_quota < current.amount_quota {
                return Err(MerchantFinanceError::Invariant);
            }
            // Disputes that arise after requesting a withdrawal block a new payout.
            if status == "paid" && (account.debt_quota > 0 || account.frozen_quota > 0) {
                return Err(MerchantFinanceError::Conflict);
            }
            account.withdrawing_quota -= current.amount_quota;
            if status == "paid" {
                account.total_paid_quota =
                    checked_add(account.total_paid_quota, current.amount_quota)?;
            } else {
                let against_debt = account.debt_quota.min(current.amount_quota);
                account.debt_quota -= against_debt;
                account.available_quota =
                    checked_add(account.available_quota, current.amount_quota - against_debt)?;
            }
            let now = now();
            save_account(&tx, &account, now).await?;
            let update = Query::update()
                .table(Alias::new(WITHDRAWALS))
                .values([
                    (Alias::new("status"), status.into()),
                    (Alias::new("reference"), reference.into()),
                    (Alias::new("reason"), reason.map(str::to_owned).into()),
                    (Alias::new("actor_user_id"), actor.get().into()),
                    (Alias::new("updated_at"), now.into()),
                ])
                .and_where(Expr::col(Alias::new("id")).eq(id))
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
                current.merchant_id,
                &format!("withdraw-{status}:{}", current.event_key),
                if status == "paid" {
                    "withdrawal_paid"
                } else {
                    "withdrawal_rejected"
                },
                reference,
                0,
                0,
                if status == "paid" {
                    -current.amount_quota
                } else {
                    current.amount_quota
                },
                now,
            )
            .await?;
            let result = withdrawal_by_id(&tx, id, false)
                .await?
                .ok_or(MerchantFinanceError::Invariant)?;
            tx.commit().await.map_err(db_error)?;
            Ok(result)
        })
        .await
        .map_err(|_| MerchantFinanceError::Unavailable)?
    }

    pub async fn hold_balance(
        &self,
        merchant_id: i64,
        actor: UserId,
        event_key: &str,
        amount: i64,
        freeze: bool,
        reason: &str,
    ) -> Result<MerchantFinanceBalance, MerchantFinanceError> {
        if merchant_id <= 0
            || amount <= 0
            || event_key.len() != 32
            || !event_key.bytes().all(|byte| byte.is_ascii_hexdigit())
            || reason.trim().is_empty()
            || reason.len() > 200
        {
            return Err(MerchantFinanceError::InvalidInput);
        }
        timeout(self.timeout, async {
            self.release_due(merchant_id).await?;
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            ensure_account(&tx, merchant_id).await?;
            let mut account = load_account(&tx, merchant_id, true).await?;
            let event = format!("hold:{event_key}");
            let reference = format!("admin:{}:{reason}", actor.get());
            let kind = if freeze {
                "funds_frozen"
            } else {
                "funds_unfrozen"
            };
            let mut existing_query = select(ENTRIES);
            existing_query.and_where(Expr::col(Alias::new("event_key")).eq(event.clone()));
            if let Some(row) = tx
                .query_one(tx.get_database_backend().build(&existing_query))
                .await
                .map_err(db_error)?
            {
                if get::<i64>(&row, "merchant_id")? != merchant_id
                    || get::<String>(&row, "kind")? != kind
                    || get::<String>(&row, "reference")? != reference
                    || get::<i64>(&row, "net_quota")? != if freeze { -amount } else { amount }
                {
                    return Err(MerchantFinanceError::Conflict);
                }
                return Ok(account);
            }
            if freeze {
                if account.available_quota < amount {
                    return Err(MerchantFinanceError::InsufficientBalance);
                }
                account.available_quota -= amount;
                account.frozen_quota = checked_add(account.frozen_quota, amount)?;
            } else {
                if account.frozen_quota < amount {
                    return Err(MerchantFinanceError::InsufficientBalance);
                }
                account.frozen_quota -= amount;
                let against_debt = account.debt_quota.min(amount);
                account.debt_quota -= against_debt;
                account.available_quota =
                    checked_add(account.available_quota, amount - against_debt)?;
            }
            save_account(&tx, &account, now()).await?;
            append_entry(
                &tx,
                merchant_id,
                &event,
                kind,
                &reference,
                0,
                0,
                if freeze { -amount } else { amount },
                now(),
            )
            .await?;
            tx.commit().await.map_err(db_error)?;
            Ok(account)
        })
        .await
        .map_err(|_| MerchantFinanceError::Unavailable)?
    }

    async fn release_due(&self, merchant_id: i64) -> Result<(), MerchantFinanceError> {
        let mut query = select(RESERVATIONS);
        query
            .and_where(Expr::col(Alias::new("merchant_id")).eq(merchant_id))
            .and_where(Expr::col(Alias::new("status")).eq("settled"))
            .and_where(Expr::col(Alias::new("released")).eq(false))
            .and_where(Expr::col(Alias::new("available_at")).lte(now()))
            .limit(1000);
        let rows = all(self.pool.connection(), query).await?;
        for row in rows {
            let key: String = get(&row, "reservation_key")?;
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            let Some(current) = merchant_reservation(&tx, &key, true).await? else {
                return Err(MerchantFinanceError::Invariant);
            };
            if get::<String>(&current, "status")? != "settled" || get::<bool>(&current, "released")?
            {
                continue;
            }
            let net: i64 = get(&current, "net_quota")?;
            let mut account = load_account(&tx, merchant_id, true).await?;
            if account.pending_quota < net {
                return Err(MerchantFinanceError::Invariant);
            }
            account.pending_quota -= net;
            let against_debt = account.debt_quota.min(net);
            account.debt_quota -= against_debt;
            account.available_quota = checked_add(account.available_quota, net - against_debt)?;
            save_account(&tx, &account, now()).await?;
            let update = Query::update()
                .table(Alias::new(RESERVATIONS))
                .value(Alias::new("released"), true)
                .and_where(Expr::col(Alias::new("reservation_key")).eq(key.clone()))
                .to_owned();
            tx.execute(tx.get_database_backend().build(&update))
                .await
                .map_err(db_error)?;
            append_entry(
                &tx,
                merchant_id,
                &format!("mature:{key}"),
                "income_available",
                &key,
                0,
                0,
                net,
                now(),
            )
            .await?;
            tx.commit().await.map_err(db_error)?;
        }
        Ok(())
    }

    /// Reverse the complete captured sale, never rewrite its original ledger entry.
    /// The consumer refund, platform fee reversal and merchant debit commit together.
    pub async fn reverse_settlement(
        &self,
        key: &str,
        actor: UserId,
        reason: &str,
    ) -> Result<(), MerchantFinanceError> {
        if key.len() != 32
            || !key.bytes().all(|byte| byte.is_ascii_hexdigit())
            || reason.trim().is_empty()
            || reason.len() > 500
        {
            return Err(MerchantFinanceError::InvalidInput);
        }
        timeout(self.timeout, async {
            let mut observed_query = select("billing_reservations");
            observed_query.and_where(Expr::col(Alias::new("idempotency_key")).eq(key));
            let observed = self
                .pool
                .connection()
                .query_one(
                    self.pool
                        .connection()
                        .get_database_backend()
                        .build(&observed_query),
                )
                .await
                .map_err(db_error)?
                .ok_or(MerchantFinanceError::NotFound)?;
            let user_id: i64 = get(&observed, "user_id")?;
            let tx = self.pool.connection().begin().await.map_err(db_error)?;
            // Match the consumer-first lock order of normal billing settlement.
            let guard = Query::update()
                .table(Alias::new("users"))
                .value(Alias::new("quota"), Expr::col(Alias::new("quota")))
                .and_where(Expr::col(Alias::new("id")).eq(user_id))
                .to_owned();
            tx.execute(tx.get_database_backend().build(&guard))
                .await
                .map_err(db_error)?;
            let mut user_query = select("users");
            user_query
                .and_where(Expr::col(Alias::new("id")).eq(user_id))
                .lock(LockType::Update);
            let user = tx
                .query_one(tx.get_database_backend().build(&user_query))
                .await
                .map_err(db_error)?
                .ok_or(MerchantFinanceError::NotFound)?;
            let row = merchant_reservation(&tx, key, true)
                .await?
                .ok_or(MerchantFinanceError::NotFound)?;
            let status: String = get(&row, "status")?;
            if status == "reversed" {
                return Ok(());
            }
            if status != "settled" || get::<i16>(&observed, "status")? != 3 {
                return Err(MerchantFinanceError::Conflict);
            }
            let merchant_id: i64 = get(&row, "merchant_id")?;
            let gross: i64 = get(&row, "gross_quota")?;
            let fee: i64 = get(&row, "commission_quota")?;
            let net: i64 = get(&row, "net_quota")?;
            if gross < 0 || fee < 0 || net < 0 || fee.checked_add(net) != Some(gross) {
                return Err(MerchantFinanceError::Invariant);
            }
            let mut account = load_account(&tx, merchant_id, true).await?;
            if !get::<bool>(&row, "released")? {
                account.pending_quota = account
                    .pending_quota
                    .checked_sub(net)
                    .filter(|value| *value >= 0)
                    .ok_or(MerchantFinanceError::Invariant)?;
            } else {
                let available = account.available_quota.min(net);
                account.available_quota -= available;
                account.debt_quota = checked_add(account.debt_quota, net - available)?;
            }
            account.total_gross_quota = account
                .total_gross_quota
                .checked_sub(gross)
                .filter(|value| *value >= 0)
                .ok_or(MerchantFinanceError::Invariant)?;
            account.total_commission_quota = account
                .total_commission_quota
                .checked_sub(fee)
                .filter(|value| *value >= 0)
                .ok_or(MerchantFinanceError::Invariant)?;
            save_account(&tx, &account, now()).await?;
            let before: i64 = get(&user, "quota")?;
            let after = checked_add(before, gross)?;
            let user_update = Query::update()
                .table(Alias::new("users"))
                .value(Alias::new("quota"), after)
                .value(
                    Alias::new("updated_at"),
                    TimeDateTimeWithTimeZone::now_utc(),
                )
                .and_where(Expr::col(Alias::new("id")).eq(user_id))
                .and_where(Expr::col(Alias::new("quota")).eq(before))
                .to_owned();
            if tx
                .execute(tx.get_database_backend().build(&user_update))
                .await
                .map_err(db_error)?
                .rows_affected()
                != 1
            {
                return Err(MerchantFinanceError::Conflict);
            }
            let update = Query::update()
                .table(Alias::new(RESERVATIONS))
                .value(Alias::new("status"), "reversed")
                .and_where(Expr::col(Alias::new("reservation_key")).eq(key))
                .and_where(Expr::col(Alias::new("status")).eq("settled"))
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
            if gross > 0 {
                use sha2::{Digest, Sha256};
                let digest = Sha256::digest(format!("merchant-refund:{key}"));
                let event_key = digest[..16]
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                let wallet = Query::insert()
                    .into_table(Alias::new("wallet_ledger_entries"))
                    .columns(
                        [
                            "event_key",
                            "user_id",
                            "actor_user_id",
                            "entry_type",
                            "quota_delta",
                            "balance_before",
                            "balance_after",
                            "reason",
                            "created_at",
                        ]
                        .map(Alias::new),
                    )
                    .values_panic([
                        event_key.into(),
                        user_id.into(),
                        actor.get().into(),
                        2_i16.into(),
                        gross.into(),
                        before.into(),
                        after.into(),
                        reason.into(),
                        TimeDateTimeWithTimeZone::now_utc().into(),
                    ])
                    .to_owned();
                tx.execute(tx.get_database_backend().build(&wallet))
                    .await
                    .map_err(db_error)?;
            }
            append_entry(
                &tx,
                merchant_id,
                &format!("reverse:{key}"),
                "sale_reversed",
                key,
                -gross,
                -fee,
                -net,
                now(),
            )
            .await?;
            tx.commit().await.map_err(db_error)?;
            Ok(())
        })
        .await
        .map_err(|_| MerchantFinanceError::Unavailable)?
    }
}

pub(crate) fn select(table: &str) -> SelectStatement {
    Query::select()
        .column(sea_orm::sea_query::Asterisk)
        .from(Alias::new(table))
        .to_owned()
}

fn report_bucket_expression(backend: DbBackend, granularity: &str) -> String {
    match backend {
        DbBackend::Sqlite => {
            let format = if granularity == "month" {
                "%Y-%m"
            } else {
                "%Y-%m-%d"
            };
            format!("strftime('{format}', e.created_at, 'unixepoch')")
        }
        DbBackend::Postgres => {
            let format = if granularity == "month" {
                "YYYY-MM"
            } else {
                "YYYY-MM-DD"
            };
            format!("to_char(to_timestamp(e.created_at) AT TIME ZONE 'UTC', '{format}')")
        }
        DbBackend::MySql => {
            let format = if granularity == "month" {
                "%Y-%m"
            } else {
                "%Y-%m-%d"
            };
            format!(
                "DATE_FORMAT(TIMESTAMPADD(SECOND, e.created_at, '1970-01-01 00:00:00'), '{format}')"
            )
        }
    }
}

fn parse_report_date(
    value: &str,
    format: &[format_description::FormatItem<'_>],
) -> Result<Date, MerchantFinanceError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return Err(MerchantFinanceError::InvalidInput);
    }
    Date::parse(value, format).map_err(|_| MerchantFinanceError::InvalidInput)
}

pub(crate) async fn all<C: ConnectionTrait>(
    connection: &C,
    query: SelectStatement,
) -> Result<Vec<QueryResult>, MerchantFinanceError> {
    connection
        .query_all(connection.get_database_backend().build(&query))
        .await
        .map_err(db_error)
}

pub(crate) fn get<T: sea_orm::TryGetable>(
    row: &QueryResult,
    column: &str,
) -> Result<T, MerchantFinanceError> {
    row.try_get("", column)
        .map_err(|_| MerchantFinanceError::Invariant)
}

pub(crate) fn db_error(_: DbErr) -> MerchantFinanceError {
    MerchantFinanceError::Unavailable
}
pub(crate) fn checked_add(a: i64, b: i64) -> Result<i64, MerchantFinanceError> {
    a.checked_add(b).ok_or(MerchantFinanceError::Invariant)
}
pub(crate) fn now() -> i64 {
    TimeDateTimeWithTimeZone::now_utc().unix_timestamp()
}
fn page_limit(limit: u32) -> Result<u32, MerchantFinanceError> {
    if !(1..=100).contains(&limit) {
        Err(MerchantFinanceError::InvalidInput)
    } else {
        Ok(limit)
    }
}
fn page_query(
    query: &mut SelectStatement,
    limit: u32,
    before_id: Option<i64>,
) -> Result<(), MerchantFinanceError> {
    if let Some(id) = before_id {
        if id <= 0 {
            return Err(MerchantFinanceError::InvalidInput);
        }
        query.and_where(Expr::col(Alias::new("id")).lt(id));
    }
    query
        .order_by(Alias::new("id"), Order::Desc)
        .limit(u64::from(limit) + 1);
    Ok(())
}

pub(crate) async fn ensure_account(
    tx: &DatabaseTransaction,
    merchant_id: i64,
) -> Result<(), MerchantFinanceError> {
    if merchant_id <= 0 {
        return Err(MerchantFinanceError::InvalidInput);
    }
    let insert = Query::insert()
        .into_table(Alias::new(ACCOUNTS))
        .columns([Alias::new("merchant_id")])
        .values_panic([merchant_id.into()])
        .on_conflict(
            OnConflict::column(Alias::new("merchant_id"))
                .do_nothing()
                .to_owned(),
        )
        .to_owned();
    tx.execute(tx.get_database_backend().build(&insert))
        .await
        .map_err(db_error)?;
    Ok(())
}

pub(crate) async fn load_account(
    tx: &DatabaseTransaction,
    merchant_id: i64,
    lock: bool,
) -> Result<MerchantFinanceBalance, MerchantFinanceError> {
    let mut query = select(ACCOUNTS);
    query.and_where(Expr::col(Alias::new("merchant_id")).eq(merchant_id));
    if lock {
        query.lock(LockType::Update);
    }
    let row = tx
        .query_one(tx.get_database_backend().build(&query))
        .await
        .map_err(db_error)?
        .ok_or(MerchantFinanceError::NotFound)?;
    let account = MerchantFinanceBalance {
        merchant_id,
        pending_quota: get(&row, "pending_quota")?,
        available_quota: get(&row, "available_quota")?,
        withdrawing_quota: get(&row, "withdrawing_quota")?,
        frozen_quota: get(&row, "frozen_quota")?,
        debt_quota: get(&row, "debt_quota")?,
        total_gross_quota: get(&row, "total_gross_quota")?,
        total_commission_quota: get(&row, "total_commission_quota")?,
        total_paid_quota: get(&row, "total_paid_quota")?,
        quota_per_usd: 500_000,
    };
    if [
        account.pending_quota,
        account.available_quota,
        account.withdrawing_quota,
        account.frozen_quota,
        account.debt_quota,
        account.total_gross_quota,
        account.total_commission_quota,
        account.total_paid_quota,
    ]
    .into_iter()
    .any(|value| value < 0)
    {
        return Err(MerchantFinanceError::Invariant);
    }
    Ok(account)
}

pub(crate) async fn save_account(
    tx: &DatabaseTransaction,
    account: &MerchantFinanceBalance,
    at: i64,
) -> Result<(), MerchantFinanceError> {
    let update = Query::update()
        .table(Alias::new(ACCOUNTS))
        .values([
            (Alias::new("pending_quota"), account.pending_quota.into()),
            (
                Alias::new("available_quota"),
                account.available_quota.into(),
            ),
            (
                Alias::new("withdrawing_quota"),
                account.withdrawing_quota.into(),
            ),
            (Alias::new("frozen_quota"), account.frozen_quota.into()),
            (Alias::new("debt_quota"), account.debt_quota.into()),
            (
                Alias::new("total_gross_quota"),
                account.total_gross_quota.into(),
            ),
            (
                Alias::new("total_commission_quota"),
                account.total_commission_quota.into(),
            ),
            (
                Alias::new("total_paid_quota"),
                account.total_paid_quota.into(),
            ),
            (Alias::new("updated_at"), at.into()),
        ])
        .and_where(Expr::col(Alias::new("merchant_id")).eq(account.merchant_id))
        .to_owned();
    tx.execute(tx.get_database_backend().build(&update))
        .await
        .map_err(db_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn append_entry(
    tx: &DatabaseTransaction,
    merchant_id: i64,
    event: &str,
    kind: &str,
    reference: &str,
    gross: i64,
    commission: i64,
    net: i64,
    at: i64,
) -> Result<(), MerchantFinanceError> {
    let insert = Query::insert()
        .into_table(Alias::new(ENTRIES))
        .columns(
            [
                "event_key",
                "merchant_id",
                "kind",
                "reference",
                "gross_quota",
                "commission_quota",
                "net_quota",
                "created_at",
            ]
            .map(Alias::new),
        )
        .values_panic([
            event.into(),
            merchant_id.into(),
            kind.into(),
            reference.into(),
            gross.into(),
            commission.into(),
            net.into(),
            at.into(),
        ])
        .to_owned();
    tx.execute(tx.get_database_backend().build(&insert))
        .await
        .map_err(db_error)?;
    Ok(())
}

fn entry_from_row(row: &QueryResult) -> Result<MerchantFinanceEntry, MerchantFinanceError> {
    Ok(MerchantFinanceEntry {
        id: get(row, "id")?,
        kind: get(row, "kind")?,
        reference: get(row, "reference")?,
        gross_quota: get(row, "gross_quota")?,
        commission_quota: get(row, "commission_quota")?,
        net_quota: get(row, "net_quota")?,
        created_at: get(row, "created_at")?,
    })
}
fn withdrawal_from_row(row: &QueryResult) -> Result<MerchantWithdrawal, MerchantFinanceError> {
    Ok(MerchantWithdrawal {
        id: get(row, "id")?,
        event_key: get(row, "event_key")?,
        merchant_id: get(row, "merchant_id")?,
        amount_quota: get(row, "amount_quota")?,
        status: get(row, "status")?,
        reference: get(row, "reference")?,
        reason: get(row, "reason")?,
        created_at: get(row, "created_at")?,
        updated_at: get(row, "updated_at")?,
        payout: None,
    })
}
async fn withdrawal_by_event(
    tx: &DatabaseTransaction,
    event: &str,
) -> Result<Option<MerchantWithdrawal>, MerchantFinanceError> {
    let mut query = select(WITHDRAWALS);
    query.and_where(Expr::col(Alias::new("event_key")).eq(event));
    tx.query_one(tx.get_database_backend().build(&query))
        .await
        .map_err(db_error)?
        .as_ref()
        .map(withdrawal_from_row)
        .transpose()
}
pub(crate) async fn withdrawal_by_id(
    tx: &DatabaseTransaction,
    id: i64,
    lock: bool,
) -> Result<Option<MerchantWithdrawal>, MerchantFinanceError> {
    let mut query = select(WITHDRAWALS);
    query.and_where(Expr::col(Alias::new("id")).eq(id));
    if lock {
        query.lock(LockType::Update);
    }
    tx.query_one(tx.get_database_backend().build(&query))
        .await
        .map_err(db_error)?
        .as_ref()
        .map(withdrawal_from_row)
        .transpose()
}
async fn merchant_reservation<C: ConnectionTrait>(
    connection: &C,
    key: &str,
    lock: bool,
) -> Result<Option<QueryResult>, MerchantFinanceError> {
    let mut query = select(RESERVATIONS);
    query.and_where(Expr::col(Alias::new("reservation_key")).eq(key));
    if lock {
        query.lock(LockType::Update);
    }
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(db_error)
}

fn quota_error(error: MerchantFinanceError) -> QuotaRepositoryError {
    match error {
        MerchantFinanceError::Unavailable => QuotaRepositoryError::Query,
        MerchantFinanceError::InsufficientBalance => QuotaRepositoryError::UserQuotaInsufficient,
        _ => QuotaRepositoryError::Invariant,
    }
}

/// Conservative lower bound: all historical spending and outstanding reservations consume
/// purchased credit first. Gifts, referrals, redemption and opening/admin credit never add backing.
pub(crate) async fn cash_backed_quota<C: ConnectionTrait>(
    connection: &C,
    user_id: i64,
    used: i64,
    frozen: i64,
) -> Result<i64, QuotaRepositoryError> {
    let mut query = Query::select();
    query
        .columns([Alias::new("quota_delta"), Alias::new("entry_type")])
        .from(Alias::new("wallet_ledger_entries"))
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id));
    let rows = connection
        .query_all(connection.get_database_backend().build(&query))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    let mut purchased = 0_i128;
    let mut other_debits = 0_i128;
    for row in rows {
        let delta: i64 = row
            .try_get("", "quota_delta")
            .map_err(|_| QuotaRepositoryError::Invariant)?;
        let kind: i16 = row
            .try_get("", "entry_type")
            .map_err(|_| QuotaRepositoryError::Invariant)?;
        if kind == 3 {
            if delta < 0 {
                return Err(QuotaRepositoryError::Invariant);
            }
            purchased += i128::from(delta);
        } else if delta < 0 {
            other_debits -= i128::from(delta);
        }
    }
    let paid = Query::select()
        .column(Alias::new("quota_amount"))
        .from(Alias::new("topup_orders"))
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id))
        .and_where(Expr::col(Alias::new("organization_id")).is_null())
        .and_where(Expr::col(Alias::new("status")).eq(3_i16))
        .and_where(Expr::col(Alias::new("amount_minor")).gt(0_i64))
        .to_owned();
    let mut confirmed = 0_i128;
    for row in connection
        .query_all(connection.get_database_backend().build(&paid))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?
    {
        let quota: i64 = row
            .try_get("", "quota_amount")
            .map_err(|_| QuotaRepositoryError::Invariant)?;
        if quota < 0 {
            return Err(QuotaRepositoryError::Invariant);
        }
        confirmed += i128::from(quota);
    }
    purchased = purchased.min(confirmed) - other_debits;
    // A reversed merchant sale restores purchased backing without making admin gifts cash.
    let mut refunds = Query::select();
    refunds
        .column((Alias::new(RESERVATIONS), Alias::new("gross_quota")))
        .from(Alias::new(RESERVATIONS))
        .inner_join(
            Alias::new("billing_reservations"),
            Expr::col((Alias::new(RESERVATIONS), Alias::new("reservation_key"))).equals((
                Alias::new("billing_reservations"),
                Alias::new("idempotency_key"),
            )),
        )
        .and_where(
            Expr::col((Alias::new("billing_reservations"), Alias::new("user_id"))).eq(user_id),
        )
        .and_where(Expr::col((Alias::new(RESERVATIONS), Alias::new("status"))).eq("reversed"));
    for row in connection
        .query_all(connection.get_database_backend().build(&refunds))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?
    {
        let restored: i64 = row
            .try_get("", "gross_quota")
            .map_err(|_| QuotaRepositoryError::Invariant)?;
        if restored < 0 {
            return Err(QuotaRepositoryError::Invariant);
        }
        purchased += i128::from(restored);
    }
    cash_lower_bound(purchased.max(0), used, frozen)
}

fn cash_lower_bound(purchased: i128, used: i64, frozen: i64) -> Result<i64, QuotaRepositoryError> {
    if purchased < 0 || used < 0 || frozen < 0 {
        return Err(QuotaRepositoryError::Invariant);
    }
    let eligible = (purchased - i128::from(used) - i128::from(frozen))
        .max(0)
        .min(i128::from(i64::MAX));
    i64::try_from(eligible).map_err(|_| QuotaRepositoryError::Invariant)
}

pub(crate) async fn reserve_merchant_billing(
    tx: &DatabaseTransaction,
    key: &str,
    principal: GatewayPrincipal,
    amount: Quota,
    used: i64,
    frozen: i64,
) -> Result<(), QuotaRepositoryError> {
    let Some(snapshot) = principal.merchant_billing_snapshot() else {
        return Ok(());
    };
    if principal.organization_principal().is_some() {
        return Err(QuotaRepositoryError::Conflict);
    }
    // A candidate may have been selected just before retirement. Serialize its
    // admission with tombstones so deletion cannot race a new in-flight hold.
    for (table, id) in [
        ("merchant_pools", snapshot.pool_id()),
        ("merchant_profiles", snapshot.merchant_id()),
        ("merchant_services", snapshot.service_id()),
    ] {
        let lock = Query::update()
            .table(Alias::new(table))
            .value(Alias::new("id"), Expr::col(Alias::new("id")))
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        tx.execute(tx.get_database_backend().build(&lock))
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        let mut query = Query::select()
            .column(Alias::new("deleted_at"))
            .from(Alias::new(table))
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        if tx.get_database_backend() != sea_orm::DbBackend::Sqlite {
            query.lock(LockType::Update);
        }
        let row = tx
            .query_one(tx.get_database_backend().build(&query))
            .await
            .map_err(|_| QuotaRepositoryError::Query)?
            .ok_or(QuotaRepositoryError::Conflict)?;
        if get::<Option<TimeDateTimeWithTimeZone>>(&row, "deleted_at")
            .map_err(quota_error)?
            .is_some()
        {
            return Err(QuotaRepositoryError::Conflict);
        }
    }
    if cash_backed_quota(tx, principal.user_id().get(), used, frozen).await? < amount.units() {
        return Err(QuotaRepositoryError::MerchantFundingInsufficient);
    }
    let insert = Query::insert()
        .into_table(Alias::new(RESERVATIONS))
        .columns(
            [
                "reservation_key",
                "pool_id",
                "merchant_id",
                "service_id",
                "pricing_version",
                "sales_ratio_micros",
                "commission_bps",
                "settlement_delay_secs",
                "created_at",
            ]
            .map(Alias::new),
        )
        .values_panic([
            key.into(),
            snapshot.pool_id().into(),
            snapshot.merchant_id().into(),
            snapshot.service_id().into(),
            snapshot.pricing_version().into(),
            snapshot.sales_ratio_micros().into(),
            i64::from(snapshot.commission_bps()).into(),
            i64::from(snapshot.settlement_delay_secs()).into(),
            now().into(),
        ])
        .to_owned();
    tx.execute(tx.get_database_backend().build(&insert))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    Ok(())
}

pub(crate) async fn verify_merchant_replay<C: ConnectionTrait>(
    connection: &C,
    key: &str,
    snapshot: Option<MerchantBillingSnapshot>,
) -> Result<(), QuotaRepositoryError> {
    let row = merchant_reservation(connection, key, false)
        .await
        .map_err(quota_error)?;
    match (row, snapshot) {
        (None, None) => Ok(()),
        (Some(row), Some(snapshot)) => {
            let same = get::<i64>(&row, "pool_id").map_err(quota_error)? == snapshot.pool_id()
                && get::<i64>(&row, "merchant_id").map_err(quota_error)? == snapshot.merchant_id()
                && get::<i64>(&row, "service_id").map_err(quota_error)? == snapshot.service_id()
                && get::<i64>(&row, "pricing_version").map_err(quota_error)?
                    == snapshot.pricing_version()
                && get::<i64>(&row, "sales_ratio_micros").map_err(quota_error)?
                    == snapshot.sales_ratio_micros()
                && get::<i64>(&row, "commission_bps").map_err(quota_error)?
                    == i64::from(snapshot.commission_bps())
                && get::<i64>(&row, "settlement_delay_secs").map_err(quota_error)?
                    == i64::from(snapshot.settlement_delay_secs());
            if same {
                Ok(())
            } else {
                Err(QuotaRepositoryError::Conflict)
            }
        }
        _ => Err(QuotaRepositoryError::Conflict),
    }
}

pub(crate) async fn has_merchant_reservation<C: ConnectionTrait>(
    connection: &C,
    key: &str,
) -> Result<bool, QuotaRepositoryError> {
    Ok(merchant_reservation(connection, key, false)
        .await
        .map_err(quota_error)?
        .is_some())
}

pub(crate) async fn cancel_merchant_billing(
    tx: &DatabaseTransaction,
    key: &str,
    rejected_quota: Option<Quota>,
) -> Result<(), QuotaRepositoryError> {
    let update = Query::update()
        .table(Alias::new(RESERVATIONS))
        .value(
            Alias::new("status"),
            if rejected_quota.is_some() {
                "usage_rejected"
            } else {
                "canceled"
            },
        )
        .value(
            Alias::new("reported_quota"),
            rejected_quota.map(Quota::units),
        )
        .and_where(Expr::col(Alias::new("reservation_key")).eq(key))
        .and_where(Expr::col(Alias::new("status")).eq("reserved"))
        .to_owned();
    tx.execute(tx.get_database_backend().build(&update))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    Ok(())
}

pub(crate) async fn settle_merchant_billing(
    tx: &DatabaseTransaction,
    key: &str,
    frozen: i64,
    reserved: Quota,
    actual: Quota,
) -> Result<(), QuotaRepositoryError> {
    let Some(row) = merchant_reservation(tx, key, true)
        .await
        .map_err(quota_error)?
    else {
        return Ok(());
    };
    if actual.units() > reserved.units() {
        return Err(QuotaRepositoryError::MerchantUsageExceedsAuthorization);
    }
    if get::<String>(&row, "status").map_err(quota_error)? != "reserved" {
        return Err(QuotaRepositoryError::Conflict);
    }
    // Purchased backing was checked while the user row was locked at precharge.
    // That money is now physically frozen. Other requests may spend gifts in the
    // meantime and increase used_quota; they cannot invalidate this captured hold.
    // No merchant supplement is permitted, so the original hold fully covers actual.
    if frozen < reserved.units() {
        return Err(QuotaRepositoryError::Invariant);
    }
    let merchant_id: i64 = get(&row, "merchant_id").map_err(quota_error)?;
    let fee: i64 = get(&row, "commission_bps").map_err(quota_error)?;
    if !(0..=10_000).contains(&fee) {
        return Err(QuotaRepositoryError::Invariant);
    }
    let gross = actual.units();
    let commission = i64::try_from(i128::from(gross) * i128::from(fee) / 10_000)
        .map_err(|_| QuotaRepositoryError::Invariant)?;
    let net = gross - commission;
    ensure_account(tx, merchant_id).await.map_err(quota_error)?;
    let mut account = load_account(tx, merchant_id, true)
        .await
        .map_err(quota_error)?;
    account.pending_quota = checked_add(account.pending_quota, net).map_err(quota_error)?;
    account.total_gross_quota =
        checked_add(account.total_gross_quota, gross).map_err(quota_error)?;
    account.total_commission_quota =
        checked_add(account.total_commission_quota, commission).map_err(quota_error)?;
    let at = now();
    let delay: i64 = get(&row, "settlement_delay_secs").map_err(quota_error)?;
    let available_at = at
        .checked_add(delay)
        .ok_or(QuotaRepositoryError::Invariant)?;
    save_account(tx, &account, at).await.map_err(quota_error)?;
    let update = Query::update()
        .table(Alias::new(RESERVATIONS))
        .values([
            (Alias::new("status"), "settled".into()),
            (Alias::new("gross_quota"), gross.into()),
            (Alias::new("commission_quota"), commission.into()),
            (Alias::new("net_quota"), net.into()),
            (Alias::new("available_at"), available_at.into()),
        ])
        .and_where(Expr::col(Alias::new("reservation_key")).eq(key))
        .and_where(Expr::col(Alias::new("status")).eq("reserved"))
        .to_owned();
    if tx
        .execute(tx.get_database_backend().build(&update))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?
        .rows_affected()
        != 1
    {
        return Err(QuotaRepositoryError::Conflict);
    }
    append_entry(
        tx,
        merchant_id,
        &format!("settle:{key}"),
        "income_pending",
        key,
        gross,
        commission,
        net,
        at,
    )
    .await
    .map_err(quota_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gifts_cannot_increase_cash_and_reservations_cannot_double_spend() {
        assert_eq!(cash_lower_bound(1_000, 200, 300).unwrap(), 500);
        assert_eq!(cash_lower_bound(0, 0, 0).unwrap(), 0);
        assert_eq!(cash_lower_bound(1_000, 1_100, 0).unwrap(), 0);
        assert_eq!(cash_lower_bound(1_000, 200, 800).unwrap(), 0);
        assert!(cash_lower_bound(1_000, -1, 0).is_err());
    }
}
