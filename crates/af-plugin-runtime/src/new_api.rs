use std::sync::Arc;

use af_plugin_contract::{
    CapabilityAccess, CapabilityDeclaration, CapabilityId, ConnectorError, ConnectorErrorCode,
    ConnectorLimits, ContractVersion, ContractVersionRange, ExternalAccountProfile,
    ExternalBalanceSnapshot, PluginId, PluginManifest, PluginVersion, SideEffectClass,
};
use async_trait::async_trait;
use serde_json::{Number, Value};

use crate::{
    ACCOUNT_BALANCE_READ, ACCOUNT_PROFILE_READ, ConnectorInvocation, ConnectorRegistration,
    ExternalAccountAuthenticator, ExternalAccountProfileReader, ExternalBalanceReader,
    ExternalPlatformCredential, PlatformConnector, get_json, invalid_connector_response,
    valid_public_base_url,
};

/// NewAPI account connector. It accepts account-scoped PATs with `profile:read`;
/// model API keys are not treated as login or account-binding credentials.
pub struct NewApiConnector {
    manifest: PluginManifest,
}

impl Default for NewApiConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl NewApiConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: manifest(),
        }
    }
}

pub fn new_api_registration() -> ConnectorRegistration {
    let connector = Arc::new(NewApiConnector::new());
    ConnectorRegistration::new(connector.clone())
        .with_authenticator(connector.clone())
        .with_account_profile_reader(connector.clone())
        .with_balance_reader(connector)
}

impl PlatformConnector for NewApiConnector {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn validate_config(&self, config: &Value) -> Result<(), ConnectorError> {
        let valid = config
            .get("baseUrl")
            .and_then(Value::as_str)
            .is_some_and(valid_public_base_url);
        if valid {
            Ok(())
        } else {
            Err(ConnectorError::new(
                ConnectorErrorCode::InvalidConfiguration,
                af_plugin_contract::OperationOutcome::NotApplied,
            ))
        }
    }
}

#[async_trait]
impl ExternalAccountAuthenticator for NewApiConnector {
    async fn authenticate(
        &self,
        invocation: &ConnectorInvocation,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalAccountProfile, ConnectorError> {
        let response = get_json(invocation, "/api/user/self", credential).await?;
        profile(&response)
    }
}

#[async_trait]
impl ExternalAccountProfileReader for NewApiConnector {
    async fn read_profile(
        &self,
        invocation: &ConnectorInvocation,
        external_subject: &str,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalAccountProfile, ConnectorError> {
        let response = get_json(invocation, "/api/user/self", credential).await?;
        let profile = profile(&response)?;
        if profile.subject != external_subject {
            return Err(ConnectorError::new(
                ConnectorErrorCode::Conflict,
                af_plugin_contract::OperationOutcome::NotApplied,
            ));
        }
        Ok(profile)
    }
}

#[async_trait]
impl ExternalBalanceReader for NewApiConnector {
    async fn read_balance(
        &self,
        invocation: &ConnectorInvocation,
        external_subject: &str,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalBalanceSnapshot, ConnectorError> {
        let response = get_json(invocation, "/api/user/self", credential).await?;
        let data = response.get("data").unwrap_or(&response);
        let subject = subject(data.get("id")).ok_or_else(invalid_connector_response)?;
        if subject != external_subject {
            return Err(ConnectorError::new(
                ConnectorErrorCode::Conflict,
                af_plugin_contract::OperationOutcome::NotApplied,
            ));
        }
        let amount = decimal_string(data.get("quota")).ok_or_else(invalid_connector_response)?;
        let snapshot = ExternalBalanceSnapshot {
            amount,
            unit: "quota".to_owned(),
            observed_at_unix_ms: unix_millis(),
        };
        snapshot
            .validate()
            .map_err(|_| invalid_connector_response())?;
        Ok(snapshot)
    }
}

fn profile(value: &Value) -> Result<ExternalAccountProfile, ConnectorError> {
    let data = value.get("data").unwrap_or(value);
    let subject = subject(data.get("id")).ok_or_else(invalid_connector_response)?;
    let display_name = data
        .get("display_name")
        .or_else(|| data.get("username"))
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty() && name.len() <= 256)
        .map(str::to_owned);
    Ok(ExternalAccountProfile {
        subject,
        display_name,
    })
}

fn subject(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) if !value.is_empty() && value.len() <= 512 => Some(value.clone()),
        Value::Number(value) if !value.is_f64() => Some(value.to_string()),
        _ => None,
    }
}

fn decimal_string(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::String(value) if value.len() <= 128 => Some(value.clone()),
        Value::Number(value) => Some(number_string(value)),
        _ => None,
    }
}

fn number_string(value: &Number) -> String {
    value.to_string()
}

fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_id: PluginId::new("org.anyflows.newapi").expect("static plugin ID is valid"),
        display_name: "NewAPI".to_owned(),
        plugin_version: PluginVersion::new("0.1.0").expect("static plugin version is valid"),
        contract_range: ContractVersionRange::new(
            ContractVersion::new(1, 0),
            ContractVersion::new(1, 2),
        )
        .expect("static contract range is valid"),
        capabilities: vec![
            capability(ACCOUNT_PROFILE_READ),
            capability(ACCOUNT_BALANCE_READ),
        ],
        authentication_modes: vec!["bearer_token".to_owned()],
        limits: ConnectorLimits {
            max_concurrency: 4,
            request_timeout_ms: 10_000,
            max_response_bytes: 1_048_576,
            max_page_size: 1,
        },
        config_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["baseUrl"],
            "properties": { "baseUrl": { "type": "string", "format": "uri", "maxLength": 2048 } }
        }),
    }
}

fn capability(id: &str) -> CapabilityDeclaration {
    CapabilityDeclaration {
        id: CapabilityId::new(id).expect("static capability ID is valid"),
        version: 1,
        access: CapabilityAccess::Read,
        side_effect: SideEffectClass::ExternalRead,
        write_semantics: None,
        input_schema: None,
        output_schema: None,
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newapi_registration_declares_only_read_capabilities_and_pat_auth() {
        let registration = new_api_registration();
        let manifest = registration.connector.manifest();
        assert_eq!(manifest.plugin_id.as_str(), "org.anyflows.newapi");
        assert_eq!(manifest.authentication_modes, ["bearer_token"]);
        assert!(manifest.capabilities.iter().all(|capability| {
            capability.access == CapabilityAccess::Read
                && capability.side_effect == SideEffectClass::ExternalRead
        }));
    }

    #[test]
    fn account_profile_does_not_include_email_or_api_credentials() {
        let parsed = profile(&serde_json::json!({
            "success": true,
            "data": { "id": 21, "username": "alice", "email": "private@example.test", "access_token": "secret" }
        }))
        .unwrap();
        assert_eq!(parsed.subject, "21");
        assert_eq!(parsed.display_name.as_deref(), Some("alice"));
        let json = serde_json::to_string(&parsed).unwrap();
        assert!(!json.contains("email"));
        assert!(!json.contains("secret"));
    }
}
