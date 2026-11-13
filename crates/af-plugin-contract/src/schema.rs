use serde_json::{Value, json};

/// Generated JSON Schema document for the language-neutral plugin v1 contract.
#[must_use]
pub fn schema_document() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "urn:anyflows:plugin-contract:v1",
        "title": "AnyFlows Plugin Contract v1",
        "oneOf": [
            { "$ref": "#/$defs/PluginManifest" },
            { "$ref": "#/$defs/ExternalAccountProfile" },
            { "$ref": "#/$defs/ExternalBalanceSnapshot" },
            { "$ref": "#/$defs/ConnectorHealth" },
            { "$ref": "#/$defs/ConnectorError" },
            { "$ref": "#/$defs/CapabilityInvocation" },
            { "$ref": "#/$defs/CapabilityResult" }
        ],
        "$defs": {
            "ContractVersion": {
                "type": "object",
                "additionalProperties": true,
                "required": ["major", "minor"],
                "properties": {
                    "major": { "type": "integer", "minimum": 1, "maximum": 65535 },
                    "minor": { "type": "integer", "minimum": 0, "maximum": 65535 }
                }
            },
            "ContractVersionRange": {
                "type": "object",
                "additionalProperties": true,
                "required": ["minimum", "maximum"],
                "properties": {
                    "minimum": { "$ref": "#/$defs/ContractVersion" },
                    "maximum": { "$ref": "#/$defs/ContractVersion" }
                }
            },
            "CapabilityDeclaration": {
                "type": "object",
                "additionalProperties": true,
                "required": ["id", "version", "access", "sideEffect"],
                "properties": {
                    "id": { "type": "string", "pattern": "^[a-z](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z](?:[a-z0-9-]*[a-z0-9])?)*$", "maxLength": 128 },
                    "version": { "type": "integer", "minimum": 1, "maximum": 65535 },
                    "access": { "enum": ["read", "write"] },
                    "sideEffect": { "enum": ["none", "external_read", "external_write"] },
                    "writeSemantics": { "$ref": "#/$defs/ExternalWriteSemantics" },
                    "inputSchema": { "type": "object" },
                    "outputSchema": { "type": "object" }
                },
                "dependentRequired": {
                    "inputSchema": ["outputSchema"],
                    "outputSchema": ["inputSchema"]
                },
                "allOf": [
                    {
                        "if": { "properties": { "sideEffect": { "const": "external_write" } }, "required": ["sideEffect"] },
                        "then": { "required": ["writeSemantics"], "properties": { "access": { "const": "write" } } }
                    },
                    {
                        "if": { "properties": { "sideEffect": { "enum": ["none", "external_read"] } }, "required": ["sideEffect"] },
                        "then": { "not": { "required": ["writeSemantics"] } }
                    }
                ]
            },
            "ExternalWriteSemantics": {
                "type": "object",
                "additionalProperties": true,
                "required": ["nativeIdempotencyKey", "resultLookup", "reconciliation"],
                "properties": {
                    "nativeIdempotencyKey": { "type": "boolean" },
                    "resultLookup": { "enum": ["none", "by_request_id", "by_external_reference"] },
                    "reconciliation": { "type": "boolean" }
                }
            },
            "PluginManifest": {
                "type": "object",
                "additionalProperties": true,
                "required": ["pluginId", "pluginVersion", "contractRange", "capabilities", "authenticationModes", "limits", "configSchema"],
                "properties": {
                    "pluginId": { "type": "string", "pattern": "^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+$", "maxLength": 128 },
                    "displayName": { "type": "string", "maxLength": 128 },
                    "pluginVersion": { "type": "string", "pattern": "^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)(-(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\\.(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*)?(\\+[0-9A-Za-z-]+(\\.[0-9A-Za-z-]+)*)?$", "maxLength": 128 },
                    "contractRange": { "$ref": "#/$defs/ContractVersionRange" },
                    "capabilities": { "type": "array", "minItems": 1, "maxItems": 128, "items": { "$ref": "#/$defs/CapabilityDeclaration" } },
                    "authenticationModes": { "type": "array", "minItems": 1, "maxItems": 16, "uniqueItems": true, "items": { "type": "string", "pattern": "^[a-z0-9_-]+(?:\\.[a-z0-9_-]+)*$", "maxLength": 128 } },
                    "limits": { "$ref": "#/$defs/ConnectorLimits" },
                    "configSchema": { "type": "object" }
                }
            },
            "CapabilityInvocation": {
                "type": "object",
                "additionalProperties": false,
                "required": ["capabilityVersion", "input"],
                "properties": {
                    "capabilityVersion": { "type": "integer", "minimum": 1, "maximum": 65535 },
                    "input": {}
                }
            },
            "CapabilityResult": {
                "type": "object",
                "additionalProperties": false,
                "required": ["output"],
                "properties": { "output": {} }
            },
            "ConnectorLimits": {
                "type": "object",
                "additionalProperties": true,
                "required": ["maxConcurrency", "requestTimeoutMs", "maxResponseBytes", "maxPageSize"],
                "properties": {
                    "maxConcurrency": { "type": "integer", "minimum": 1, "maximum": 1024 },
                    "requestTimeoutMs": { "type": "integer", "minimum": 1, "maximum": 300000 },
                    "maxResponseBytes": { "type": "integer", "minimum": 1, "maximum": 16777216 },
                    "maxPageSize": { "type": "integer", "minimum": 1, "maximum": 10000 }
                }
            },
            "ExternalAccountProfile": {
                "type": "object",
                "additionalProperties": true,
                "required": ["subject"],
                "properties": {
                    "subject": { "type": "string", "minLength": 1, "maxLength": 512 },
                    "displayName": { "type": "string", "maxLength": 256 }
                }
            },
            "ConnectorHealth": {
                "type": "object",
                "additionalProperties": true,
                "required": ["status", "checkedAtUnixMs"],
                "properties": {
                    "status": { "enum": ["healthy", "degraded", "unavailable"] },
                    "checkedAtUnixMs": { "type": "integer", "minimum": 1 },
                    "errorCode": { "type": "string", "minLength": 1, "maxLength": 64 }
                }
            },
            "ExternalBalanceSnapshot": {
                "type": "object",
                "additionalProperties": true,
                "required": ["amount", "unit", "observedAtUnixMs"],
                "properties": {
                    "amount": { "type": "string", "pattern": "^(0|[1-9][0-9]*)(\\.[0-9]+)?$", "maxLength": 128 },
                    "unit": { "type": "string", "minLength": 1, "maxLength": 64 },
                    "observedAtUnixMs": { "type": "integer", "minimum": 1 }
                }
            },
            "ConnectorError": {
                "type": "object",
                "additionalProperties": true,
                "required": ["code", "outcome"],
                "properties": {
                    "code": { "enum": ["invalid_configuration", "unsupported", "authentication_required", "forbidden", "rate_limited", "temporarily_unavailable", "conflict", "upstream_failure", "partial_success", "outcome_unknown", "invalid_response"] },
                    "outcome": { "enum": ["not_applied", "applied", "unknown"] },
                    "retryAfterMs": { "type": "integer", "minimum": 0 }
                }
            }
        }
    })
}

/// OpenAPI 3.1 control-plane contract for connector discovery and read-only
/// account capabilities. Host authentication is intentionally transport-owned.
#[must_use]
pub fn openapi_document() -> Value {
    let mut schemas = schema_document()["$defs"]
        .as_object()
        .expect("plugin schema has definitions")
        .clone();
    for definition in schemas.values_mut() {
        rewrite_schema_refs(definition);
    }
    schemas.insert(
        "InvocationMetadata".to_owned(),
        json!({
            "type": "object",
                "additionalProperties": true,
            "required": ["requestId", "deadlineUnixMs"],
            "properties": {
                "requestId": { "type": "string", "minLength": 1, "maxLength": 128 },
                "deadlineUnixMs": { "type": "integer", "minimum": 1 }
            }
        }),
    );

    let common_parameters = vec![
        json!({ "$ref": "#/components/parameters/RequestId" }),
        json!({ "$ref": "#/components/parameters/Deadline" }),
        json!({ "$ref": "#/components/parameters/TraceContext" }),
    ];
    let subject_parameter = json!({
        "name": "subject",
        "in": "path",
        "required": true,
        "schema": { "type": "string", "minLength": 1, "maxLength": 512 }
    });

    let mut document = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "AnyFlows Platform Connector Contract",
            "version": "1.0.0",
            "description": "Language-neutral connector control-plane contract. The host capability token is short-lived and connection-scoped; platform secrets, wallet authority, and database access are not part of this contract."
        },
        "security": [{ "HostCapabilityToken": [] }],
        "paths": {
            "/v1/manifest": {
                "get": {
                    "operationId": "getPluginManifest",
                    "summary": "Return plugin identity and declared capabilities",
                    "parameters": common_parameters,
                    "responses": {
                        "200": {
                            "description": "Plugin manifest",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/PluginManifest" } } }
                        }
                    }
                }
            },
            "/v1/health": {
                "get": {
                    "operationId": "getConnectorHealth",
                    "summary": "Return connector health without performing side effects",
                    "parameters": common_parameters,
                    "responses": {
                        "200": {
                            "description": "Connector health",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ConnectorHealth" } } }
                        }
                    }
                }
            },
            "/v1/accounts/{subject}": {
                "get": {
                    "operationId": "readExternalAccountProfile",
                    "summary": "Read the profile for a previously linked external subject",
                    "parameters": [subject_parameter.clone(), common_parameters[0].clone(), common_parameters[1].clone(), common_parameters[2].clone()],
                    "responses": {
                        "200": {
                            "description": "External account profile",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ExternalAccountProfile" } } }
                        },
                        "default": { "$ref": "#/components/responses/ConnectorError" }
                    }
                }
            },
            "/v1/accounts/{subject}/balance": {
                "get": {
                    "operationId": "readExternalBalanceSnapshot",
                    "summary": "Read an external balance snapshot in its native unit",
                    "parameters": [subject_parameter, common_parameters[0].clone(), common_parameters[1].clone(), common_parameters[2].clone()],
                    "responses": {
                        "200": {
                            "description": "External balance snapshot; not a host wallet balance",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ExternalBalanceSnapshot" } } }
                        },
                        "default": { "$ref": "#/components/responses/ConnectorError" }
                    }
                }
            },
            "/v1/capabilities/{capabilityId}": {
                "post": {
                    "operationId": "invokeReadCapability",
                    "summary": "Invoke one negotiated read capability using its declared input and output schemas",
                    "parameters": [
                        { "name": "capabilityId", "in": "path", "required": true, "schema": { "type": "string", "pattern": "^[a-z](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z](?:[a-z0-9-]*[a-z0-9])?)*$", "minLength": 1, "maxLength": 128 } },
                        common_parameters[0].clone(), common_parameters[1].clone(), common_parameters[2].clone()
                    ],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CapabilityInvocation" } } }
                    },
                    "responses": {
                        "200": {
                            "description": "Validated capability result",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CapabilityResult" } } }
                        },
                        "default": { "$ref": "#/components/responses/ConnectorError" }
                    }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "HostCapabilityToken": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "short-lived scoped token",
                    "description": "Issued by the host for one connector instance and granted capability set. Not a platform credential."
                }
            },
            "parameters": {
                "RequestId": { "name": "X-Request-Id", "in": "header", "required": true, "schema": { "type": "string", "minLength": 1, "maxLength": 128 } },
                "Deadline": { "name": "X-Deadline-Unix-Ms", "in": "header", "required": true, "schema": { "type": "integer", "minimum": 1 } },
                "TraceContext": { "name": "traceparent", "in": "header", "required": false, "schema": { "type": "string", "maxLength": 128 } }
            },
            "responses": {
                "ConnectorError": {
                    "description": "Structured connector error without upstream bodies or secrets",
                    "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ConnectorError" } } }
                }
            }
        }
    });
    document["components"]["schemas"] = Value::Object(schemas);
    document
}

fn rewrite_schema_refs(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(Value::String(reference)) = object.get_mut("$ref")
                && let Some(name) = reference.strip_prefix("#/$defs/")
            {
                *reference = format!("#/components/schemas/{name}");
            }
            for child in object.values_mut() {
                rewrite_schema_refs(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_schema_refs(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{
        CapabilityAccess, CapabilityDeclaration, CapabilityId, CapabilityInvocation,
        CapabilityResult, ConnectorError, ConnectorErrorCode, ContractVersion,
        ContractVersionRange, ExternalAccountProfile, ExternalBalanceSnapshot, PluginId,
        PluginManifest, PluginVersion, SideEffectClass,
    };

    #[test]
    fn schema_definitions_cover_serialized_wire_fields() {
        assert_eq!(
            serde_json::from_str::<Value>(include_str!(
                "../../../contracts/plugins/v1/schema.json"
            ))
            .unwrap(),
            schema_document()
        );
        assert_eq!(
            serde_json::from_str::<Value>(include_str!(
                "../../../contracts/plugins/v1/openapi.json"
            ))
            .unwrap(),
            openapi_document()
        );

        let manifest = PluginManifest {
            plugin_id: PluginId::new("org.example.account-source").unwrap(),
            display_name: "Example Account Source".to_owned(),
            plugin_version: PluginVersion::new("1.2.0").unwrap(),
            contract_range: ContractVersionRange::new(
                ContractVersion::new(1, 0),
                ContractVersion::new(1, 2),
            )
            .unwrap(),
            capabilities: vec![CapabilityDeclaration {
                id: CapabilityId::new("external.account.balance.read").unwrap(),
                version: 1,
                access: CapabilityAccess::Read,
                side_effect: SideEffectClass::ExternalRead,
                write_semantics: None,
                input_schema: None,
                output_schema: None,
            }],
            authentication_modes: vec!["api_key".to_owned()],
            limits: crate::ConnectorLimits {
                max_concurrency: 8,
                request_timeout_ms: 10_000,
                max_response_bytes: 1_048_576,
                max_page_size: 100,
            },
            config_schema: json!({ "type": "object" }),
        };
        let account = ExternalAccountProfile {
            subject: "opaque-123".to_owned(),
            display_name: Some("Example".to_owned()),
        };
        let balance = ExternalBalanceSnapshot {
            amount: "12.3400".to_owned(),
            unit: "quota".to_owned(),
            observed_at_unix_ms: 1_800_000_000_000,
        };
        let error = ConnectorError::rate_limited(250);
        let invocation = CapabilityInvocation {
            capability_version: 1,
            input: json!({ "query": "sample" }),
        };
        let result = CapabilityResult {
            output: json!({ "items": ["sample"] }),
        };
        let schema = schema_document();

        assert_schema_fields(
            &schema,
            "PluginManifest",
            &serde_json::to_value(manifest.clone()).unwrap(),
        );
        assert_schema_fields(
            &schema,
            "ExternalAccountProfile",
            &serde_json::to_value(account.clone()).unwrap(),
        );
        assert_schema_fields(
            &schema,
            "ExternalBalanceSnapshot",
            &serde_json::to_value(balance.clone()).unwrap(),
        );
        assert_schema_fields(
            &schema,
            "ConnectorError",
            &serde_json::to_value(error).unwrap(),
        );
        assert_schema_fields(
            &schema,
            "CapabilityInvocation",
            &serde_json::to_value(invocation).unwrap(),
        );
        assert_schema_fields(
            &schema,
            "CapabilityResult",
            &serde_json::to_value(result).unwrap(),
        );
        assert_eq!(
            schema["$defs"]["CapabilityDeclaration"]["dependentRequired"]["inputSchema"],
            json!(["outputSchema"])
        );
        assert_eq!(
            serde_json::to_value(ConnectorErrorCode::OutcomeUnknown).unwrap(),
            "outcome_unknown"
        );
        let unknown = ConnectorError::new(
            ConnectorErrorCode::OutcomeUnknown,
            crate::OperationOutcome::Unknown,
        );
        assert!(unknown.requires_reconciliation());
        let openapi = openapi_document();
        assert_eq!(openapi["openapi"], "3.1.0");
        assert!(openapi["paths"]["/v1/manifest"]["get"].is_object());
        assert!(openapi["paths"]["/v1/accounts/{subject}/balance"]["get"].is_object());
        assert!(openapi["paths"]["/v1/capabilities/{capabilityId}"]["post"].is_object());
        assert_openapi_refs_resolve(&openapi, &openapi);
        assert!(account.validate().is_ok());
        assert!(balance.validate().is_ok());
        assert!(
            ExternalAccountProfile {
                subject: "invalid\nsubject".to_owned(),
                display_name: None,
            }
            .validate()
            .is_err()
        );
        assert!(
            ExternalBalanceSnapshot {
                amount: "1".to_owned(),
                unit: "USD\n".to_owned(),
                observed_at_unix_ms: 1_800_000_000_000,
            }
            .validate()
            .is_err()
        );

        let mut manifest_value = serde_json::to_value(manifest.clone()).unwrap();
        manifest_value["futureOptionalField"] = json!("ignored by v1");
        assert!(serde_json::from_value::<PluginManifest>(manifest_value).is_ok());
        let mut legacy_manifest = serde_json::to_value(manifest).unwrap();
        legacy_manifest
            .as_object_mut()
            .unwrap()
            .remove("displayName");
        let legacy: PluginManifest = serde_json::from_value(legacy_manifest).unwrap();
        assert!(legacy.display_name.is_empty());
        assert!(legacy.validate().is_ok());
    }

    fn assert_schema_fields(schema: &Value, definition: &str, value: &Value) {
        let declared = schema["$defs"][definition]["properties"]
            .as_object()
            .expect("schema definition must declare properties")
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let serialized = value
            .as_object()
            .expect("wire value must serialize as an object")
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        assert!(serialized.is_subset(&declared), "{definition}");
    }

    fn assert_openapi_refs_resolve(root: &Value, value: &Value) {
        match value {
            Value::Object(object) => {
                if let Some(Value::String(reference)) = object.get("$ref") {
                    assert!(!reference.starts_with("#/$defs/"), "{reference}");
                    if let Some(name) = reference.strip_prefix("#/components/schemas/") {
                        assert!(
                            root["components"]["schemas"][name].is_object(),
                            "{reference}"
                        );
                    }
                    if let Some(name) = reference.strip_prefix("#/components/responses/") {
                        assert!(
                            root["components"]["responses"][name].is_object(),
                            "{reference}"
                        );
                    }
                    if let Some(name) = reference.strip_prefix("#/components/parameters/") {
                        assert!(
                            root["components"]["parameters"][name].is_object(),
                            "{reference}"
                        );
                    }
                }
                for child in object.values() {
                    assert_openapi_refs_resolve(root, child);
                }
            }
            Value::Array(items) => {
                for item in items {
                    assert_openapi_refs_resolve(root, item);
                }
            }
            _ => {}
        }
    }
}
