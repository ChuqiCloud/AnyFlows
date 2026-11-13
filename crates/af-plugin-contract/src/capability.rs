use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ContractValidationError;

/// Opaque, stable capability name. Versioning is carried separately.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct CapabilityId(String);

impl CapabilityId {
    pub fn new(value: impl Into<String>) -> Result<Self, ContractValidationError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 128
            && value.split('.').all(|segment| {
                !segment.is_empty()
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
                    && segment.as_bytes()[0].is_ascii_lowercase()
                    && segment.as_bytes()[segment.len() - 1].is_ascii_alphanumeric()
            });
        if !valid {
            return Err(ContractValidationError::InvalidIdentifier);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for CapabilityId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CapabilityId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Capability access direction declared by an adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAccess {
    Read,
    Write,
}

/// Side-effect class is explicit so read-only observation cannot imply writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectClass {
    None,
    ExternalRead,
    ExternalWrite,
}

/// Recovery guarantees for a connector operation that writes to an upstream.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalWriteSemantics {
    /// Reusing the same key is guaranteed to return the original result.
    pub native_idempotency_key: bool,
    /// How an outcome can be queried after a timeout that may follow a send.
    pub result_lookup: ResultLookupSupport,
    /// An external record exists for later reconciliation.
    pub reconciliation: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultLookupSupport {
    None,
    ByRequestId,
    ByExternalReference,
}

impl ExternalWriteSemantics {
    #[must_use]
    pub const fn retry_after_unknown_outcome_is_safe(self) -> bool {
        self.native_idempotency_key
    }
}

/// A capability supported by a plugin and its contract version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDeclaration {
    pub id: CapabilityId,
    pub version: u16,
    pub access: CapabilityAccess,
    pub side_effect: SideEffectClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_semantics: Option<ExternalWriteSemantics>,
}

impl CapabilityDeclaration {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.version == 0
            || (self.access == CapabilityAccess::Read
                && self.side_effect == SideEffectClass::ExternalWrite)
            || (self.side_effect == SideEffectClass::ExternalWrite
                && self.write_semantics.is_none())
            || (self.side_effect != SideEffectClass::ExternalWrite
                && self.write_semantics.is_some())
        {
            return Err(ContractValidationError::InvalidCapabilityVersion);
        }
        Ok(())
    }
}

/// Capability a host requires from a connector during negotiation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityRequirement {
    id: CapabilityId,
    minimum_version: u16,
    maximum_version: u16,
    required: bool,
}

impl CapabilityRequirement {
    pub fn new(
        id: CapabilityId,
        minimum_version: u16,
        maximum_version: u16,
        required: bool,
    ) -> Result<Self, ContractValidationError> {
        if minimum_version == 0 || minimum_version > maximum_version {
            return Err(ContractValidationError::InvalidCapabilityVersion);
        }
        Ok(Self {
            id,
            minimum_version,
            maximum_version,
            required,
        })
    }

    #[must_use]
    pub fn id(&self) -> &CapabilityId {
        &self.id
    }

    #[must_use]
    pub const fn minimum_version(&self) -> u16 {
        self.minimum_version
    }

    #[must_use]
    pub const fn maximum_version(&self) -> u16 {
        self.maximum_version
    }

    #[must_use]
    pub const fn required(&self) -> bool {
        self.required
    }

    #[must_use]
    pub const fn accepts(&self, version: u16) -> bool {
        self.minimum_version <= version && version <= self.maximum_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_id_is_platform_neutral_and_version_is_separate() {
        let id = CapabilityId::new("external.account.balance.read").unwrap();
        let declaration = CapabilityDeclaration {
            id,
            version: 1,
            access: CapabilityAccess::Read,
            side_effect: SideEffectClass::ExternalRead,
            write_semantics: None,
        };

        let value = serde_json::to_value(declaration).unwrap();
        assert_eq!(value["id"], "external.account.balance.read");
        assert_eq!(value["version"], 1);
        assert!(CapabilityId::new("NewApi.balance.v1").is_err());
    }

    #[test]
    fn read_capability_cannot_declare_external_write() {
        let declaration = CapabilityDeclaration {
            id: CapabilityId::new("external.account.balance.read").unwrap(),
            version: 1,
            access: CapabilityAccess::Read,
            side_effect: SideEffectClass::ExternalWrite,
            write_semantics: None,
        };

        assert_eq!(
            declaration.validate(),
            Err(ContractValidationError::InvalidCapabilityVersion)
        );
    }

    #[test]
    fn external_write_requires_explicit_recovery_semantics() {
        let mut declaration = CapabilityDeclaration {
            id: CapabilityId::new("external.payment.refund").unwrap(),
            version: 1,
            access: CapabilityAccess::Write,
            side_effect: SideEffectClass::ExternalWrite,
            write_semantics: None,
        };
        assert_eq!(
            declaration.validate(),
            Err(ContractValidationError::InvalidCapabilityVersion)
        );

        declaration.write_semantics = Some(ExternalWriteSemantics {
            native_idempotency_key: false,
            result_lookup: ResultLookupSupport::ByRequestId,
            reconciliation: true,
        });
        assert_eq!(declaration.validate(), Ok(()));
        assert!(
            !declaration
                .write_semantics
                .unwrap()
                .retry_after_unknown_outcome_is_safe()
        );
    }
}
