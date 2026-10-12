use std::sync::Arc;

use af_domain::{BillingReservationId, OrganizationServiceAccountRuntimeIdentity, Quota};

use crate::{
    BillingPrechargeFuture, BillingSettlementFuture, BillingSettlementPort, RefundSignalOutcome,
    RefundSignalPort, SettlementRequest,
};

/// Distribution-owned funds for a service identity. No personal token is accepted.
pub trait ServiceAccountBillingPort: Send + Sync + 'static {
    /// The absolute expiry is fixed by the caller and must be reused on retries.
    fn precharge<'a>(
        &'a self,
        reservation: BillingReservationId,
        identity: OrganizationServiceAccountRuntimeIdentity,
        amount: Quota,
        expires_at: i64,
    ) -> BillingPrechargeFuture<'a>;

    /// Terminal operations use the original identity, including after key rotation.
    fn settle<'a>(
        &'a self,
        identity: OrganizationServiceAccountRuntimeIdentity,
        request: SettlementRequest,
    ) -> BillingSettlementFuture<'a>;

    /// Non-blocking compensation for a confirmed reservation. No IO is allowed here.
    fn try_signal_refund(
        &self,
        identity: OrganizationServiceAccountRuntimeIdentity,
        reservation: BillingReservationId,
    ) -> RefundSignalOutcome;
}

pub(crate) struct BoundServiceAccountBilling {
    pub identity: OrganizationServiceAccountRuntimeIdentity,
    pub port: Arc<dyn ServiceAccountBillingPort>,
}

impl BillingSettlementPort for BoundServiceAccountBilling {
    fn settle<'a>(&'a self, request: SettlementRequest) -> BillingSettlementFuture<'a> {
        self.port.settle(self.identity, request)
    }
}

impl RefundSignalPort for BoundServiceAccountBilling {
    fn try_signal_refund(&self, reservation: BillingReservationId) -> RefundSignalOutcome {
        self.port.try_signal_refund(self.identity, reservation)
    }
}
