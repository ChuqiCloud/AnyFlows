//! Immutable commercial terms for one approved merchant service attempt.

use thiserror::Error;

/// Validated terms captured before reserving a consumer's money.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MerchantBillingSnapshot {
    pool_id: i64,
    merchant_id: i64,
    service_id: i64,
    pricing_version: i64,
    sales_ratio_micros: i64,
    commission_bps: u16,
    settlement_delay_secs: u32,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("商户计费条款无效")]
pub struct MerchantBillingSnapshotError;

impl MerchantBillingSnapshot {
    pub const fn new(
        pool_id: i64,
        merchant_id: i64,
        service_id: i64,
        pricing_version: i64,
        sales_ratio_micros: i64,
        commission_bps: u16,
        settlement_delay_secs: u32,
    ) -> Result<Self, MerchantBillingSnapshotError> {
        if pool_id <= 0
            || merchant_id <= 0
            || service_id <= 0
            || pricing_version <= 0
            || sales_ratio_micros <= 0
            || commission_bps > 10_000
            || settlement_delay_secs > 90 * 86_400
        {
            return Err(MerchantBillingSnapshotError);
        }
        Ok(Self {
            pool_id,
            merchant_id,
            service_id,
            pricing_version,
            sales_ratio_micros,
            commission_bps,
            settlement_delay_secs,
        })
    }

    #[must_use]
    pub const fn pool_id(self) -> i64 {
        self.pool_id
    }
    #[must_use]
    pub const fn merchant_id(self) -> i64 {
        self.merchant_id
    }
    #[must_use]
    pub const fn service_id(self) -> i64 {
        self.service_id
    }
    #[must_use]
    pub const fn pricing_version(self) -> i64 {
        self.pricing_version
    }
    #[must_use]
    pub const fn sales_ratio_micros(self) -> i64 {
        self.sales_ratio_micros
    }
    #[must_use]
    pub const fn commission_bps(self) -> u16 {
        self.commission_bps
    }
    #[must_use]
    pub const fn settlement_delay_secs(self) -> u32 {
        self.settlement_delay_secs
    }

    /// Round only the commission; merchant receives the exact remainder.
    pub fn split(self, gross_quota: i64) -> Result<(i64, i64), MerchantBillingSnapshotError> {
        if gross_quota < 0 {
            return Err(MerchantBillingSnapshotError);
        }
        let commission =
            i64::try_from(i128::from(gross_quota) * i128::from(self.commission_bps) / 10_000)
                .map_err(|_| MerchantBillingSnapshotError)?;
        Ok((commission, gross_quota - commission))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_preserve_all_units_including_rounding_and_maximum() {
        let terms = MerchantBillingSnapshot::new(1, 2, 3, 4, 800_000, 333, 86_400).unwrap();
        for gross in [0, 1, 31, 10_000, i64::MAX] {
            let (commission, merchant) = terms.split(gross).unwrap();
            assert_eq!(commission.checked_add(merchant), Some(gross));
        }
        assert_eq!(terms.split(10_000).unwrap(), (333, 9_667));
        assert!(terms.split(-1).is_err());
        assert!(MerchantBillingSnapshot::new(1, 2, 3, 4, 0, 500, 0).is_err());
        assert!(MerchantBillingSnapshot::new(1, 2, 3, 4, 1, 10_001, 0).is_err());
    }
}
