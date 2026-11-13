use serde::{Deserialize, Serialize};

use crate::ContractValidationError;

/// Contract protocol version negotiated independently from adapter version.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractVersion {
    pub major: u16,
    pub minor: u16,
}

impl ContractVersion {
    #[must_use]
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

/// Inclusive compatible range declared by an adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractVersionRange {
    pub minimum: ContractVersion,
    pub maximum: ContractVersion,
}

impl ContractVersionRange {
    pub fn new(
        minimum: ContractVersion,
        maximum: ContractVersion,
    ) -> Result<Self, ContractValidationError> {
        let range = Self { minimum, maximum };
        range.validate()?;
        Ok(range)
    }

    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.minimum.major == 0
            || self.minimum.major != self.maximum.major
            || self.minimum > self.maximum
        {
            return Err(ContractValidationError::InvalidContractRange);
        }
        Ok(())
    }

    #[must_use]
    pub fn contains(&self, version: ContractVersion) -> bool {
        self.minimum <= version && version <= self.maximum
    }
}

/// Strict SemVer 2.0 string used for adapter releases.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PluginVersion(String);

impl PluginVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, ContractValidationError> {
        let value = value.into();
        if !is_valid_semver(&value) {
            return Err(ContractValidationError::InvalidPluginVersion);
        }
        Ok(Self(value))
    }

    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if is_valid_semver(&self.0) {
            Ok(())
        } else {
            Err(ContractValidationError::InvalidPluginVersion)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PluginVersion {
    type Error = ContractValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PluginVersion> for String {
    fn from(value: PluginVersion) -> Self {
        value.0
    }
}

fn is_valid_semver(value: &str) -> bool {
    if value.is_empty() || value.len() > 128 || !value.is_ascii() {
        return false;
    }
    let (without_build, build) = match value.split_once('+') {
        Some((left, right)) if !right.is_empty() && !right.contains('+') => (left, Some(right)),
        Some(_) => return false,
        None => (value, None),
    };
    let (core, prerelease) = match without_build.split_once('-') {
        Some((left, right)) if !right.is_empty() => (left, Some(right)),
        Some(_) => return false,
        None => (without_build, None),
    };
    let numbers = core.split('.').collect::<Vec<_>>();
    if numbers.len() != 3
        || numbers.iter().any(|number| {
            number.is_empty()
                || !number.bytes().all(|byte| byte.is_ascii_digit())
                || (number.len() > 1 && number.starts_with('0'))
        })
    {
        return false;
    }
    prerelease.is_none_or(|value| valid_identifiers(value, true))
        && build.is_none_or(|value| valid_identifiers(value, false))
}

fn valid_identifiers(value: &str, forbid_numeric_leading_zero: bool) -> bool {
    value.split('.').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !(forbid_numeric_leading_zero
                && part.len() > 1
                && part.starts_with('0')
                && part.bytes().all(|byte| byte.is_ascii_digit()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_serializes_semver() {
        for value in ["1.0.0", "1.2.3-beta.1", "0.1.0+build.7"] {
            let version = PluginVersion::new(value).unwrap();
            assert_eq!(
                serde_json::to_string(&version).unwrap(),
                format!("\"{value}\"")
            );
            assert_eq!(
                serde_json::from_str::<PluginVersion>(&format!("\"{value}\"")).unwrap(),
                version
            );
        }
        for value in ["", "01.0.0", "1.0", "1.0.0-", "1.0.0+", "1.0.0+one+two"] {
            assert!(PluginVersion::new(value).is_err(), "{value}");
        }
    }

    #[test]
    fn contract_range_is_inclusive_and_single_major() {
        let range =
            ContractVersionRange::new(ContractVersion::new(1, 0), ContractVersion::new(1, 4))
                .unwrap();
        assert!(range.contains(ContractVersion::new(1, 0)));
        assert!(range.contains(ContractVersion::new(1, 4)));
        assert!(!range.contains(ContractVersion::new(2, 0)));
        assert!(
            ContractVersionRange::new(ContractVersion::new(1, 4), ContractVersion::new(1, 0))
                .is_err()
        );
    }
}
