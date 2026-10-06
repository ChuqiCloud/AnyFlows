use std::fmt;

use rust_decimal::Decimal;
use thiserror::Error;

use crate::{BillingContractPriceId, OrganizationId};

/// Immutable token prices captured for one billing request.
///
/// The snapshot is a shared billing contract. Storage and the source that
/// produced it remain outside the domain crate.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BillingContractPriceSnapshot {
    organization_id: OrganizationId,
    id: BillingContractPriceId,
    version: i64,
    prices: [Decimal; 5],
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("合同价快照无效")]
pub struct BillingContractPriceSnapshotError;

impl BillingContractPriceSnapshot {
    /// Prices are USD per million tokens: input, output, read, 5m write, 1h write.
    pub fn new(
        organization_id: OrganizationId,
        id: BillingContractPriceId,
        version: i64,
        prices: [Decimal; 5],
    ) -> Result<Self, BillingContractPriceSnapshotError> {
        if version <= 0 || prices.into_iter().any(|price| price < Decimal::ZERO) {
            return Err(BillingContractPriceSnapshotError);
        }
        Ok(Self {
            organization_id,
            id,
            version,
            prices,
        })
    }

    /// Reconstruct a snapshot from validated persistence fields.
    pub fn from_persistence_parts(
        organization_id: i64,
        id: i64,
        version: i64,
        prices: [Decimal; 5],
    ) -> Result<Self, BillingContractPriceSnapshotError> {
        Self::new(
            OrganizationId::new(organization_id).map_err(|_| BillingContractPriceSnapshotError)?,
            BillingContractPriceId::new(id).map_err(|_| BillingContractPriceSnapshotError)?,
            version,
            prices,
        )
    }

    #[must_use]
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }

    #[must_use]
    pub const fn id(self) -> BillingContractPriceId {
        self.id
    }

    #[must_use]
    pub const fn version(self) -> i64 {
        self.version
    }

    #[must_use]
    pub const fn prices(self) -> [Decimal; 5] {
        self.prices
    }
}

impl fmt::Debug for BillingContractPriceSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingContractPriceSnapshot")
            .field("organization_id", &self.organization_id)
            .field("id", &self.id)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_validate_version_and_prices() {
        let organization = OrganizationId::new(1).unwrap();
        let id = BillingContractPriceId::new(2).unwrap();
        let prices = [Decimal::ONE; 5];
        let snapshot = BillingContractPriceSnapshot::new(organization, id, 3, prices).unwrap();
        assert_eq!(snapshot.organization_id(), organization);
        assert_eq!(snapshot.id(), id);
        assert_eq!(snapshot.version(), 3);
        assert_eq!(snapshot.prices(), prices);
        for version in [0, -1] {
            assert!(BillingContractPriceSnapshot::new(organization, id, version, prices).is_err());
        }
        for index in 0..5 {
            let mut invalid = prices;
            invalid[index] = -Decimal::ONE;
            assert!(BillingContractPriceSnapshot::new(organization, id, 1, invalid).is_err());
        }
    }

    #[test]
    fn persistence_parts_preserve_billing_identity_and_reject_invalid_ids() {
        let prices = [Decimal::ONE; 5];
        let snapshot =
            BillingContractPriceSnapshot::from_persistence_parts(1, 2, 3, prices).unwrap();
        assert_eq!(snapshot.organization_id().get(), 1);
        assert_eq!(snapshot.id().get(), 2);
        assert_eq!(snapshot.version(), 3);
        assert_eq!(snapshot.prices(), prices);
        for (owner, id) in [(0, 2), (-1, 2), (1, 0), (1, -1)] {
            assert!(
                BillingContractPriceSnapshot::from_persistence_parts(owner, id, 3, prices).is_err()
            );
        }
    }
}
