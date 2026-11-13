use std::sync::Arc;

use af_plugin_contract::{
    CapabilityAccess, CapabilityDeclaration, CapabilityId, ConnectorError, ConnectorErrorCode,
    ConnectorLimits, ContractVersion, ContractVersionRange, ExternalAccountProfile,
    ExternalBalanceSnapshot, PluginId, PluginManifest, PluginVersion, SideEffectClass,
};
use async_trait::async_trait;
use serde_json::Value;

use crate::{
    ACCOUNT_BALANCE_READ, ACCOUNT_PROFILE_READ, ConnectorInvocation, ConnectorRegistration,
    ExternalAccountAuthenticator, ExternalAccountProfileReader, ExternalBalanceReader,
    ExternalPlatformCredential, PlatformConnector, get_json, invalid_connector_response,
    valid_public_base_url,
};

/// Sub2API account connector. It accepts an existing authorized bearer token;
/// it never collects or forwards the user's account password.
pub struct Sub2ApiConnector {
    manifest: PluginManifest,
}

impl Default for Sub2ApiConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl Sub2ApiConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: manifest(),
        }
    }
}

pub fn sub2api_registration() -> ConnectorRegistration {
    let connector = Arc::new(Sub2ApiConnector::new());
    ConnectorRegistration::new(connector.clone())
        .with_authenticator(connector.clone())
        .with_account_profile_reader(connector.clone())
        .with_balance_reader(connector)
}

impl PlatformConnector for Sub2ApiConnector {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn validate_config(&self, config: &Value) -> Result<(), ConnectorError> {
        if config
            .get("baseUrl")
            .and_then(Value::as_str)
            .is_some_and(valid_public_base_url)
        {
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
impl ExternalAccountAuthenticator for Sub2ApiConnector {
    async fn authenticate(
        &self,
        invocation: &ConnectorInvocation,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalAccountProfile, ConnectorError> {
        let response = get_json(invocation, "/api/v1/auth/me", credential).await?;
        profile(&response)
    }
}

#[async_trait]
impl ExternalAccountProfileReader for Sub2ApiConnector {
    async fn read_profile(
        &self,
        invocation: &ConnectorInvocation,
        external_subject: &str,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalAccountProfile, ConnectorError> {
        let response = get_json(invocation, "/api/v1/auth/me", credential).await?;
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
impl ExternalBalanceReader for Sub2ApiConnector {
    async fn read_balance(
        &self,
        invocation: &ConnectorInvocation,
        external_subject: &str,
        credential: &ExternalPlatformCredential,
    ) -> Result<ExternalBalanceSnapshot, ConnectorError> {
        let response = get_json(invocation, "/api/v1/auth/me", credential).await?;
        let data = response.get("data").unwrap_or(&response);
        let subject = subject(data.get("id")).ok_or_else(invalid_connector_response)?;
        if subject != external_subject {
            return Err(ConnectorError::new(
                ConnectorErrorCode::Conflict,
                af_plugin_contract::OperationOutcome::NotApplied,
            ));
        }
        let amount = decimal_string(data.get("balance")).ok_or_else(invalid_connector_response)?;
        let snapshot = ExternalBalanceSnapshot {
            amount,
            unit: "USD".to_owned(),
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
        .get("username")
        .or_else(|| data.get("name"))
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
    match value? {
        Value::String(value) if value.len() <= 128 => Some(value.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_id: PluginId::new("org.anyflows.sub2api").expect("static plugin ID is valid"),
        display_name: "Sub2API".to_owned(),
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
    fn sub2api_registration_declares_read_only_account_capabilities() {
        let registration = sub2api_registration();
        let manifest = registration.connector.manifest();
        assert_eq!(manifest.plugin_id.as_str(), "org.anyflows.sub2api");
        assert_eq!(manifest.authentication_modes, ["bearer_token"]);
        assert!(manifest.capabilities.iter().all(|capability| {
            capability.access == CapabilityAccess::Read
                && capability.side_effect == SideEffectClass::ExternalRead
        }));
    }

    #[test]
    fn sub2api_balance_uses_native_currency_without_user_metadata() {
        let profile = profile(&serde_json::json!({
            "data": { "id": "subject-7", "username": "alice", "email": "private@example.test", "balance": "12.3400", "auth_token": "secret" }
        }))
        .unwrap();
        assert_eq!(profile.subject, "subject-7");
        assert_eq!(profile.display_name.as_deref(), Some("alice"));
        let balance = ExternalBalanceSnapshot {
            amount: decimal_string(Some(&serde_json::json!("12.3400"))).unwrap(),
            unit: "USD".to_owned(),
            observed_at_unix_ms: unix_millis(),
        };
        assert_eq!(balance.amount, "12.3400");
        assert_eq!(balance.unit, "USD");
    }
}
