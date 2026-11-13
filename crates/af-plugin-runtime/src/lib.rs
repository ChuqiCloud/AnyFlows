//! Trusted, compile-time connector registry and typed capability façade.
//!
//! This crate is not a sandbox or dynamic plugin loader. Only reviewed
//! implementations shipped with the host may be registered here.

use std::{collections::BTreeMap, sync::Arc};

use af_httpclient::{HeaderMap, HeaderValue, Method, PooledClient, StatusCode};
use af_plugin_contract::{
    CapabilityDeclaration, CapabilityId, CapabilityRequirement, ConnectorError, ConnectorErrorCode,
    ContractVersion, ExternalAccountProfile, ExternalBalanceSnapshot, InvocationMetadata, PluginId,
    PluginManifest, PluginVersion,
};
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

mod new_api;
mod sub2api;

pub use new_api::{NewApiConnector, new_api_registration};
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

/// Implementations supplied only for capabilities advertised by the connector.
pub struct ConnectorRegistration {
    connector: Arc<dyn PlatformConnector>,
    authenticator: Option<Arc<dyn ExternalAccountAuthenticator>>,
    account_profile_reader: Option<Arc<dyn ExternalAccountProfileReader>>,
    balance_reader: Option<Arc<dyn ExternalBalanceReader>>,
}

impl ConnectorRegistration {
    #[must_use]
    pub fn new(connector: Arc<dyn PlatformConnector>) -> Self {
        Self {
            connector,
            authenticator: None,
            account_profile_reader: None,
            balance_reader: None,
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
}

struct RegisteredConnector {
    connector: Arc<dyn PlatformConnector>,
    concurrency: Arc<Semaphore>,
    authenticator: Option<Arc<dyn ExternalAccountAuthenticator>>,
    account_profile_reader: Option<Arc<dyn ExternalAccountProfileReader>>,
    balance_reader: Option<Arc<dyn ExternalBalanceReader>>,
}

/// Startup registry for compile-time registered connectors.
#[derive(Default)]
pub struct ConnectorRegistry {
    connectors: BTreeMap<PluginId, RegisteredConnector>,
}

impl ConnectorRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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

        let max_concurrency = manifest.limits.max_concurrency;
        self.connectors.insert(
            manifest.plugin_id.clone(),
            RegisteredConnector {
                connector: registration.connector,
                concurrency: Arc::new(Semaphore::new(usize::from(max_concurrency))),
                authenticator: registration.authenticator,
                account_profile_reader: registration.account_profile_reader,
                balance_reader: registration.balance_reader,
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

    #[must_use]
    pub fn manifests(&self) -> Vec<PluginManifest> {
        self.connectors
            .values()
            .map(|connector| connector.connector.manifest().clone())
            .collect()
    }

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
        let manifest = registration.connector.manifest();
        if !manifest.contract_range.contains(host_contract) {
            return Err(ConnectorRegistryError::IncompatibleContract);
        }

        let mut capabilities = Vec::with_capacity(requirements.len());
        let mut skipped_optional = Vec::new();
        for requirement in requirements {
            let declaration = manifest.capabilities.iter().find(|capability| {
                capability.id == *requirement.id() && requirement.accepts(capability.version)
            });
            let executable = match requirement.id().as_str() {
                ACCOUNT_PROFILE_READ => registration.account_profile_reader.is_some(),
                ACCOUNT_BALANCE_READ => registration.balance_reader.is_some(),
                _ => false,
            };
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
        self.connectors
            .get(plugin_id)
            .ok_or(ConnectorRegistryError::NotFound)?
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
        require_negotiated_capability(negotiated, ACCOUNT_PROFILE_READ)?;
        let connector = self
            .connectors
            .get(&negotiated.plugin_id)
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let _permit = connector
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| rate_limited_connector_error())?;
        let reader = connector
            .account_profile_reader
            .as_ref()
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let profile = reader
            .read_profile(invocation, external_subject, credential)
            .await
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
        require_negotiated_capability(negotiated, ACCOUNT_PROFILE_READ)?;
        let connector = self
            .connectors
            .get(&negotiated.plugin_id)
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let _permit = connector
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| rate_limited_connector_error())?;
        let authenticator = connector
            .authenticator
            .as_ref()
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let profile = authenticator
            .authenticate(invocation, credential)
            .await
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
        require_negotiated_capability(negotiated, ACCOUNT_BALANCE_READ)?;
        let connector = self
            .connectors
            .get(&negotiated.plugin_id)
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let _permit = connector
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| rate_limited_connector_error())?;
        let reader = connector
            .balance_reader
            .as_ref()
            .ok_or(ConnectorOperationError::NotRegistered)?;
        let snapshot = reader
            .read_balance(invocation, external_subject, credential)
            .await
            .map_err(ConnectorOperationError::Connector)?;
        snapshot
            .validate()
            .map_err(|_| ConnectorOperationError::InvalidResponse)?;
        Ok(snapshot)
    }

    pub fn plugin_ids(&self) -> impl Iterator<Item = &PluginId> {
        self.connectors.keys()
    }
}

fn rate_limited_connector_error() -> ConnectorOperationError {
    ConnectorOperationError::Connector(ConnectorError::rate_limited(250))
}

fn require_negotiated_capability(
    negotiated: &NegotiatedConnector,
    capability: &str,
) -> Result<(), ConnectorOperationError> {
    if negotiated
        .capabilities
        .iter()
        .any(|declared| declared.id.as_str() == capability)
    {
        Ok(())
    } else {
        Err(ConnectorOperationError::CapabilityNotNegotiated)
    }
}

/// Result of negotiating the host and adapter contract versions/capabilities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NegotiatedConnector {
    pub plugin_id: PluginId,
    pub plugin_version: PluginVersion,
    pub contract_version: ContractVersion,
    pub capabilities: Vec<CapabilityDeclaration>,
    pub skipped_optional: Vec<CapabilityId>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ConnectorRegistryError {
    #[error("connector manifest is invalid")]
    InvalidManifest,
    #[error("connector ID is already registered")]
    DuplicatePlugin,
    #[error("connector is not registered")]
    NotFound,
    #[error("connector contract version is incompatible")]
    IncompatibleContract,
    #[error("required connector capability is unavailable")]
    MissingCapability { capability: CapabilityId },
    #[error("connector capability declaration and implementation do not match")]
    CapabilityHandlerMismatch { capability: CapabilityId },
    #[error("connector configuration is invalid")]
    Configuration(ConnectorError),
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ConnectorOperationError {
    #[error("requested connector capability was not negotiated")]
    CapabilityNotNegotiated,
    #[error("connector capability implementation is unavailable")]
    NotRegistered,
    #[error("connector invocation context is invalid")]
    InvalidInvocation,
    #[error("connector response does not match the contract")]
    InvalidResponse,
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
        }];
        if include_profile_capability {
            capabilities.push(CapabilityDeclaration {
                id: CapabilityId::new(ACCOUNT_PROFILE_READ).unwrap(),
                version: 1,
                access: CapabilityAccess::Read,
                side_effect: SideEffectClass::ExternalRead,
                write_semantics: None,
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
        ConnectorInvocation::new(
            "connection-1".to_owned(),
            InvocationMetadata {
                request_id: "request-1".to_owned(),
                deadline_unix_ms: 1_800_000_001_000,
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

    fn required(id: &str) -> CapabilityRequirement {
        CapabilityRequirement::new(CapabilityId::new(id).unwrap(), 1, 1, true).unwrap()
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
