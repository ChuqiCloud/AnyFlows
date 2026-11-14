//! Platform-neutral plugin manifest and wire-contract types.
//!
//! This crate deliberately has no dependency on AnyFlows domain, persistence,
//! networking, or application services so other languages can implement the
//! same versioned contract.

mod capability;
mod error;
mod manifest;
mod schema;
mod value;
mod version;

pub use capability::{
    CapabilityAccess, CapabilityDeclaration, CapabilityId, CapabilityRequirement,
    ExternalWriteSemantics, ResultLookupSupport, SideEffectClass,
};
pub use error::{ConnectorError, ConnectorErrorCode, OperationOutcome};
pub use manifest::{ConnectorLimits, PluginId, PluginManifest};
pub use schema::{openapi_document, schema_document};
pub use value::{
    CapabilityInvocation, CapabilityResult, ConnectorHealth, ConnectorHealthStatus,
    ExternalAccountProfile, ExternalBalanceSnapshot, InvocationMetadata, PluginHostOperation,
    PluginHostRequest, PluginHostResponse, PluginHostResult,
};
pub use version::{ContractVersion, ContractVersionRange, PluginVersion};

/// Errors produced while validating wire-contract values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ContractValidationError {
    #[error("identifier is invalid")]
    InvalidIdentifier,
    #[error("plugin version is invalid")]
    InvalidPluginVersion,
    #[error("contract version range is invalid")]
    InvalidContractRange,
    #[error("capability version is invalid")]
    InvalidCapabilityVersion,
    #[error("manifest is invalid")]
    InvalidManifest,
    #[error("manifest contains duplicate capabilities")]
    DuplicateCapability,
    #[error("JSON Schema has an invalid shape")]
    InvalidJsonSchema,
    #[error("configuration does not match its JSON Schema")]
    InvalidConfiguration,
    #[error("external value is invalid")]
    InvalidExternalValue,
}
