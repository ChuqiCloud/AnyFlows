mod material;
mod repository;
mod types;

pub use material::{
    IssuedRedemptionBatch, IssuedRedemptionCode, PresentedRedemptionCode, RedemptionCodeDefinition,
    RedemptionCodeDigest, RedemptionMaterialError,
};
pub use repository::RedemptionRepository;
pub use types::{
    MAX_REDEMPTION_BATCH_CODES, MAX_REDEMPTION_BATCH_NAME_BYTES, MAX_REDEMPTION_BATCH_PAGE_SIZE,
    RedemptionAttempt, RedemptionBatchCreateOutcome, RedemptionBatchDisable,
    RedemptionBatchDisableOutcome, RedemptionBatchListRecord, RedemptionBatchPageRecord,
    RedemptionBatchRecord, RedemptionBatchWrite, RedemptionInputError, RedemptionOutcome,
    RedemptionRecord, RedemptionRejection, RedemptionRepositoryConfigError,
    RedemptionRepositoryError,
};

#[cfg(test)]
mod tests;
