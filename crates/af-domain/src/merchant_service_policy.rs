//! Supply pricing schedules never modify reviewed upstream configuration.
use crate::{MerchantPoolError, validate_merchant_text};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantModelRatio {
    pub model: String,
    pub ratio_micros: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantServicePriceWrite {
    pub expected_revision_id: i64,
    pub effective_at: i64,
    pub default_ratio_micros: i64,
    pub model_ratios: Vec<MerchantModelRatio>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerchantServicePrice {
    pub id: i64,
    pub service_id: i64,
    pub revision_id: i64,
    pub effective_at: i64,
    pub default_ratio_micros: i64,
    pub model_ratios: Vec<MerchantModelRatio>,
    pub status: String,
    pub reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantMaintenanceWrite {
    pub starts_at: i64,
    pub ends_at: i64,
    pub message: String,
    pub blocks_requests: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerchantMaintenance {
    pub id: i64,
    pub service_id: i64,
    pub starts_at: i64,
    pub ends_at: i64,
    pub message: String,
    pub blocks_requests: bool,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl MerchantServicePriceWrite {
    pub fn validate(&self, now: i64) -> Result<(), MerchantPoolError> {
        if self.expected_revision_id <= 0
            || self.effective_at < now.saturating_sub(60)
            || self.effective_at > now.saturating_add(90 * 86_400)
            || !(1..=100_000_000).contains(&self.default_ratio_micros)
            || self.model_ratios.len() > 128
        {
            return Err(MerchantPoolError::InvalidInput);
        }
        let mut names = std::collections::BTreeSet::new();
        if self.model_ratios.iter().any(|model| {
            !validate_merchant_text(&model.model, 255, false)
                || !names.insert(&model.model)
                || model
                    .ratio_micros
                    .is_some_and(|ratio| !(1..=100_000_000).contains(&ratio))
        }) {
            return Err(MerchantPoolError::InvalidInput);
        }
        Ok(())
    }
}

impl MerchantMaintenanceWrite {
    pub fn validate(&self, now: i64) -> Result<(), MerchantPoolError> {
        if self.starts_at < 0
            || self.ends_at <= now
            || self.ends_at <= self.starts_at
            || self.ends_at.saturating_sub(self.starts_at) > 30 * 86_400
            || self.ends_at > now.saturating_add(90 * 86_400)
            || !validate_merchant_text(&self.message, 2048, false)
        {
            return Err(MerchantPoolError::InvalidInput);
        }
        Ok(())
    }
}
