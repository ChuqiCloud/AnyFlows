use af_domain::{BillingReservationId, GroupId, OrganizationId, Quota, TokenId, UserId};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use super::{
    QuotaMutationOutcome, QuotaRepository, QuotaRepositoryError, QuotaReservationKind,
    QuotaReservationStatus, load_reservation,
};

/// Persisted settlement facts for trusted management and recovery adapters.
/// This type deliberately has no serialization or Debug implementation.
pub struct QuotaSettlementSnapshot {
    pub id: BillingReservationId,
    pub organization_id: Option<OrganizationId>,
    pub user_id: UserId,
    pub token_id: TokenId,
    pub group_id: GroupId,
    pub kind: QuotaReservationKind,
    pub status: QuotaReservationStatus,
    pub reserved: Quota,
    pub actual: Option<Quota>,
}

impl QuotaRepository {
    /// Read validated facts with an exact owner scope. `None` selects personal funding.
    /// A reservation outside the requested scope is indistinguishable from a missing one.
    pub async fn settlement_snapshot(
        &self,
        id: BillingReservationId,
        organization: Option<OrganizationId>,
    ) -> Result<QuotaSettlementSnapshot, QuotaRepositoryError> {
        let operation = async {
            let state = load_reservation(self.pool.connection(), id.persistence_key(), false)
                .await?
                .ok_or(QuotaRepositoryError::NotFound)?;
            if state.organization_id != organization.map(OrganizationId::get) {
                return Err(QuotaRepositoryError::NotFound);
            }
            Ok(QuotaSettlementSnapshot {
                id,
                organization_id: organization,
                user_id: UserId::new(state.user_id).map_err(|_| QuotaRepositoryError::Invariant)?,
                token_id: TokenId::new(state.token_id).map_err(|_| QuotaRepositoryError::Invariant)?,
                group_id: GroupId::new(state.group_id).map_err(|_| QuotaRepositoryError::Invariant)?,
                kind: state.reservation_kind,
                status: state.status,
                reserved: Quota::new(state.reserved_quota)
                    .map_err(|_| QuotaRepositoryError::Invariant)?,
                actual: state
                    .actual_quota
                    .map(Quota::new)
                    .transpose()
                    .map_err(|_| QuotaRepositoryError::Invariant)?,
            })
        };
        timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| QuotaRepositoryError::Query)?
    }

    /// Retry a recorded settlement, preserving its original amount and purpose.
    /// Reserved or refunded records cannot be turned into new settlements by recovery.
    pub async fn retry_settlement(
        &self,
        id: BillingReservationId,
        organization: Option<OrganizationId>,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        let snapshot = self.settlement_snapshot(id, organization).await?;
        if !matches!(
            snapshot.status,
            QuotaReservationStatus::SettlementPending | QuotaReservationStatus::Settled
        ) {
            return Err(QuotaRepositoryError::Conflict);
        }
        let actual = snapshot.actual.ok_or(QuotaRepositoryError::Invariant)?;
        self.settle_with_kind(id, actual, snapshot.kind).await
    }
}
