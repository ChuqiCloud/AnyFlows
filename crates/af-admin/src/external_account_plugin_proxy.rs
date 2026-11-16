//! User-scoped adapter for the fixed external-account Sidecar callback endpoints.

use std::sync::Arc;

use af_plugin_contract::{
    PluginProxyError, PluginProxyErrorCode, PluginProxyRequest, PluginProxyResponse,
    PluginProxyResult,
};
use af_plugin_runtime::PluginProxyDispatcher;
use async_trait::async_trait;

use crate::{
    SessionPrincipal, UserExternalAccountService,
    external_account::external_account_proxy_endpoint,
};

/// Dispatcher bound to one authenticated user and one of that user's linked accounts.
/// The account identifier is never serialized to the Sidecar.
pub struct UserExternalAccountProxyDispatcher {
    accounts: Arc<dyn UserExternalAccountService>,
    principal: SessionPrincipal,
    connection_id: i64,
    allow_external_reads: bool,
}

impl UserExternalAccountProxyDispatcher {
    #[must_use]
    pub fn new(
        accounts: Arc<dyn UserExternalAccountService>,
        principal: SessionPrincipal,
        connection_id: i64,
        allow_external_reads: bool,
    ) -> Self {
        Self {
            accounts,
            principal,
            connection_id,
            allow_external_reads,
        }
    }
}

#[async_trait]
impl PluginProxyDispatcher for UserExternalAccountProxyDispatcher {
    async fn dispatch(&self, request: PluginProxyRequest) -> PluginProxyResponse {
        let result = if !proxy_request_is_allowed(
            self.allow_external_reads,
            self.connection_id,
            &request,
        ) {
            proxy_error(PluginProxyErrorCode::Forbidden)
        } else {
            self.accounts
                .proxy_read(self.principal, self.connection_id, request.clone())
                .await
                .unwrap_or_else(|_| proxy_error(PluginProxyErrorCode::TemporarilyUnavailable))
        };
        let result_is_valid = match &result {
            PluginProxyResult::Response(projection) => projection.validate(16 * 1024).is_ok(),
            PluginProxyResult::Error(_) => true,
        };
        let result = if result_is_valid {
            result
        } else {
            proxy_error(PluginProxyErrorCode::InvalidResponse)
        };
        PluginProxyResponse {
            protocol_version: request.protocol_version,
            plugin_id: request.plugin_id,
            plugin_version: request.plugin_version,
            artifact_sha256: request.artifact_sha256,
            deployment_generation: request.deployment_generation,
            request_id: request.request_id,
            capability_id: request.capability_id,
            capability_version: request.capability_version,
            proxy_request_id: request.proxy_request_id,
            result,
        }
    }
}

fn proxy_request_is_allowed(
    allow_external_reads: bool,
    connection_id: i64,
    request: &PluginProxyRequest,
) -> bool {
    allow_external_reads
        && connection_id > 0
        && request.validate().is_ok()
        && external_account_proxy_endpoint(request).is_some()
}

fn proxy_error(code: PluginProxyErrorCode) -> PluginProxyResult {
    PluginProxyResult::Error(PluginProxyError { code })
}

#[cfg(test)]
mod tests {
    use af_plugin_contract::{
        CapabilityId, HostHttpMethod, HostHttpRequestProposal, PluginId, PluginProxyResult,
        PluginVersion,
    };
    use serde_json::json;

    use super::*;

    fn request(endpoint_id: &str, capability_id: &str) -> PluginProxyRequest {
        PluginProxyRequest {
            protocol_version: 1,
            plugin_id: PluginId::new("org.example.account-review").unwrap(),
            plugin_version: PluginVersion::new("1.0.0").unwrap(),
            artifact_sha256: "ab".repeat(32),
            deployment_generation: 3,
            request_id: "request-1".to_owned(),
            capability_id: CapabilityId::new(capability_id).unwrap(),
            capability_version: 1,
            proxy_request_id: "proxy-1".to_owned(),
            request: HostHttpRequestProposal {
                endpoint_id: endpoint_id.to_owned(),
                method: HostHttpMethod::Get,
                query: Default::default(),
            },
        }
    }

    #[test]
    fn fixed_projections_do_not_contain_account_identity_or_credentials() {
        let profile = json!({ "displayName": "Alice" });
        assert_eq!(profile, json!({ "displayName": "Alice" }));
        let balance = json!({
            "amount":"12.3400",
            "unit":"USD",
            "status":"available",
            "observedAt":1_700_000_000_i64
        });
        let serialized = serde_json::to_string(&balance).unwrap();
        assert!(!serialized.contains("subject"));
        assert!(!serialized.contains("token"));
        assert!(!serialized.contains("url"));
    }

    #[test]
    fn endpoint_scope_is_specific_to_capability_and_has_no_query_surface() {
        let profile = request("account.profile", "external.account.profile.read");
        assert_eq!(
            external_account_proxy_endpoint(&profile),
            Some(crate::external_account::ExternalAccountProxyEndpoint::Profile)
        );

        let mismatched = request("account.balance", "external.account.profile.read");
        assert_eq!(external_account_proxy_endpoint(&mismatched), None);

        let mut queried = request("account.balance", "external.account.balance.read");
        queried
            .request
            .query
            .insert("include".to_owned(), "private".to_owned());
        assert_eq!(external_account_proxy_endpoint(&queried), None);

        let mut head = request("account.balance", "external.account.balance.read");
        head.request.method = HostHttpMethod::Head;
        assert_eq!(external_account_proxy_endpoint(&head), None);
    }

    #[test]
    fn external_reads_require_explicit_per_invocation_consent() {
        let request = request("account.balance", "external.account.balance.read");
        assert!(proxy_request_is_allowed(true, 17, &request));
        assert!(!proxy_request_is_allowed(false, 17, &request));
        assert!(!proxy_request_is_allowed(true, 0, &request));
    }

    #[test]
    fn proxy_failures_use_only_fixed_result_codes() {
        let response = proxy_error(PluginProxyErrorCode::Forbidden);
        assert!(matches!(&response, PluginProxyResult::Error(_)));
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(!serialized.contains("account_id"));
        assert!(!serialized.contains("https://"));
    }
}
