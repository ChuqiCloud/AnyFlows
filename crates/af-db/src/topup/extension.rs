use std::{future::Future, pin::Pin};

use af_domain::{OrganizationId, Quota, UserId, WalletEventId};
use sea_orm::{DatabaseTransaction, entity::prelude::TimeDateTimeWithTimeZone};

use super::TopupRepositoryError;

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
