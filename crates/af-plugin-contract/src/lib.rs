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
pub use schema::{CompiledJsonSchema, openapi_document, schema_document, validate_json_schema};
pub use value::{
    CapabilityInvocation, CapabilityResult, ConnectorHealth, ConnectorHealthStatus,
    ExternalAccountProfile, ExternalBalanceSnapshot, InvocationMetadata,
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
    #[error("JSON Schema is invalid or references an external resource")]
    InvalidJsonSchema,
    #[error("configuration does not match its JSON Schema")]
    InvalidConfiguration,
    #[error("external value is invalid")]
    InvalidExternalValue,
}
