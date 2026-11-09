//! Session-owned merchant proceeds and administrator payout decisions.

use std::{future::Future, pin::Pin};

pub use af_db::{
    MerchantFinanceBalance, MerchantFinanceEntry, MerchantFinanceError, MerchantFinancePage,
    MerchantWithdrawal,
};
use af_db::{MerchantFinanceRepository, MerchantPoolRepository};

use crate::{SessionPrincipal, SessionRole};

pub type MerchantFinanceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, MerchantFinanceError>> + Send + 'a>>;

pub trait MerchantFinanceService: Send + Sync {
    fn balance(
        &self,
        principal: SessionPrincipal,
    ) -> MerchantFinanceFuture<'_, MerchantFinanceBalance>;
    fn entries(
        &self,
        principal: SessionPrincipal,
        limit: u32,
        before_id: Option<i64>,
    ) -> MerchantFinanceFuture<'_, MerchantFinancePage<MerchantFinanceEntry>>;
    fn withdrawals(
        &self,
        principal: SessionPrincipal,
        limit: u32,
        before_id: Option<i64>,
    ) -> MerchantFinanceFuture<'_, MerchantFinancePage<MerchantWithdrawal>>;
    fn request_withdrawal(
        &self,
        principal: SessionPrincipal,
        event_key: String,
        amount_quota: i64,
    ) -> MerchantFinanceFuture<'_, MerchantWithdrawal>;
    fn admin_withdrawals(
        &self,
        principal: SessionPrincipal,
        status: Option<String>,
        limit: u32,
        before_id: Option<i64>,
    ) -> MerchantFinanceFuture<'_, MerchantFinancePage<MerchantWithdrawal>>;
    fn decide_withdrawal(
        &self,
        principal: SessionPrincipal,
        id: i64,
        status: String,
        reference: String,
        reason: Option<String>,
    ) -> MerchantFinanceFuture<'_, MerchantWithdrawal>;
    fn reverse_settlement(
        &self,
        principal: SessionPrincipal,
        reservation_key: String,
        reason: String,
    ) -> MerchantFinanceFuture<'_, ()>;
    fn hold_balance(
        &self,
        principal: SessionPrincipal,
        merchant_id: i64,
        event_key: String,
        amount_quota: i64,
        freeze: bool,
        reason: String,
    ) -> MerchantFinanceFuture<'_, MerchantFinanceBalance>;
    fn admin_balance(
        &self,
        principal: SessionPrincipal,
        merchant_id: i64,
    ) -> MerchantFinanceFuture<'_, MerchantFinanceBalance>;
    fn admin_entries(
        &self,
        principal: SessionPrincipal,
        merchant_id: i64,
        limit: u32,
        before_id: Option<i64>,
    ) -> MerchantFinanceFuture<'_, MerchantFinancePage<MerchantFinanceEntry>>;
}

pub struct DatabaseMerchantFinanceService {
    merchants: MerchantPoolRepository,
    finance: MerchantFinanceRepository,
}

impl DatabaseMerchantFinanceService {
    #[must_use]
    pub const fn new(
        merchants: MerchantPoolRepository,
        finance: MerchantFinanceRepository,
    ) -> Self {
        Self { merchants, finance }
    }

    async fn owner_merchant(
        &self,
        principal: SessionPrincipal,
    ) -> Result<i64, MerchantFinanceError> {
        self.merchants
            .owner_merchant(principal.user_id())
            .await
            .map_err(|_| MerchantFinanceError::Unavailable)?
            .map(|merchant| merchant.id)
            .ok_or(MerchantFinanceError::NotFound)
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), MerchantFinanceError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(MerchantFinanceError::Forbidden)
    }
}

impl MerchantFinanceService for DatabaseMerchantFinanceService {
    fn balance(
        &self,
        principal: SessionPrincipal,
    ) -> MerchantFinanceFuture<'_, MerchantFinanceBalance> {
        Box::pin(async move {
            self.finance
                .balance(self.owner_merchant(principal).await?)
                .await
        })
    }
    fn entries(
        &self,
        principal: SessionPrincipal,
        limit: u32,
        before_id: Option<i64>,
    ) -> MerchantFinanceFuture<'_, MerchantFinancePage<MerchantFinanceEntry>> {
        Box::pin(async move {
            self.finance
                .entries(self.owner_merchant(principal).await?, limit, before_id)
                .await
        })
    }
    fn withdrawals(
        &self,
        principal: SessionPrincipal,
        limit: u32,
        before_id: Option<i64>,
    ) -> MerchantFinanceFuture<'_, MerchantFinancePage<MerchantWithdrawal>> {
        Box::pin(async move {
            self.finance
                .withdrawals(
                    Some(self.owner_merchant(principal).await?),
                    None,
                    limit,
                    before_id,
                )
                .await
        })
    }
    fn request_withdrawal(
        &self,
        principal: SessionPrincipal,
        event_key: String,
        amount_quota: i64,
    ) -> MerchantFinanceFuture<'_, MerchantWithdrawal> {
        Box::pin(async move {
            self.finance
                .request_withdrawal(
                    self.owner_merchant(principal).await?,
                    &event_key,
                    amount_quota,
                )
                .await
        })
    }
    fn admin_withdrawals(
        &self,
        principal: SessionPrincipal,
        status: Option<String>,
        limit: u32,
        before_id: Option<i64>,
    ) -> MerchantFinanceFuture<'_, MerchantFinancePage<MerchantWithdrawal>> {
        Box::pin(async move {
            require_admin(principal)?;
            self.finance
                .withdrawals(None, status.as_deref(), limit, before_id)
                .await
        })
    }
    fn decide_withdrawal(
        &self,
        principal: SessionPrincipal,
        id: i64,
        status: String,
        reference: String,
        reason: Option<String>,
    ) -> MerchantFinanceFuture<'_, MerchantWithdrawal> {
        Box::pin(async move {
            require_admin(principal)?;
            self.finance
                .decide_withdrawal(
                    id,
                    principal.user_id(),
                    &status,
                    &reference,
                    reason.as_deref(),
                )
                .await
        })
    }
    fn reverse_settlement(
        &self,
        principal: SessionPrincipal,
        reservation_key: String,
        reason: String,
    ) -> MerchantFinanceFuture<'_, ()> {
        Box::pin(async move {
            require_admin(principal)?;
            self.finance
                .reverse_settlement(&reservation_key, principal.user_id(), &reason)
                .await
        })
    }
    fn hold_balance(
        &self,
        principal: SessionPrincipal,
        merchant_id: i64,
        event_key: String,
        amount_quota: i64,
        freeze: bool,
        reason: String,
    ) -> MerchantFinanceFuture<'_, MerchantFinanceBalance> {
        Box::pin(async move {
            require_admin(principal)?;
            self.merchants
                .get_merchant(merchant_id)
                .await
                .map_err(|_| MerchantFinanceError::NotFound)?;
            self.finance
                .hold_balance(
                    merchant_id,
                    principal.user_id(),
                    &event_key,
                    amount_quota,
                    freeze,
                    &reason,
                )
                .await
        })
    }
    fn admin_balance(
        &self,
        principal: SessionPrincipal,
        merchant_id: i64,
    ) -> MerchantFinanceFuture<'_, MerchantFinanceBalance> {
        Box::pin(async move {
            require_admin(principal)?;
            self.merchants
                .get_merchant(merchant_id)
                .await
                .map_err(|_| MerchantFinanceError::NotFound)?;
            self.finance.balance(merchant_id).await
        })
    }
    fn admin_entries(
        &self,
        principal: SessionPrincipal,
        merchant_id: i64,
        limit: u32,
        before_id: Option<i64>,
    ) -> MerchantFinanceFuture<'_, MerchantFinancePage<MerchantFinanceEntry>> {
        Box::pin(async move {
            require_admin(principal)?;
            self.merchants
                .get_merchant(merchant_id)
                .await
                .map_err(|_| MerchantFinanceError::NotFound)?;
            self.finance.entries(merchant_id, limit, before_id).await
        })
    }
}
