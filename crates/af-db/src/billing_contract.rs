use std::fmt;

use af_domain::{OrganizationContractPriceId, OrganizationId};
use rust_decimal::Decimal;
use thiserror::Error;

/// Immutable price facts carried across the billing extension boundary.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct OrganizationContractPriceSnapshot {
    organization_id: OrganizationId,
    id: OrganizationContractPriceId,
    version: i64,
    prices: [Decimal; 5],
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("Invalid billing contract price snapshot")]
pub struct BillingContractPriceSnapshotError;

impl OrganizationContractPriceSnapshot {
    /// Prices are USD per million tokens: input, output, read, 5m write, 1h write.
    pub fn new(
        organization_id: OrganizationId,
        id: OrganizationContractPriceId,
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

    pub(crate) fn from_parts(
        organization_id: i64,
        id: i64,
        version: i64,
        prices: [Decimal; 5],
    ) -> Result<Self, BillingContractPriceSnapshotError> {
        Self::new(
            OrganizationId::new(organization_id).map_err(|_| BillingContractPriceSnapshotError)?,
            OrganizationContractPriceId::new(id).map_err(|_| BillingContractPriceSnapshotError)?,
            version,
            prices,
        )
    }

    #[must_use]
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }

    #[must_use]
    pub const fn id(self) -> OrganizationContractPriceId {
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

impl fmt::Debug for OrganizationContractPriceSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationContractPriceSnapshot")
            .field("organization_id", &self.organization_id)
            .field("id", &self.id)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}
