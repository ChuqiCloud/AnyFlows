use std::{future::Future, pin::Pin};

use af_domain::{
    BillingReservationId, GatewayPrincipal, GroupId, OrganizationId, Quota, TokenId, UserId,
};
use sea_orm::{DatabaseTransaction, entity::prelude::TimeDateTimeWithTimeZone};

use super::{QuotaRepositoryError, QuotaReservationKind, state::ReservationState};

/// Immutable core reservation facts passed to a distribution funding adapter.
pub struct QuotaFundingContext {
    pub id: BillingReservationId,
    pub organization_id: OrganizationId,
    pub user_id: UserId,
    pub token_id: TokenId,
    pub group_id: GroupId,
    pub reserved: Quota,
    pub kind: QuotaReservationKind,
    pub now: TimeDateTimeWithTimeZone,
    pub expires_at: Option<TimeDateTimeWithTimeZone>,
}

pub type QuotaFundingFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), QuotaRepositoryError>> + Send + 'a>>;

/// Replace funding inside the core transaction without bypassing key or group limits.
///
/// Hooks must use the supplied transaction and must never commit it. `lock` runs
/// before the core subject locks; settlement runs inside a rollback savepoint.
/// A failed supplement retains the actual amount and calls `pending` outside
/// that savepoint. Terminal replays never repeat settlement or refund hooks.
pub trait QuotaFundingExtension: Send + Sync {
    fn lock<'a>(
        &'a self,
        transaction: &'a DatabaseTransaction,
        context: &'a QuotaFundingContext,
    ) -> QuotaFundingFuture<'a>;
    fn reserve<'a>(
        &'a self,
        transaction: &'a DatabaseTransaction,
        context: &'a QuotaFundingContext,
        principal: GatewayPrincipal,
    ) -> QuotaFundingFuture<'a>;
    fn replay<'a>(
        &'a self,
        transaction: &'a DatabaseTransaction,
        context: &'a QuotaFundingContext,
        principal: GatewayPrincipal,
    ) -> QuotaFundingFuture<'a>;
    fn settle<'a>(
        &'a self,
        transaction: &'a DatabaseTransaction,
        context: &'a QuotaFundingContext,
        actual: Quota,
    ) -> QuotaFundingFuture<'a>;
    fn pending<'a>(
        &'a self,
        transaction: &'a DatabaseTransaction,
        context: &'a QuotaFundingContext,
        actual: Quota,
    ) -> QuotaFundingFuture<'a>;
    fn refund<'a>(
        &'a self,
        transaction: &'a DatabaseTransaction,
        context: &'a QuotaFundingContext,
    ) -> QuotaFundingFuture<'a>;
}

pub(super) fn context(
    id: BillingReservationId,
    state: &ReservationState,
    now: TimeDateTimeWithTimeZone,
) -> Result<Option<QuotaFundingContext>, QuotaRepositoryError> {
    state
        .organization_id
        .map(|organization| {
            Ok(QuotaFundingContext {
                id,
                organization_id: OrganizationId::new(organization)
                    .map_err(|_| QuotaRepositoryError::Invariant)?,
                user_id: UserId::new(state.user_id).map_err(|_| QuotaRepositoryError::Invariant)?,
                token_id: TokenId::new(state.token_id)
                    .map_err(|_| QuotaRepositoryError::Invariant)?,
                group_id: GroupId::new(state.group_id)
                    .map_err(|_| QuotaRepositoryError::Invariant)?,
                reserved: Quota::new(state.reserved_quota)
                    .map_err(|_| QuotaRepositoryError::Invariant)?,
                kind: state.reservation_kind,
                now,
                expires_at: None,
            })
        })
        .transpose()
}
