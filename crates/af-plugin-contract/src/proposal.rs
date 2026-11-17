use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CapabilityId, ContractValidationError};

/// A plugin's requested host action. This is only a proposal: decoding it never
/// executes the action, confirms a user intent, or grants write authority.
///
/// The host must resolve `action_id` through its fixed action registry, validate
/// `input` against that action's host-owned schema, recheck the authenticated
/// principal and target resource, and obtain any required confirmation before
/// creating a host-owned operation record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostActionProposal {
    pub action_id: CapabilityId,
    pub action_version: u16,
    pub input: Value,
}

impl HostActionProposal {
    pub const MAX_INPUT_BYTES: usize = 64 * 1024;

    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.action_version == 0 || !self.input.is_object() {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        let input = serde_json::to_vec(&self.input)
            .map_err(|_| ContractValidationError::InvalidExternalValue)?;
        if input.len() > Self::MAX_INPUT_BYTES {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposal_is_a_bounded_typed_request_not_an_execution_grant() {
        let proposal = HostActionProposal {
            action_id: CapabilityId::new("channel.config.draft").unwrap(),
            action_version: 1,
            input: serde_json::json!({"suggestedTimeoutMs": 2_000}),
        };
        proposal.validate().unwrap();

        let wire = serde_json::to_value(&proposal).unwrap();
        assert_eq!(wire["actionId"], "channel.config.draft");
        assert_eq!(wire["actionVersion"], 1);
        assert!(wire.get("approved").is_none());
        assert!(
            serde_json::from_value::<HostActionProposal>(serde_json::json!({
                "actionId":"channel.config.draft",
                "actionVersion":1,
                "input":{},
                "approved":true
            }))
            .is_err()
        );
    }

    #[test]
    fn proposal_rejects_invalid_versions_non_objects_and_oversized_input() {
        let mut proposal = HostActionProposal {
            action_id: CapabilityId::new("channel.config.draft").unwrap(),
            action_version: 1,
            input: serde_json::json!({}),
        };
        proposal.action_version = 0;
        assert_eq!(
            proposal.validate(),
            Err(ContractValidationError::InvalidExternalValue)
        );
        proposal.action_version = 1;
        proposal.input = serde_json::json!([]);
        assert_eq!(
            proposal.validate(),
            Err(ContractValidationError::InvalidExternalValue)
        );
        proposal.input = serde_json::json!({
            "payload": "x".repeat(HostActionProposal::MAX_INPUT_BYTES)
        });
        assert_eq!(
            proposal.validate(),
            Err(ContractValidationError::InvalidExternalValue)
        );
    }
}
