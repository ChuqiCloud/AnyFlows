use sea_orm::QueryResult;

use super::types::{BillingBatchRepositoryError, BillingBatchWrite};

pub(super) struct CheckpointState {
    pub(super) last_start_sequence: i64,
    pub(super) last_end_sequence: i64,
    pub(super) last_event_count: i64,
    pub(super) last_fingerprint: String,
}

impl CheckpointState {
    pub(super) fn try_from_result(
        result: &QueryResult,
    ) -> Result<Self, BillingBatchRepositoryError> {
        let state = Self {
            last_start_sequence: get(result, "last_start_sequence")?,
            last_end_sequence: get(result, "last_end_sequence")?,
            last_event_count: get(result, "last_event_count")?,
            last_fingerprint: get(result, "last_fingerprint")?,
        };
        state.validate()?;
        Ok(state)
    }

    pub(super) fn matches(&self, batch: &BillingBatchWrite) -> bool {
        self.last_start_sequence == batch.start_sequence()
            && self.last_end_sequence == batch.end_sequence()
            && self.last_event_count == batch.event_count()
            && self.last_fingerprint == batch.fingerprint_key()
    }

    pub(super) fn accepts_next(&self, batch: &BillingBatchWrite) -> bool {
        self.last_end_sequence.checked_add(1) == Some(batch.start_sequence())
    }

    fn validate(&self) -> Result<(), BillingBatchRepositoryError> {
        let valid_fingerprint = self.last_fingerprint.len() == 64
            && self
                .last_fingerprint
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
        let valid_range = self.last_start_sequence > 0
            && self.last_end_sequence >= self.last_start_sequence
            && self.last_event_count > 0
            && self
                .last_end_sequence
                .checked_sub(self.last_start_sequence)
                .and_then(|length| length.checked_add(1))
                == Some(self.last_event_count);
        if valid_fingerprint && valid_range {
            Ok(())
        } else {
            Err(BillingBatchRepositoryError::Invariant)
        }
    }
}

fn get<T>(result: &QueryResult, column: &str) -> Result<T, BillingBatchRepositoryError>
where
    T: sea_orm::TryGetable,
{
    result
        .try_get("", column)
        .map_err(|_| BillingBatchRepositoryError::Invariant)
}
