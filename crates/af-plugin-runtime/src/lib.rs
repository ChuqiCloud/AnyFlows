//! Trusted, compile-time connector registry and typed capability façade.
//!
//! This crate is not a sandbox or dynamic plugin loader. Only reviewed
//! implementations shipped with the host may be registered here.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_httpclient::{HeaderMap, HeaderValue, Method, PooledClient, StatusCode};
use af_plugin_contract::{
    CapabilityAccess, CapabilityDeclaration, CapabilityId, CapabilityInvocation,
    CapabilityRequirement, CapabilityResult, ConnectorError, ConnectorErrorCode, ContractVersion,
    ExternalAccountProfile, ExternalBalanceSnapshot, InvocationMetadata, PluginId, PluginManifest,
    PluginVersion, SideEffectClass,
};
use async_trait::async_trait;
use serde_json::Value;
use tokio::{sync::Semaphore, time::timeout};
use zeroize::Zeroizing;

mod new_api;
mod schema;
mod sidecar;
mod sub2api;

use crate::schema::CompiledJsonSchema;
pub use crate::schema::validate_plugin_configuration;

pub use new_api::{NewApiConnector, new_api_registration};
pub use sidecar::{
    PluginArtifactDescriptor, SidecarError, SidecarLimits, SidecarSupervisor, TrustedPluginKey,
    VerifiedPluginArtifact, plugin_artifact_signature_payload, plugin_package_digest,
    validate_public_plugin_configuration,
};
pub use sub2api::{Sub2ApiConnector, sub2api_registration};

const ACCOUNT_PROFILE_READ: &str = "external.account.profile.read";
const ACCOUNT_BALANCE_READ: &str = "external.account.balance.read";

/// Reviewed connector implementation compiled into the AnyFlows host.
pub trait PlatformConnector: Send + Sync {
    fn manifest(&self) -> &PluginManifest;

    /// Validate public connection settings after the host validates the
    /// manifest's JSON Schema. Secret values are never included here.
    fn validate_config(&self, config: &Value) -> Result<(), ConnectorError>;
}

/// Context supplied by the host for one capability operation.
///
/// It contains only the connection's public settings and correlation metadata;
/// database handles, host wallet authority, and platform secrets are excluded.
#[derive(Clone)]
pub struct ConnectorInvocation {
    connection_id: String,
    metadata: InvocationMetadata,
    public_config: Value,
    base_url: String,
    max_response_bytes: u32,
    client: PooledClient,
}

/// Host-supplied context for a generic JSON capability. It has no URL, HTTP
/// client, or credential fields, so pure and local capabilities need no URL.
#[derive(Clone)]
pub struct CapabilityExecutionContext {
    connection_id: String,
    metadata: InvocationMetadata,
    public_config: Value,
    max_response_bytes: u32,
}

impl CapabilityExecutionContext {
    pub fn new(
        connection_id: String,
        metadata: InvocationMetadata,
        public_config: Value,
        max_response_bytes: u32,
    ) -> Result<Self, ConnectorOperationError> {
        if connection_id.is_empty()
            || connection_id.len() > 128
            || metadata.validate().is_err()
            || !public_config.is_object()
            || max_response_bytes == 0
            || max_response_bytes > 16 * 1024 * 1024
        {
            return Err(ConnectorOperationError::InvalidInvocation);
        }
        Ok(Self {
            connection_id,
            metadata,
            public_config,
            max_response_bytes,
        })
    }

    /// Return a copy with host-owned plugin configuration substituted for any
    /// caller-provided value. Lifecycle adapters use this to bind each call to
    /// the configuration persisted with the active deployment generation.
    pub fn with_public_config(
        &self,
        public_config: Value,
    ) -> Result<Self, ConnectorOperationError> {
        if !public_config.is_object() {
            return Err(ConnectorOperationError::InvalidInvocation);
        }
        let mut invocation = self.clone();
        invocation.public_config = public_config;
        Ok(invocation)
    }

    /// Remaining host-call budget, including deployment lookup or recovery.
    pub fn remaining_timeout(&self) -> Result<Duration, ConnectorOperationError> {
        operation_timeout_for_metadata(&self.metadata, 300_000)
    }
}

impl std::fmt::Debug for CapabilityExecutionContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CapabilityExecutionContext")
            .field("connection_id", &self.connection_id)
            .field("metadata", &self.metadata)
            .field("public_config", &"<redacted>")
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

/// Least-authority context exposed to generic capability handlers.
///
/// This type intentionally exposes no credential, HTTP client, database handle,
/// or host service. Generic handlers are trusted in-process transformations;
/// this is not yet a process-isolation boundary.
#[derive(Clone)]
pub struct CapabilityContext<'a> {
    connection_id: &'a str,
    metadata: &'a InvocationMetadata,
    public_config: &'a Value,
}

impl<'a> CapabilityContext<'a> {
    fn from_execution_context(invocation: &'a CapabilityExecutionContext) -> Self {
        Self {
            connection_id: &invocation.connection_id,
            metadata: &invocation.metadata,
            public_config: &invocation.public_config,
        }
    }

    #[must_use]
    pub fn connection_id(&self) -> &str {
        self.connection_id
    }

    #[must_use]
    pub fn metadata(&self) -> &InvocationMetadata {
        self.metadata
    }

    #[must_use]
    pub fn public_config(&self) -> &Value {
        self.public_config
    }
}

impl std::fmt::Debug for CapabilityContext<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CapabilityContext")
            .field("connection_id", &self.connection_id)
            .field("metadata", &self.metadata)
            .field("public_config", &"<redacted>")
            .finish()
    }
}

impl std::fmt::Debug for ConnectorInvocation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectorInvocation")
            .field("connection_id", &self.connection_id)
            .field("metadata", &self.metadata)
            .field("public_config", &"<redacted>")
            .field("base_url", &"<redacted>")
            .field("client", &"<managed>")
            .finish()
    }
}

impl ConnectorInvocation {
    pub fn new(
        connection_id: String,
        metadata: InvocationMetadata,
        public_config: Value,
        base_url: String,
        max_response_bytes: u32,
        client: PooledClient,
    ) -> Result<Self, ConnectorOperationError> {
        if connection_id.is_empty()
            || connection_id.len() > 128
            || metadata.validate().is_err()
            || !public_config.is_object()
            || public_config.get("baseUrl").and_then(Value::as_str) != Some(base_url.as_str())
            || !valid_public_base_url(&base_url)
            || max_response_bytes == 0
            || max_response_bytes > 16 * 1024 * 1024
        {
            return Err(ConnectorOperationError::InvalidInvocation);
        }
        Ok(Self {
            connection_id,
            metadata,
            public_config,
            base_url,
            max_response_bytes,
            client,
        })
    }

    #[must_use]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    #[must_use]
    pub const fn metadata(&self) -> &InvocationMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn public_config(&self) -> &Value {
        &self.public_config
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    #[must_use]
    pub const fn max_response_bytes(&self) -> u32 {
        self.max_response_bytes
    }

    #[must_use]
    pub fn client(&self) -> &PooledClient {
        &self.client
    }
}

/// Short-lived external account credential, excluded from `Debug` and wire DTOs.
pub struct ExternalPlatformCredential(Zeroizing<String>);

impl ExternalPlatformCredential {
    pub fn new(value: String) -> Result<Self, ConnectorOperationError> {
        let value = Zeroizing::new(value);
        if value.is_empty()
            || value.len() > 8192
            || value.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(ConnectorOperationError::InvalidInvocation);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn from_zeroizing(value: Zeroizing<String>) -> Self {
        Self(value)
    }

    fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Debug for ExternalPlatformCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ExternalPlatformCredential(<redacted>)")
    }
}

pub(crate) fn valid_public_base_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && value.len() <= 2048
    })
}

/// Typed reader for an external account profile capability.
#[async_trait]
pub trait ExternalAccountProfileReader: Send + Sync {
    async fn read_profile(
        &self,
        invocation: &ConnectorInvocation,
        external_subject: &str,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalAccountProfile, ConnectorError>;
}

/// Verifies a user-supplied platform credential and returns its stable subject.
#[async_trait]
pub trait ExternalAccountAuthenticator: Send + Sync {
    async fn authenticate(
        &self,
        invocation: &ConnectorInvocation,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalAccountProfile, ConnectorError>;
}

/// Typed reader for a point-in-time external balance capability.
#[async_trait]
pub trait ExternalBalanceReader: Send + Sync {
    async fn read_balance(
        &self,
        invocation: &ConnectorInvocation,
        external_subject: &str,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalBalanceSnapshot, ConnectorError>;
}

/// Handler for an explicitly declared, host-authorized read capability.
///
/// Generic handlers receive JSON data only: they cannot access host services,
/// credentials, or the HTTP client. Capabilities with external writes must use
/// a dedicated host-controlled workflow instead.
#[async_trait]
pub trait CapabilityHandler: Send + Sync {
    fn capability_id(&self) -> &CapabilityId;

    async fn invoke(
        &self,
        invocation: &CapabilityContext<'_>,
        input: Value,
    ) -> Result<Value, ConnectorError>;
}

/// Implementations supplied only for capabilities advertised by the connector.
pub struct ConnectorRegistration {
    connector: Arc<dyn PlatformConnector>,
    authenticator: Option<Arc<dyn ExternalAccountAuthenticator>>,
    account_profile_reader: Option<Arc<dyn ExternalAccountProfileReader>>,
    balance_reader: Option<Arc<dyn ExternalBalanceReader>>,
    capability_handlers: Vec<Arc<dyn CapabilityHandler>>,
}

impl ConnectorRegistration {
    #[must_use]
    pub fn new(connector: Arc<dyn PlatformConnector>) -> Self {
        Self {
            connector,
            authenticator: None,
            account_profile_reader: None,
            balance_reader: None,
            capability_handlers: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_authenticator(
        mut self,
        authenticator: Arc<dyn ExternalAccountAuthenticator>,
    ) -> Self {
        self.authenticator = Some(authenticator);
        self
    }

    #[must_use]
    pub fn with_account_profile_reader(
        mut self,
        reader: Arc<dyn ExternalAccountProfileReader>,
    ) -> Self {
        self.account_profile_reader = Some(reader);
        self
    }

    #[must_use]
    pub fn with_balance_reader(mut self, reader: Arc<dyn ExternalBalanceReader>) -> Self {
        self.balance_reader = Some(reader);
        self
    }

    #[must_use]
    pub fn with_capability_handler(mut self, handler: Arc<dyn CapabilityHandler>) -> Self {
        self.capability_handlers.push(handler);
        self
    }
}

struct RegisteredConnector {
    connector: Arc<dyn PlatformConnector>,
    config_validator: CompiledJsonSchema,
    capability_validators: BTreeMap<CapabilityId, (CompiledJsonSchema, CompiledJsonSchema)>,
    concurrency: Arc<Semaphore>,
    authenticator: Option<Arc<dyn ExternalAccountAuthenticator>>,
    account_profile_reader: Option<Arc<dyn ExternalAccountProfileReader>>,
    balance_reader: Option<Arc<dyn ExternalBalanceReader>>,
    capability_handlers: BTreeMap<CapabilityId, Arc<dyn CapabilityHandler>>,
    enabled: AtomicBool,
}

/// Startup registry for compile-time registered connectors.
pub struct ConnectorRegistry {
    connectors: BTreeMap<PluginId, RegisteredConnector>,
    identity: Arc<()>,
}

impl Default for ConnectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectorRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            connectors: BTreeMap::new(),
            identity: Arc::new(()),
        }
    }

    pub fn register(
        &mut self,
        registration: ConnectorRegistration,
    ) -> Result<(), ConnectorRegistryError> {
        let manifest = registration.connector.manifest();
        manifest
            .validate()
            .map_err(|_| ConnectorRegistryError::InvalidManifest)?;
        if self.connectors.contains_key(&manifest.plugin_id) {
            return Err(ConnectorRegistryError::DuplicatePlugin);
        }
        let config_validator = CompiledJsonSchema::compile(&manifest.config_schema)
            .map_err(|_| ConnectorRegistryError::InvalidManifest)?;

        let declares_profile = manifest
            .capabilities
            .iter()
            .any(|capability| capability.id.as_str() == ACCOUNT_PROFILE_READ);
        let declares_balance = manifest
            .capabilities
            .iter()
            .any(|capability| capability.id.as_str() == ACCOUNT_BALANCE_READ);
        if declares_profile
            != (registration.authenticator.is_some()
                && registration.account_profile_reader.is_some())
        {
            return Err(ConnectorRegistryError::CapabilityHandlerMismatch {
                capability: CapabilityId::new(ACCOUNT_PROFILE_READ)
                    .expect("static capability ID is valid"),
            });
        }
        if declares_balance != registration.balance_reader.is_some() {
            return Err(ConnectorRegistryError::CapabilityHandlerMismatch {
                capability: CapabilityId::new(ACCOUNT_BALANCE_READ)
                    .expect("static capability ID is valid"),
            });
        }

        let mut capability_handlers = BTreeMap::new();
        let mut capability_validators = BTreeMap::new();
        for handler in registration.capability_handlers {
            let capability_id = handler.capability_id().clone();
            if matches!(
                capability_id.as_str(),
                ACCOUNT_PROFILE_READ | ACCOUNT_BALANCE_READ
            ) || !manifest
                .capabilities
                .iter()
                .any(|capability| capability.id == capability_id)
            {
                return Err(ConnectorRegistryError::CapabilityHandlerMismatch {
                    capability: capability_id.clone(),
                });
            }
            let declaration = manifest
                .capabilities
                .iter()
                .find(|capability| capability.id == capability_id)
                .expect("manifest capability was checked above");
            if declaration.input_schema.is_none()
                || declaration.output_schema.is_none()
                || declaration.access != CapabilityAccess::Read
                || declaration.side_effect == SideEffectClass::ExternalWrite
            {
                return Err(ConnectorRegistryError::CapabilityHandlerMismatch {
                    capability: capability_id,
                });
            }
            let input_validator = CompiledJsonSchema::compile(
                declaration.input_schema.as_ref().expect("checked above"),
            )
            .map_err(|_| ConnectorRegistryError::InvalidManifest)?;
            let output_validator = CompiledJsonSchema::compile(
                declaration.output_schema.as_ref().expect("checked above"),
            )
            .map_err(|_| ConnectorRegistryError::InvalidManifest)?;
            capability_validators
                .insert(capability_id.clone(), (input_validator, output_validator));
            if capability_handlers
                .insert(capability_id.clone(), handler)
                .is_some()
            {
                return Err(ConnectorRegistryError::CapabilityHandlerMismatch {
                    capability: capability_id,
                });
            }
        }

        for declaration in &manifest.capabilities {
            let implemented = match declaration.id.as_str() {
                ACCOUNT_PROFILE_READ => {
                    registration.authenticator.is_some()
                        && registration.account_profile_reader.is_some()
                }
                ACCOUNT_BALANCE_READ => registration.balance_reader.is_some(),
                _ => capability_handlers.contains_key(&declaration.id),
            };
            if !implemented {
                return Err(ConnectorRegistryError::CapabilityHandlerMismatch {
                    capability: declaration.id.clone(),
                });
            }
        }

        let max_concurrency = manifest.limits.max_concurrency;
        self.connectors.insert(
            manifest.plugin_id.clone(),
            RegisteredConnector {
                connector: registration.connector,
                config_validator,
                capability_validators,
                concurrency: Arc::new(Semaphore::new(usize::from(max_concurrency))),
                authenticator: registration.authenticator,
                account_profile_reader: registration.account_profile_reader,
                balance_reader: registration.balance_reader,
                capability_handlers,
                enabled: AtomicBool::new(true),
            },
        );
        Ok(())
    }

    pub fn builtin() -> Result<Self, ConnectorRegistryError> {
        let mut registry = Self::new();
        registry.register(new_api_registration())?;
        registry.register(sub2api_registration())?;
        Ok(registry)
    }

    pub fn manifest(
        &self,
        plugin_id: &PluginId,
    ) -> Result<&PluginManifest, ConnectorRegistryError> {
        self.connectors
            .get(plugin_id)
            .map(|connector| connector.connector.manifest())
            .ok_or(ConnectorRegistryError::NotFound)
    }

    /// Change the activation state of a registered connector. This affects
    /// future operations immediately and does not mutate its immutable manifest.
    pub fn set_enabled(
        &self,
        plugin_id: &PluginId,
        enabled: bool,
    ) -> Result<(), ConnectorRegistryError> {
        let connector = self
            .connectors
            .get(plugin_id)
            .ok_or(ConnectorRegistryError::NotFound)?;
        connector.enabled.store(enabled, Ordering::Release);
        Ok(())
    }

    #[must_use]
    pub fn is_enabled(&self, plugin_id: &PluginId) -> bool {
        self.connectors
            .get(plugin_id)
            .is_some_and(|connector| connector.enabled.load(Ordering::Acquire))
    }

    #[must_use]
    pub fn manifests(&self) -> Vec<PluginManifest> {
        self.connectors
            .values()
            .map(|connector| connector.connector.manifest().clone())
            .collect()
    }

    /// Negotiate runtime compatibility for the capabilities selected by the
    /// trusted host. This does not authorize a user or connection by itself.
    pub fn negotiate(
        &self,
        plugin_id: &PluginId,
        host_contract: ContractVersion,
        requirements: &[CapabilityRequirement],
    ) -> Result<NegotiatedConnector, ConnectorRegistryError> {
        let registration = self
            .connectors
            .get(plugin_id)
            .ok_or(ConnectorRegistryError::NotFound)?;
        ensure_enabled(registration)?;
        let manifest = registration.connector.manifest();
        if !manifest.contract_range.contains(host_contract) {
            return Err(ConnectorRegistryError::IncompatibleContract);
        }

        let mut capabilities = Vec::with_capacity(requirements.len());
        let mut skipped_optional = Vec::new();
        let mut requested_ids = BTreeSet::new();
        for requirement in requirements {
            if !requested_ids.insert(requirement.id().clone()) {
                return Err(ConnectorRegistryError::DuplicateCapabilityRequirement {
                    capability: requirement.id().clone(),
                });
            }
            let declaration = manifest.capabilities.iter().find(|capability| {
                capability.id == *requirement.id() && requirement.accepts(capability.version)
            });
            let executable = capability_is_executable(registration, requirement.id().as_str());
            let Some(declaration) = declaration.filter(|_| executable) else {
                if requirement.required() {
                    return Err(ConnectorRegistryError::MissingCapability {
                        capability: requirement.id().clone(),
                    });
                }
                skipped_optional.push(requirement.id().clone());
                continue;
            };
            capabilities.push(declaration.clone());
        }

        Ok(NegotiatedConnector {
            registry_identity: Arc::clone(&self.identity),
            plugin_id: manifest.plugin_id.clone(),
            plugin_version: manifest.plugin_version.clone(),
            contract_version: host_contract,
            capabilities,
            skipped_optional,
        })
    }

    pub fn validate_config(
        &self,
        plugin_id: &PluginId,
        config: &Value,
    ) -> Result<(), ConnectorRegistryError> {
        let connector = self
            .connectors
            .get(plugin_id)
            .ok_or(ConnectorRegistryError::NotFound)?;
        connector
            .config_validator
            .validate(config)
            .map_err(|_| ConnectorRegistryError::InvalidConfiguration)?;
        connector
            .connector
            .validate_config(config)
            .map_err(ConnectorRegistryError::Configuration)
    }

    pub async fn read_external_account_profile(
        &self,
        negotiated: &NegotiatedConnector,
        invocation: &ConnectorInvocation,
        external_subject: &str,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalAccountProfile, ConnectorOperationError> {
        require_negotiated_capability(self, negotiated, ACCOUNT_PROFILE_READ)?;
        let connector = self
            .connectors
            .get(&negotiated.plugin_id)
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let _permit = connector
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| rate_limited_connector_error())?;
        ensure_enabled_for_operation(connector)?;
        validate_invocation(connector, invocation)?;
        let reader = connector
            .account_profile_reader
            .as_ref()
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let profile = timeout(
            operation_timeout(
                invocation,
                connector.connector.manifest().limits.request_timeout_ms,
            )?,
            reader.read_profile(invocation, external_subject, credential),
        )
        .await
        .map_err(|_| operation_timeout_error(false))?
        .map_err(ConnectorOperationError::Connector)?;
        profile
            .validate()
            .map_err(|_| ConnectorOperationError::InvalidResponse)?;
        Ok(profile)
    }

    pub async fn authenticate_external_account(
        &self,
        negotiated: &NegotiatedConnector,
        invocation: &ConnectorInvocation,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalAccountProfile, ConnectorOperationError> {
        require_negotiated_capability(self, negotiated, ACCOUNT_PROFILE_READ)?;
        let connector = self
            .connectors
            .get(&negotiated.plugin_id)
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let _permit = connector
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| rate_limited_connector_error())?;
        ensure_enabled_for_operation(connector)?;
        validate_invocation(connector, invocation)?;
        let authenticator = connector
            .authenticator
            .as_ref()
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let profile = timeout(
            operation_timeout(
                invocation,
                connector.connector.manifest().limits.request_timeout_ms,
            )?,
            authenticator.authenticate(invocation, credential),
        )
        .await
        .map_err(|_| operation_timeout_error(false))?
        .map_err(ConnectorOperationError::Connector)?;
        profile
            .validate()
            .map_err(|_| ConnectorOperationError::InvalidResponse)?;
        Ok(profile)
    }

    pub async fn read_external_balance(
        &self,
        negotiated: &NegotiatedConnector,
        invocation: &ConnectorInvocation,
        external_subject: &str,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalBalanceSnapshot, ConnectorOperationError> {
        require_negotiated_capability(self, negotiated, ACCOUNT_BALANCE_READ)?;
        let connector = self
            .connectors
            .get(&negotiated.plugin_id)
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let _permit = connector
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| rate_limited_connector_error())?;
        ensure_enabled_for_operation(connector)?;
        validate_invocation(connector, invocation)?;
        let reader = connector
            .balance_reader
            .as_ref()
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let snapshot = timeout(
            operation_timeout(
                invocation,
                connector.connector.manifest().limits.request_timeout_ms,
            )?,
            reader.read_balance(invocation, external_subject, credential),
        )
        .await
        .map_err(|_| operation_timeout_error(false))?
        .map_err(ConnectorOperationError::Connector)?;
        snapshot
            .validate()
            .map_err(|_| ConnectorOperationError::InvalidResponse)?;
        Ok(snapshot)
    }

    /// Invoke a negotiated generic capability after bounding input, execution,
    /// and output at the host boundary. Mutating capabilities are denied here.
    /// Callers must derive requirements from host authorization policy; a
    /// manifest declaration alone never grants access. Do not include secrets
    /// in the generic request payload.
    pub async fn invoke_capability(
        &self,
        negotiated: &NegotiatedConnector,
        invocation: &CapabilityExecutionContext,
        capability_id: &CapabilityId,
        request: CapabilityInvocation,
    ) -> Result<CapabilityResult, ConnectorOperationError> {
        require_negotiated_capability(self, negotiated, capability_id.as_str())?;
        request
            .validate()
            .map_err(|_| ConnectorOperationError::InvalidInvocation)?;
        let connector = self
            .connectors
            .get(&negotiated.plugin_id)
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let _permit = connector
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| rate_limited_connector_error())?;
        ensure_enabled_for_operation(connector)?;
        validate_capability_invocation(connector, invocation)?;
        let declaration = negotiated
            .capabilities
            .iter()
            .find(|capability| capability.id == *capability_id)
            .ok_or(ConnectorOperationError::CapabilityNotNegotiated)?;
        if declaration.access != CapabilityAccess::Read
            || declaration.side_effect == SideEffectClass::ExternalWrite
        {
            return Err(ConnectorOperationError::WriteCapabilityRequiresHostWorkflow);
        }
        if request.capability_version != declaration.version {
            return Err(ConnectorOperationError::CapabilityNotNegotiated);
        }
        let manifest = connector.connector.manifest();
        let payload_limit = manifest
            .limits
            .max_response_bytes
            .min(invocation.max_response_bytes);
        ensure_json_payload_within_limit(&request.input, payload_limit as usize)?;
        let (input_validator, output_validator) = connector
            .capability_validators
            .get(capability_id)
            .ok_or(ConnectorOperationError::CapabilitySchemaMissing)?;
        input_validator
            .validate(&request.input)
            .map_err(|_| ConnectorOperationError::InvalidInvocation)?;
        let handler = connector
            .capability_handlers
            .get(capability_id)
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let output = timeout(
            operation_timeout_for_metadata(
                &invocation.metadata,
                manifest.limits.request_timeout_ms,
            )?,
            handler.invoke(
                &CapabilityContext::from_execution_context(invocation),
                request.input,
            ),
        )
        .await
        .map_err(|_| operation_timeout_error(false))?
        .map_err(ConnectorOperationError::Connector)?;
        ensure_json_payload_within_limit(&output, payload_limit as usize)?;
        output_validator
            .validate(&output)
            .map_err(|_| ConnectorOperationError::InvalidResponse)?;
        Ok(CapabilityResult { output })
    }

    pub fn plugin_ids(&self) -> impl Iterator<Item = &PluginId> {
        self.connectors.keys()
    }
}

fn rate_limited_connector_error() -> ConnectorOperationError {
    ConnectorOperationError::Connector(ConnectorError::rate_limited(250))
}

struct PayloadSizeWriter {
    limit: usize,
    written: usize,
}

impl Write for PayloadSizeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.written) {
            return Err(io::Error::other("connector payload exceeds its limit"));
        }
        self.written += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn ensure_json_payload_within_limit(
    value: &Value,
    max_bytes: usize,
) -> Result<(), ConnectorOperationError> {
    let mut writer = PayloadSizeWriter {
        limit: max_bytes,
        written: 0,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| ConnectorOperationError::PayloadTooLarge)?;
    Ok(())
}

fn ensure_enabled(connector: &RegisteredConnector) -> Result<(), ConnectorRegistryError> {
    if connector.enabled.load(Ordering::Acquire) {
        Ok(())
    } else {
        Err(ConnectorRegistryError::Disabled)
    }
}

fn ensure_enabled_for_operation(
    connector: &RegisteredConnector,
) -> Result<(), ConnectorOperationError> {
    if connector.enabled.load(Ordering::Acquire) {
        Ok(())
    } else {
        Err(ConnectorOperationError::Disabled)
    }
}

fn require_negotiated_capability(
    registry: &ConnectorRegistry,
    negotiated: &NegotiatedConnector,
    capability: &str,
) -> Result<(), ConnectorOperationError> {
    let registered = registry
        .connectors
        .get(&negotiated.plugin_id)
        .ok_or(ConnectorOperationError::NotRegistered)?;
    ensure_enabled_for_operation(registered)?;
    let manifest = registered.connector.manifest();
    let binding_is_valid = Arc::ptr_eq(&registry.identity, &negotiated.registry_identity)
        && negotiated.plugin_id == manifest.plugin_id
        && negotiated.plugin_version == manifest.plugin_version
        && manifest
            .contract_range
            .contains(negotiated.contract_version)
        && negotiated.capabilities.iter().all(|selected| {
            manifest
                .capabilities
                .iter()
                .any(|declared| declared == selected)
                && capability_is_executable(registered, selected.id.as_str())
        })
        && negotiated
            .capabilities
            .iter()
            .enumerate()
            .all(|(index, selected)| {
                negotiated.capabilities[..index]
                    .iter()
                    .all(|previous| previous.id != selected.id)
            })
        && negotiated.skipped_optional.iter().all(|skipped| {
            !negotiated
                .capabilities
                .iter()
                .any(|selected| selected.id == *skipped)
        });
    if binding_is_valid
        && negotiated
            .capabilities
            .iter()
            .any(|declared| declared.id.as_str() == capability)
    {
        Ok(())
    } else {
        Err(ConnectorOperationError::CapabilityNotNegotiated)
    }
}

fn capability_is_executable(connector: &RegisteredConnector, capability: &str) -> bool {
    match capability {
        ACCOUNT_PROFILE_READ => {
            connector.authenticator.is_some() && connector.account_profile_reader.is_some()
        }
        ACCOUNT_BALANCE_READ => connector.balance_reader.is_some(),
        _ => CapabilityId::new(capability.to_owned())
            .is_ok_and(|id| connector.capability_handlers.contains_key(&id)),
    }
}

fn validate_invocation(
    connector: &RegisteredConnector,
    invocation: &ConnectorInvocation,
) -> Result<(), ConnectorOperationError> {
    if invocation.max_response_bytes > connector.connector.manifest().limits.max_response_bytes {
        return Err(ConnectorOperationError::InvalidInvocation);
    }
    connector
        .config_validator
        .validate(&invocation.public_config)
        .map_err(|_| ConnectorOperationError::InvalidInvocation)?;
    connector
        .connector
        .validate_config(&invocation.public_config)
        .map_err(ConnectorOperationError::Connector)?;
    operation_timeout(
        invocation,
        connector.connector.manifest().limits.request_timeout_ms,
    )?;
    Ok(())
}

fn validate_capability_invocation(
    connector: &RegisteredConnector,
    invocation: &CapabilityExecutionContext,
) -> Result<(), ConnectorOperationError> {
    if invocation.max_response_bytes > connector.connector.manifest().limits.max_response_bytes {
        return Err(ConnectorOperationError::InvalidInvocation);
    }
    connector
        .config_validator
        .validate(&invocation.public_config)
        .map_err(|_| ConnectorOperationError::InvalidInvocation)?;
    connector
        .connector
        .validate_config(&invocation.public_config)
        .map_err(ConnectorOperationError::Connector)?;
    operation_timeout_for_metadata(
        &invocation.metadata,
        connector.connector.manifest().limits.request_timeout_ms,
    )?;
    Ok(())
}

fn operation_timeout(
    invocation: &ConnectorInvocation,
    manifest_timeout_ms: u32,
) -> Result<Duration, ConnectorOperationError> {
    operation_timeout_for_metadata(invocation.metadata(), manifest_timeout_ms)
}

fn operation_timeout_for_metadata(
    metadata: &InvocationMetadata,
    manifest_timeout_ms: u32,
) -> Result<Duration, ConnectorOperationError> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .ok_or(ConnectorOperationError::InvalidInvocation)?;
    let remaining_ms = metadata
        .deadline_unix_ms
        .checked_sub(now_ms)
        .filter(|remaining| *remaining > 0)
        .ok_or_else(|| operation_timeout_error(false))?;
    Ok(Duration::from_millis(
        remaining_ms.min(u64::from(manifest_timeout_ms)),
    ))
}

fn operation_timeout_error(is_write: bool) -> ConnectorOperationError {
    ConnectorOperationError::Connector(ConnectorError::new(
        if is_write {
            ConnectorErrorCode::OutcomeUnknown
        } else {
            ConnectorErrorCode::TemporarilyUnavailable
        },
        if is_write {
            af_plugin_contract::OperationOutcome::Unknown
        } else {
            af_plugin_contract::OperationOutcome::NotApplied
        },
    ))
}

/// Result of negotiating the host and adapter contract versions/capabilities.
#[derive(Clone)]
pub struct NegotiatedConnector {
    registry_identity: Arc<()>,
    plugin_id: PluginId,
    plugin_version: PluginVersion,
    contract_version: ContractVersion,
    capabilities: Vec<CapabilityDeclaration>,
    skipped_optional: Vec<CapabilityId>,
}

impl NegotiatedConnector {
    #[must_use]
    pub fn plugin_id(&self) -> &PluginId {
        &self.plugin_id
    }

    #[must_use]
    pub fn plugin_version(&self) -> &PluginVersion {
        &self.plugin_version
    }

    #[must_use]
    pub const fn contract_version(&self) -> ContractVersion {
        self.contract_version
    }

    #[must_use]
    pub fn capabilities(&self) -> &[CapabilityDeclaration] {
        &self.capabilities
    }

    #[must_use]
    pub fn skipped_optional(&self) -> &[CapabilityId] {
        &self.skipped_optional
    }
}

impl PartialEq for NegotiatedConnector {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.registry_identity, &other.registry_identity)
            && self.plugin_id == other.plugin_id
            && self.plugin_version == other.plugin_version
            && self.contract_version == other.contract_version
            && self.capabilities == other.capabilities
            && self.skipped_optional == other.skipped_optional
    }
}

impl Eq for NegotiatedConnector {}

impl std::fmt::Debug for NegotiatedConnector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NegotiatedConnector")
            .field("plugin_id", &self.plugin_id)
            .field("plugin_version", &self.plugin_version)
            .field("contract_version", &self.contract_version)
            .field("capabilities", &self.capabilities)
            .field("skipped_optional", &self.skipped_optional)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ConnectorRegistryError {
    #[error("connector manifest is invalid")]
    InvalidManifest,
    #[error("connector ID is already registered")]
    DuplicatePlugin,
    #[error("connector is not registered")]
    NotFound,
    #[error("connector is disabled")]
    Disabled,
    #[error("connector contract version is incompatible")]
    IncompatibleContract,
    #[error("required connector capability is unavailable")]
    MissingCapability { capability: CapabilityId },
    #[error("connector capability declaration and implementation do not match")]
    CapabilityHandlerMismatch { capability: CapabilityId },
    #[error("connector configuration is invalid")]
    Configuration(ConnectorError),
    #[error("connector configuration does not match its declared schema")]
    InvalidConfiguration,
    #[error("the same capability was requested more than once")]
    DuplicateCapabilityRequirement { capability: CapabilityId },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ConnectorOperationError {
    #[error("requested connector capability was not negotiated")]
    CapabilityNotNegotiated,
    #[error("connector capability implementation is unavailable")]
    NotRegistered,
    #[error("connector is disabled")]
    Disabled,
    #[error("connector invocation context is invalid")]
    InvalidInvocation,
    #[error("connector response does not match the contract")]
    InvalidResponse,
    #[error("generic capability invocation cannot perform writes")]
    WriteCapabilityRequiresHostWorkflow,
    #[error("connector payload exceeds the manifest limit")]
    PayloadTooLarge,
    #[error("generic capability is missing an input or output schema")]
    CapabilitySchemaMissing,
    #[error("connector operation failed")]
    Connector(ConnectorError),
}

fn bearer_headers(credential: &ExternalPlatformCredential) -> Result<HeaderMap, ConnectorError> {
    let mut headers = HeaderMap::new();
    let encoded = Zeroizing::new(format!("Bearer {}", credential.expose()));
    let value = HeaderValue::from_str(encoded.as_str()).map_err(|_| {
        ConnectorError::new(
            ConnectorErrorCode::AuthenticationRequired,
            af_plugin_contract::OperationOutcome::NotApplied,
        )
    })?;
    headers.insert(http::header::AUTHORIZATION, value);
    Ok(headers)
}

async fn get_json(
    invocation: &ConnectorInvocation,
    path: &str,
    credential: &ExternalPlatformCredential,
) -> Result<Value, ConnectorError> {
    let base = invocation.base_url().trim_end_matches('/');
    let root = base
        .strip_suffix("/v1")
        .or_else(|| base.strip_suffix("/v1beta"))
        .unwrap_or(base);
    let target = format!("{root}{path}");
    let response = invocation
        .client()
        .execute(Method::GET, &target, bearer_headers(credential)?, None)
        .await
        .map_err(|_| {
            ConnectorError::new(
                ConnectorErrorCode::TemporarilyUnavailable,
                af_plugin_contract::OperationOutcome::NotApplied,
            )
        })?;
    match response.status() {
        StatusCode::OK => {}
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            return Err(ConnectorError::new(
                ConnectorErrorCode::AuthenticationRequired,
                af_plugin_contract::OperationOutcome::NotApplied,
            ));
        }
        StatusCode::TOO_MANY_REQUESTS => {
            return Err(ConnectorError::rate_limited(1_000));
        }
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_IMPLEMENTED => {
            return Err(ConnectorError::new(
                ConnectorErrorCode::Unsupported,
                af_plugin_contract::OperationOutcome::NotApplied,
            ));
        }
        _ => {
            return Err(ConnectorError::new(
                ConnectorErrorCode::TemporarilyUnavailable,
                af_plugin_contract::OperationOutcome::NotApplied,
            ));
        }
    }
    let max_response_bytes = usize::try_from(invocation.max_response_bytes())
        .map_err(|_| invalid_connector_response())?;
    if response
        .content_length()
        .is_some_and(|length| length > u64::from(invocation.max_response_bytes()))
    {
        return Err(invalid_connector_response());
    }
    let mut bytes = Vec::new();
    let mut stream = response.into_bytes_stream();
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.map_err(|_| {
            ConnectorError::new(
                ConnectorErrorCode::TemporarilyUnavailable,
                af_plugin_contract::OperationOutcome::NotApplied,
            )
        })?;
        if chunk.len() > max_response_bytes.saturating_sub(bytes.len()) {
            return Err(invalid_connector_response());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid_connector_response())?;
    if value.get("success") == Some(&Value::Bool(false)) || value.get("error").is_some() {
        return Err(ConnectorError::new(
            ConnectorErrorCode::InvalidResponse,
            af_plugin_contract::OperationOutcome::NotApplied,
        ));
    }
    Ok(value)
}

fn invalid_connector_response() -> ConnectorError {
    ConnectorError::new(
        ConnectorErrorCode::InvalidResponse,
        af_plugin_contract::OperationOutcome::NotApplied,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_plugin_contract::{
        CapabilityAccess, CapabilityDeclaration, CapabilityId, ConnectorErrorCode, ConnectorLimits,
        ContractVersionRange, ExternalAccountProfile, ExternalBalanceSnapshot, InvocationMetadata,
        OperationOutcome, SideEffectClass,
    };

    struct MockConnector {
        manifest: PluginManifest,
        invalid_balance: bool,
    }

    impl PlatformConnector for MockConnector {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        fn validate_config(&self, config: &Value) -> Result<(), ConnectorError> {
            if config.get("baseUrl").and_then(Value::as_str).is_some() {
                Ok(())
            } else {
                Err(ConnectorError::new(
                    ConnectorErrorCode::InvalidConfiguration,
                    OperationOutcome::NotApplied,
                ))
            }
        }
    }

    struct GenericConnector {
        manifest: PluginManifest,
    }

    impl PlatformConnector for GenericConnector {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        fn validate_config(&self, _config: &Value) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    struct EchoCapability {
        id: CapabilityId,
        invalid_output: bool,
        delay: Duration,
    }

    #[async_trait]
    impl CapabilityHandler for EchoCapability {
        fn capability_id(&self) -> &CapabilityId {
            &self.id
        }

        async fn invoke(
            &self,
            invocation: &CapabilityContext<'_>,
            input: Value,
        ) -> Result<Value, ConnectorError> {
            tokio::time::sleep(self.delay).await;
            assert_eq!(invocation.connection_id(), "connection-1");
            assert_eq!(invocation.public_config()["scope"], "catalog");
            if self.invalid_output {
                Ok(serde_json::json!({ "unexpected": "value" }))
            } else {
                Ok(serde_json::json!({ "items": [input["query"].clone()] }))
            }
        }
    }

    fn generic_registration(plugin_id: &str, invalid_output: bool) -> ConnectorRegistration {
        generic_registration_with_limits(plugin_id, invalid_output, 1_000, Duration::ZERO)
    }

    fn generic_registration_with_limits(
        plugin_id: &str,
        invalid_output: bool,
        request_timeout_ms: u32,
        delay: Duration,
    ) -> ConnectorRegistration {
        let capability_id = CapabilityId::new("catalog.models.search").unwrap();
        let manifest = PluginManifest {
            plugin_id: PluginId::new(plugin_id).unwrap(),
            display_name: "Generic catalog connector".to_owned(),
            plugin_version: PluginVersion::new("1.0.0").unwrap(),
            contract_range: ContractVersionRange::new(
                ContractVersion::new(1, 0),
                ContractVersion::new(1, 2),
            )
            .unwrap(),
            capabilities: vec![CapabilityDeclaration {
                id: capability_id.clone(),
                version: 2,
                access: CapabilityAccess::Read,
                side_effect: SideEffectClass::ExternalRead,
                write_semantics: None,
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "query": { "type": "string", "minLength": 1, "maxLength": 64 } },
                    "required": ["query"],
                    "additionalProperties": false
                })),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "items": { "type": "array", "items": { "type": "string" } } },
                    "required": ["items"],
                    "additionalProperties": false
                })),
            }],
            authentication_modes: vec!["none".to_owned()],
            limits: ConnectorLimits {
                max_concurrency: 2,
                request_timeout_ms,
                max_response_bytes: 1_048_576,
                max_page_size: 50,
            },
            config_schema: serde_json::json!({
                "type": "object",
                "properties": { "scope": { "type": "string" } },
                "required": ["scope"],
                "additionalProperties": false
            }),
        };
        ConnectorRegistration::new(Arc::new(GenericConnector { manifest })).with_capability_handler(
            Arc::new(EchoCapability {
                id: capability_id,
                invalid_output,
                delay,
            }),
        )
    }

    #[async_trait]
    impl ExternalAccountAuthenticator for MockConnector {
        async fn authenticate(
            &self,
            _invocation: &ConnectorInvocation,
            _credential: &ExternalPlatformCredential,
        ) -> Result<ExternalAccountProfile, ConnectorError> {
            Ok(ExternalAccountProfile {
                subject: "mock-subject".to_owned(),
                display_name: Some("Mock external account".to_owned()),
            })
        }
    }

    #[async_trait]
    impl ExternalAccountProfileReader for MockConnector {
        async fn read_profile(
            &self,
            _invocation: &ConnectorInvocation,
            external_subject: &str,
            _credential: &ExternalPlatformCredential,
        ) -> Result<ExternalAccountProfile, ConnectorError> {
            Ok(ExternalAccountProfile {
                subject: external_subject.to_owned(),
                display_name: Some("Mock external account".to_owned()),
            })
        }
    }

    #[async_trait]
    impl ExternalBalanceReader for MockConnector {
        async fn read_balance(
            &self,
            _invocation: &ConnectorInvocation,
            _external_subject: &str,
            _credential: &ExternalPlatformCredential,
        ) -> Result<ExternalBalanceSnapshot, ConnectorError> {
            Ok(ExternalBalanceSnapshot {
                amount: if self.invalid_balance {
                    "-1".to_owned()
                } else {
                    "12.3400".to_owned()
                },
                unit: "quota_unit".to_owned(),
                observed_at_unix_ms: 1_800_000_000_000,
            })
        }
    }

    fn mock_registration(
        plugin_id: &str,
        include_profile_capability: bool,
        invalid_balance: bool,
    ) -> ConnectorRegistration {
        let mut capabilities = vec![CapabilityDeclaration {
            id: CapabilityId::new(ACCOUNT_BALANCE_READ).unwrap(),
            version: 1,
            access: CapabilityAccess::Read,
            side_effect: SideEffectClass::ExternalRead,
            write_semantics: None,
            input_schema: None,
            output_schema: None,
        }];
        if include_profile_capability {
            capabilities.push(CapabilityDeclaration {
                id: CapabilityId::new(ACCOUNT_PROFILE_READ).unwrap(),
                version: 1,
                access: CapabilityAccess::Read,
                side_effect: SideEffectClass::ExternalRead,
                write_semantics: None,
                input_schema: None,
                output_schema: None,
            });
        }
        let connector = Arc::new(MockConnector {
            manifest: PluginManifest {
                plugin_id: PluginId::new(plugin_id).unwrap(),
                display_name: plugin_id.to_owned(),
                plugin_version: PluginVersion::new("1.0.0").unwrap(),
                contract_range: ContractVersionRange::new(
                    ContractVersion::new(1, 0),
                    ContractVersion::new(1, 2),
                )
                .unwrap(),
                capabilities,
                authentication_modes: vec!["api_key".to_owned()],
                limits: ConnectorLimits {
                    max_concurrency: 8,
                    request_timeout_ms: 10_000,
                    max_response_bytes: 1_048_576,
                    max_page_size: 100,
                },
                config_schema: serde_json::json!({
                    "type": "object",
                    "properties": { "baseUrl": { "type": "string" } },
                    "required": ["baseUrl"],
                    "additionalProperties": false
                }),
            },
            invalid_balance,
        });
        let mut registration =
            ConnectorRegistration::new(connector.clone()).with_balance_reader(connector.clone());
        if include_profile_capability {
            registration = registration
                .with_authenticator(connector.clone())
                .with_account_profile_reader(connector);
        }
        registration
    }

    fn call_context() -> ConnectorInvocation {
        let now_ms = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        let deadline_unix_ms = now_ms + 60_000;
        ConnectorInvocation::new(
            "connection-1".to_owned(),
            InvocationMetadata {
                request_id: "request-1".to_owned(),
                deadline_unix_ms,
            },
            serde_json::json!({ "baseUrl": "https://example.test" }),
            "https://example.test".to_owned(),
            1_048_576,
            af_httpclient::HttpClientProvider::new(af_httpclient::HttpClientConfig::default(), 2)
                .unwrap()
                .get(None)
                .unwrap(),
        )
        .unwrap()
    }

    fn capability_call_context() -> CapabilityExecutionContext {
        let now_ms = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        CapabilityExecutionContext::new(
            "connection-1".to_owned(),
            InvocationMetadata {
                request_id: "request-1".to_owned(),
                deadline_unix_ms: now_ms + 60_000,
            },
            serde_json::json!({ "scope": "catalog" }),
            1_048_576,
        )
        .unwrap()
    }

    fn required(id: &str) -> CapabilityRequirement {
        required_version(id, 1)
    }

    fn required_version(id: &str, version: u16) -> CapabilityRequirement {
        CapabilityRequirement::new(CapabilityId::new(id).unwrap(), version, version, true).unwrap()
    }

    fn capability_request(input: Value) -> CapabilityInvocation {
        CapabilityInvocation {
            capability_version: 2,
            input,
        }
    }

    #[tokio::test]
    async fn mock_connectors_read_typed_profile_and_native_balance() {
        let mut registry = ConnectorRegistry::new();
        registry
            .register(mock_registration("org.example.first", true, false))
            .unwrap();
        registry
            .register(mock_registration("net.example.second", false, false))
            .unwrap();

        let first_id = PluginId::new("org.example.first").unwrap();
        let first = registry
            .negotiate(
                &first_id,
                ContractVersion::new(1, 1),
                &[
                    required(ACCOUNT_PROFILE_READ),
                    required(ACCOUNT_BALANCE_READ),
                ],
            )
            .unwrap();
        let call = call_context();
        let profile = registry
            .read_external_account_profile(
                &first,
                &call,
                "opaque-subject-1",
                &ExternalPlatformCredential::new("mock-token".to_owned()).unwrap(),
            )
            .await
            .unwrap();
        let balance = registry
            .read_external_balance(
                &first,
                &call,
                "opaque-subject-1",
                &ExternalPlatformCredential::new("mock-token".to_owned()).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(profile.subject, "opaque-subject-1");
        assert_eq!(balance.amount, "12.3400");
        assert_eq!(balance.unit, "quota_unit");

        let second = registry
            .negotiate(
                &PluginId::new("net.example.second").unwrap(),
                ContractVersion::new(1, 1),
                &[CapabilityRequirement::new(
                    CapabilityId::new(ACCOUNT_PROFILE_READ).unwrap(),
                    1,
                    1,
                    false,
                )
                .unwrap()],
            )
            .unwrap();
        assert!(second.capabilities.is_empty());
        assert_eq!(second.skipped_optional.len(), 1);
        assert_eq!(registry.plugin_ids().count(), 2);
    }

    #[tokio::test]
    async fn unavailable_required_capability_and_contract_fail_closed() {
        let mut registry = ConnectorRegistry::new();
        registry
            .register(mock_registration("org.example.first", false, false))
            .unwrap();
        let plugin_id = PluginId::new("org.example.first").unwrap();

        assert!(matches!(
            registry.negotiate(
                &plugin_id,
                ContractVersion::new(1, 0),
                &[required(ACCOUNT_PROFILE_READ)]
            ),
            Err(ConnectorRegistryError::MissingCapability { .. })
        ));
        assert_eq!(
            registry.negotiate(&plugin_id, ContractVersion::new(2, 0), &[]),
            Err(ConnectorRegistryError::IncompatibleContract)
        );
    }

    #[tokio::test]
    async fn generic_capability_validates_declared_schemas_and_returns_typed_json() {
        let mut registry = ConnectorRegistry::new();
        registry
            .register(generic_registration("org.example.generic", false))
            .unwrap();
        let plugin_id = PluginId::new("org.example.generic").unwrap();
        let capability_id = CapabilityId::new("catalog.models.search").unwrap();
        let negotiated = registry
            .negotiate(
                &plugin_id,
                ContractVersion::new(1, 1),
                &[required_version("catalog.models.search", 2)],
            )
            .unwrap();
        let invocation = capability_call_context();

        assert_eq!(
            registry
                .invoke_capability(
                    &negotiated,
                    &invocation,
                    &capability_id,
                    capability_request(serde_json::json!({ "query": "deepseek" })),
                )
                .await
                .unwrap()
                .output,
            serde_json::json!({ "items": ["deepseek"] })
        );
        let wrong_version = CapabilityInvocation {
            capability_version: 1,
            input: serde_json::json!({ "query": "deepseek" }),
        };
        assert_eq!(
            registry
                .invoke_capability(&negotiated, &invocation, &capability_id, wrong_version)
                .await,
            Err(ConnectorOperationError::CapabilityNotNegotiated)
        );
        assert_eq!(
            registry
                .invoke_capability(
                    &negotiated,
                    &invocation,
                    &capability_id,
                    capability_request(serde_json::json!({ "unexpected": "private-canary" })),
                )
                .await,
            Err(ConnectorOperationError::InvalidInvocation)
        );
    }

    #[tokio::test]
    async fn generic_capability_rejects_invalid_output_and_foreign_negotiation() {
        let mut registry = ConnectorRegistry::new();
        registry
            .register(generic_registration("org.example.generic", true))
            .unwrap();
        let plugin_id = PluginId::new("org.example.generic").unwrap();
        let capability_id = CapabilityId::new("catalog.models.search").unwrap();
        let negotiated = registry
            .negotiate(
                &plugin_id,
                ContractVersion::new(1, 1),
                &[required_version("catalog.models.search", 2)],
            )
            .unwrap();
        let invocation = capability_call_context();
        assert_eq!(
            registry
                .invoke_capability(
                    &negotiated,
                    &invocation,
                    &capability_id,
                    capability_request(serde_json::json!({ "query": "deepseek" })),
                )
                .await,
            Err(ConnectorOperationError::InvalidResponse)
        );

        let mut other_registry = ConnectorRegistry::new();
        other_registry
            .register(generic_registration("org.example.generic", false))
            .unwrap();
        assert_eq!(
            other_registry
                .invoke_capability(
                    &negotiated,
                    &invocation,
                    &capability_id,
                    capability_request(serde_json::json!({ "query": "deepseek" })),
                )
                .await,
            Err(ConnectorOperationError::CapabilityNotNegotiated)
        );
    }

    #[tokio::test]
    async fn manifest_deadline_and_activation_state_are_enforced() {
        let mut registry = ConnectorRegistry::new();
        registry
            .register(generic_registration_with_limits(
                "org.example.deadline",
                false,
                5,
                Duration::from_millis(100),
            ))
            .unwrap();
        let plugin_id = PluginId::new("org.example.deadline").unwrap();
        let capability_id = CapabilityId::new("catalog.models.search").unwrap();
        let negotiated = registry
            .negotiate(
                &plugin_id,
                ContractVersion::new(1, 1),
                &[required_version("catalog.models.search", 2)],
            )
            .unwrap();
        let invocation = capability_call_context();
        assert!(matches!(
            registry
                .invoke_capability(
                    &negotiated,
                    &invocation,
                    &capability_id,
                    capability_request(serde_json::json!({ "query": "slow" })),
                )
                .await,
            Err(ConnectorOperationError::Connector(ConnectorError {
                code: ConnectorErrorCode::TemporarilyUnavailable,
                ..
            }))
        ));

        registry.set_enabled(&plugin_id, false).unwrap();
        assert!(!registry.is_enabled(&plugin_id));
        assert_eq!(
            registry.negotiate(&plugin_id, ContractVersion::new(1, 1), &[]),
            Err(ConnectorRegistryError::Disabled)
        );
        assert_eq!(
            registry
                .invoke_capability(
                    &negotiated,
                    &invocation,
                    &capability_id,
                    capability_request(serde_json::json!({ "query": "disabled" })),
                )
                .await,
            Err(ConnectorOperationError::Disabled)
        );
        registry.set_enabled(&plugin_id, true).unwrap();
        assert!(registry.is_enabled(&plugin_id));
    }

    #[test]
    fn registry_rejects_duplicate_ids_and_manifest_handler_mismatches() {
        let mut registry = ConnectorRegistry::new();
        registry
            .register(mock_registration("org.example.first", false, false))
            .unwrap();
        assert_eq!(
            registry.register(mock_registration("org.example.first", false, false)),
            Err(ConnectorRegistryError::DuplicatePlugin)
        );

        let missing_handler = ConnectorRegistration::new(Arc::new(MockConnector {
            manifest: PluginManifest {
                plugin_id: PluginId::new("net.example.mismatch").unwrap(),
                display_name: "Mismatch mock".to_owned(),
                plugin_version: PluginVersion::new("1.0.0").unwrap(),
                contract_range: ContractVersionRange::new(
                    ContractVersion::new(1, 0),
                    ContractVersion::new(1, 0),
                )
                .unwrap(),
                capabilities: vec![CapabilityDeclaration {
                    id: CapabilityId::new(ACCOUNT_BALANCE_READ).unwrap(),
                    version: 1,
                    access: CapabilityAccess::Read,
                    side_effect: SideEffectClass::ExternalRead,
                    write_semantics: None,
                    input_schema: None,
                    output_schema: None,
                }],
                authentication_modes: vec!["api_key".to_owned()],
                limits: ConnectorLimits {
                    max_concurrency: 8,
                    request_timeout_ms: 10_000,
                    max_response_bytes: 1_048_576,
                    max_page_size: 100,
                },
                config_schema: serde_json::json!({ "type": "object" }),
            },
            invalid_balance: false,
        }));
        assert!(matches!(
            registry.register(missing_handler),
            Err(ConnectorRegistryError::CapabilityHandlerMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn invalid_external_values_are_rejected_at_the_host_boundary() {
        let mut registry = ConnectorRegistry::new();
        registry
            .register(mock_registration("org.example.invalid", false, true))
            .unwrap();
        let negotiated = registry
            .negotiate(
                &PluginId::new("org.example.invalid").unwrap(),
                ContractVersion::new(1, 0),
                &[required(ACCOUNT_BALANCE_READ)],
            )
            .unwrap();
        assert_eq!(
            registry
                .read_external_balance(
                    &negotiated,
                    &call_context(),
                    "subject",
                    &ExternalPlatformCredential::new("mock-token".to_owned()).unwrap(),
                )
                .await,
            Err(ConnectorOperationError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn connector_concurrency_limit_rejects_excess_operations() {
        let mut registry = ConnectorRegistry::new();
        registry
            .register(mock_registration("org.example.limited", false, false))
            .unwrap();
        let plugin_id = PluginId::new("org.example.limited").unwrap();
        let negotiated = registry
            .negotiate(
                &plugin_id,
                ContractVersion::new(1, 0),
                &[required(ACCOUNT_BALANCE_READ)],
            )
            .unwrap();
        let registration = registry.connectors.get(&plugin_id).unwrap();
        let _permits = (0..8)
            .map(|_| {
                registration
                    .concurrency
                    .clone()
                    .try_acquire_owned()
                    .unwrap()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            registry
                .read_external_balance(
                    &negotiated,
                    &call_context(),
                    "subject",
                    &ExternalPlatformCredential::new("mock-token".to_owned()).unwrap(),
                )
                .await,
            Err(ConnectorOperationError::Connector(
                ConnectorError::rate_limited(250)
            ))
        );
    }

    #[tokio::test]
    async fn connector_configuration_stays_separate_from_credentials() {
        let mut registry = ConnectorRegistry::new();
        registry
            .register(mock_registration("org.example.first", false, false))
            .unwrap();
        let plugin_id = PluginId::new("org.example.first").unwrap();
        assert!(
            registry
                .validate_config(
                    &plugin_id,
                    &serde_json::json!({ "baseUrl": "https://example.test" })
                )
                .is_ok()
        );
        assert!(
            registry
                .validate_config(
                    &plugin_id,
                    &serde_json::json!({ "password": "not-in-config" })
                )
                .is_err()
        );
    }
}
