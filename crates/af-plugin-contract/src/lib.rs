//! Platform-neutral plugin manifest and wire-contract types.
//!
//! This crate deliberately has no dependency on AnyFlows domain, persistence,
//! networking, or application services so other languages can implement the
//! same versioned contract.

#![recursion_limit = "256"]

mod capability;
mod error;
mod event;
mod manifest;
mod proposal;
mod request_processing;
mod schema;
mod task;
mod value;
mod version;

pub use capability::{
    CapabilityAccess, CapabilityDeclaration, CapabilityId, CapabilityRequirement,
    ExternalWriteSemantics, ResultLookupSupport, SideEffectClass,
};
pub use error::{ConnectorError, ConnectorErrorCode, OperationOutcome};
pub use event::{
    PluginEventDefinition, PluginEventFilterField, PluginEventSubscription, plugin_event_catalog,
    plugin_event_definition,
};
pub use manifest::{
    ConnectorLimits, PluginId, PluginManifest, PluginUiBlock, PluginUiComponent,
    PluginUiContribution, PluginUiSlot, PluginUiText, PluginUiTone, PluginUiView, PluginUiViewSlot,
};
pub use proposal::HostActionProposal;
pub use request_processing::{
    HeaderPatchEffect, JsonPatchEffect, JsonPatchOperation, JsonPatchOperationKind,
    REQUEST_PROCESSING_CAPABILITY, RequestProcessingAction, RequestProcessingEffect,
    RequestProcessingFinding, RequestProcessingFindingStatus, RequestProcessingInput,
    RequestProcessingOutput, RequestProcessingPipelineResult, RequestProcessingStage,
};
pub use schema::{openapi_document, schema_document};
pub use task::{
    MAX_PLUGIN_IDEMPOTENCY_KEY_BYTES, MAX_PLUGIN_TASK_ID_BYTES, PluginTaskOperation,
    PluginTaskResult, PluginTaskState, TaskContractError,
};
pub use value::{
    CapabilityInvocation, CapabilityResult, ConnectorHealth, ConnectorHealthStatus,
    ExternalAccountProfile, ExternalBalanceSnapshot, HostHttpMethod, HostHttpRequestProposal,
    HostHttpResponseProjection, InvocationMetadata, PROVIDER_REQUEST_TRANSFORM_CAPABILITY,
    PluginHostOperation, PluginHostRequest, PluginHostResponse, PluginHostResult, PluginProxyError,
    PluginProxyErrorCode, PluginProxyRequest, PluginProxyResponse, PluginProxyResult,
    ProviderHeaderPatch, ProviderRequestTransformInput, ProviderRequestTransformOutput,
    ProviderTransformAction, ProviderTransformRejection,
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
