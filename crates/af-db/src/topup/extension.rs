use std::{future::Future, pin::Pin};

use af_domain::{OrganizationId, Quota, UserId, WalletEventId};
use sea_orm::{DatabaseTransaction, entity::prelude::TimeDateTimeWithTimeZone};

use super::{TopupOrderRecord, TopupRepositoryError};

pub type TopupExtensionFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, TopupRepositoryError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopupCreditOutcome {
    Credited,
    Overflow,
}

/// Distribution-owned operations for non-personal wallet targets.
/// The core owns the payment order transaction and event audit.
pub trait TopupExtension: Send + Sync {
    fn validate_target<'a>(
        &'a self,
        transaction: &'a DatabaseTransaction,
        organization_id: OrganizationId,
    ) -> TopupExtensionFuture<'a, bool>;

    /// Validate the payer and target before the core locks the payer account.
    /// Extensions may lock their target first to match their management lock order.
    /// This runs for both new orders and idempotent retries.
    fn validate_order<'a>(
        &'a self,
        transaction: &'a DatabaseTransaction,
        organization_id: OrganizationId,
        _payer_user_id: UserId,
    ) -> TopupExtensionFuture<'a, bool> {
        self.validate_target(transaction, organization_id)
    }

    /// Persist extension-owned audit facts in the order creation transaction.
    /// Called only for a newly inserted order; failure rolls back the order.
    fn order_created<'a>(
        &'a self,
        _transaction: &'a DatabaseTransaction,
        _order: &'a TopupOrderRecord,
    ) -> TopupExtensionFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn credit<'a>(
        &'a self,
        transaction: &'a DatabaseTransaction,
        organization_id: OrganizationId,
        event_id: WalletEventId,
        payer_user_id: UserId,
        amount: Quota,
        created_at: TimeDateTimeWithTimeZone,
    ) -> TopupExtensionFuture<'a, TopupCreditOutcome>;
}
