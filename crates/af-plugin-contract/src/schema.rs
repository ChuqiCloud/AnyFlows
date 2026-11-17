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
            { "$ref": "#/$defs/CapabilityResult" },
            { "$ref": "#/$defs/PluginTaskOperation" },
            { "$ref": "#/$defs/PluginTaskResult" },
            { "$ref": "#/$defs/PluginEventDefinition" },
            { "$ref": "#/$defs/ProviderRequestTransformInput" },
            { "$ref": "#/$defs/ProviderRequestTransformOutput" },
            { "$ref": "#/$defs/RequestProcessingInput" },
            { "$ref": "#/$defs/RequestProcessingOutput" },
            { "$ref": "#/$defs/PluginProxyRequest" },
            { "$ref": "#/$defs/PluginProxyResponse" },
            { "$ref": "#/$defs/PluginHostRequest" },
            { "$ref": "#/$defs/PluginHostResponse" },
            { "$ref": "#/$defs/PluginHostProtocol" },
            { "$ref": "#/$defs/PluginUiView" },
            { "$ref": "#/$defs/PluginUiPage" },
            { "$ref": "#/$defs/PluginUiBlock" }
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
            "PluginVersion": {
                "type": "string",
                "pattern": "^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)(-(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\\.(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*)?(\\+[0-9A-Za-z-]+(\\.[0-9A-Za-z-]+)*)?$",
                "maxLength": 128
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
                    "pluginVersion": { "$ref": "#/$defs/PluginVersion" },
                    "contractRange": { "$ref": "#/$defs/ContractVersionRange" },
                    "capabilities": { "type": "array", "minItems": 1, "maxItems": 128, "items": { "$ref": "#/$defs/CapabilityDeclaration" } },
                    "eventSubscriptions": { "type": "array", "maxItems": 32, "items": { "$ref": "#/$defs/PluginEventSubscription" } },
                    "authenticationModes": { "type": "array", "minItems": 1, "maxItems": 16, "uniqueItems": true, "items": { "type": "string", "pattern": "^[a-z0-9_-]+(?:\\.[a-z0-9_-]+)*$", "maxLength": 128 } },
                    "limits": { "$ref": "#/$defs/ConnectorLimits" },
                    "configSchema": { "type": "object" },
                    "uiContributions": { "type": "array", "maxItems": 16, "items": { "$ref": "#/$defs/PluginUiContribution" } },
                    "uiViews": { "type": "array", "maxItems": 8, "items": { "$ref": "#/$defs/PluginUiView" } },
                    "uiPages": { "type": "array", "maxItems": 32, "items": { "$ref": "#/$defs/PluginUiPage" } }
                }
            },
            "PluginEventSubscription": {
                "type": "object",
                "additionalProperties": false,
                "required": ["eventKind", "capabilityId", "capabilityVersion"],
                "properties": {
                    "eventKind": { "enum": ["channel.health.changed", "merchant.service.changed"] },
                    "capabilityId": { "type": "string", "maxLength": 128 },
                    "capabilityVersion": { "type": "integer", "minimum": 1, "maximum": 65535 },
                    "filters": {
                        "type": "object", "additionalProperties": false,
                        "properties": { "status": { "type": "array", "minItems": 1, "maxItems": 2, "uniqueItems": true, "items": { "enum": ["down", "recovered", "active", "paused"] } } }
                    }
                },
                "allOf": [
                    { "if": { "properties": { "eventKind": { "const": "channel.health.changed" } }, "required": ["eventKind"] }, "then": { "properties": { "capabilityId": { "const": "events.channel-health.changed.consume" }, "capabilityVersion": { "const": 1 }, "filters": { "properties": { "status": { "items": { "enum": ["down", "recovered"] } } } } } } },
                    { "if": { "properties": { "eventKind": { "const": "merchant.service.changed" } }, "required": ["eventKind"] }, "then": { "properties": { "capabilityId": { "const": "events.merchant-service.changed.consume" }, "capabilityVersion": { "const": 1 }, "filters": { "properties": { "status": { "items": { "enum": ["active", "paused"] } } } } } } }
                ]
            },
            "PluginEventDefinition": {
                "type": "object",
                "additionalProperties": false,
                "required": ["eventKind", "capabilityId", "capabilityVersion", "payloadSchema", "filterFields"],
                "properties": {
                    "eventKind": { "enum": ["channel.health.changed", "merchant.service.changed"] },
                    "capabilityId": { "type": "string", "maxLength": 128 },
                    "capabilityVersion": { "type": "integer", "minimum": 1, "maximum": 65535 },
                    "payloadSchema": { "type": "object" },
                    "filterFields": { "type": "array", "items": { "type": "object" } }
                }
            },
            "PluginUiContribution": {
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "slot", "component", "title"],
                "properties": {
                    "id": { "type": "string", "pattern": "^[a-z0-9](?:[a-z0-9_-]{0,62}[a-z0-9])?$", "minLength": 1, "maxLength": 64 },
                    "slot": { "enum": ["plugin_management"] },
                    "component": { "enum": ["info_card"] },
                    "title": { "$ref": "#/$defs/PluginUiText" },
                    "description": { "$ref": "#/$defs/PluginUiText" }
                }
            },
            "PluginUiText": {
                "type": "object",
                "additionalProperties": false,
                "minProperties": 1,
                "maxProperties": 2,
                "properties": {
                    "en": { "type": "string", "minLength": 1, "maxLength": 512, "not": { "pattern": "[<>]|[Hh][Tt][Tt][Pp][Ss]?://|[Jj][Aa][Vv][Aa][Ss][Cc][Rr][Ii][Pp][Tt]:|[Dd][Aa][Tt][Aa]:|[Mm][Aa][Ii][Ll][Tt][Oo]:|[Ww][Ww][Ww][.]" } },
                    "zh": { "type": "string", "minLength": 1, "maxLength": 512, "not": { "pattern": "[<>]|[Hh][Tt][Tt][Pp][Ss]?://|[Jj][Aa][Vv][Aa][Ss][Cc][Rr][Ii][Pp][Tt]:|[Dd][Aa][Tt][Aa]:|[Mm][Aa][Ii][Ll][Tt][Oo]:|[Ww][Ww][Ww][.]" } }
                }
            },
            "PluginUiViewSlot": {
                "type": "string",
                "enum": ["plugin_management", "channel_monitor", "channel_model_mapping", "merchant_service", "external_account"]
            },
            "PluginUiTone": {
                "type": "string",
                "enum": ["neutral", "success", "warning", "danger"]
            },
            "PluginUiView": {
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "schemaVersion", "slot", "blocks"],
                "properties": {
                    "id": { "type": "string", "pattern": "^[a-z0-9](?:[a-z0-9_-]{0,62}[a-z0-9])?$", "minLength": 1, "maxLength": 64 },
                    "schemaVersion": { "const": 1 },
                    "slot": { "$ref": "#/$defs/PluginUiViewSlot" },
                    "blocks": { "type": "array", "minItems": 1, "maxItems": 24, "items": { "$ref": "#/$defs/PluginUiBlock" } }
                }
            },
            "PluginUiPage": {
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "schemaVersion", "title", "capabilityId", "capabilityVersion"],
                "properties": {
                    "id": { "type": "string", "pattern": "^[a-z0-9](?:[a-z0-9_-]{0,62}[a-z0-9])?$", "minLength": 1, "maxLength": 64 },
                    "schemaVersion": { "const": 1 },
                    "title": { "$ref": "#/$defs/PluginUiText" },
                    "description": { "$ref": "#/$defs/PluginUiText" },
                    "capabilityId": { "type": "string", "pattern": "^[a-z](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z](?:[a-z0-9-]*[a-z0-9])?)*$", "maxLength": 128 },
                    "capabilityVersion": { "type": "integer", "minimum": 1, "maximum": 65535 },
                    "input": {}
                }
            },
            "PluginUiBlock": {
                "oneOf": [
                    { "type": "object", "additionalProperties": false, "required": ["type", "text"], "properties": { "type": { "const": "text" }, "text": { "$ref": "#/$defs/PluginUiText" } } },
                    { "type": "object", "additionalProperties": false, "required": ["type", "label", "tone"], "properties": { "type": { "const": "badge" }, "label": { "$ref": "#/$defs/PluginUiText" }, "tone": { "$ref": "#/$defs/PluginUiTone" } } },
                    { "type": "object", "additionalProperties": false, "required": ["type", "label", "value"], "properties": { "type": { "const": "metric" }, "label": { "$ref": "#/$defs/PluginUiText" }, "value": { "$ref": "#/$defs/PluginUiText" } } },
                    { "type": "object", "additionalProperties": false, "required": ["type", "columns", "rows"], "properties": { "type": { "const": "table" }, "columns": { "type": "array", "minItems": 1, "maxItems": 8, "items": { "$ref": "#/$defs/PluginUiText" } }, "rows": { "type": "array", "maxItems": 16, "items": { "type": "array", "minItems": 1, "maxItems": 8, "items": { "$ref": "#/$defs/PluginUiText" } } } } },
                    { "type": "object", "additionalProperties": false, "required": ["type", "items"], "properties": { "type": { "const": "list" }, "items": { "type": "array", "minItems": 1, "maxItems": 16, "items": { "$ref": "#/$defs/PluginUiText" } } } },
                    { "type": "object", "additionalProperties": false, "required": ["type", "title", "tone"], "properties": { "type": { "const": "notice" }, "title": { "$ref": "#/$defs/PluginUiText" }, "description": { "$ref": "#/$defs/PluginUiText" }, "tone": { "$ref": "#/$defs/PluginUiTone" } } },
                    { "type": "object", "additionalProperties": false, "required": ["type", "label", "percent"], "properties": { "type": { "const": "progress" }, "label": { "$ref": "#/$defs/PluginUiText" }, "percent": { "type": "integer", "minimum": 0, "maximum": 100 } } },
                    { "type": "object", "additionalProperties": false, "required": ["type", "title"], "properties": { "type": { "const": "info_card" }, "title": { "$ref": "#/$defs/PluginUiText" }, "description": { "$ref": "#/$defs/PluginUiText" } } }
                ]
            },
            "HostHttpMethod": { "type": "string", "enum": ["GET", "HEAD"] },
            "HostHttpRequestProposal": {
                "type": "object", "additionalProperties": false,
                "required": ["endpointId", "method"],
                "properties": {
                    "endpointId": { "type": "string", "pattern": "^[a-z](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z](?:[a-z0-9-]*[a-z0-9])?)*$", "maxLength": 128 },
                    "method": { "$ref": "#/$defs/HostHttpMethod" },
                    "query": { "type": "object", "maxProperties": 32, "propertyNames": { "pattern": "^[A-Za-z0-9_.-]{1,64}$" }, "additionalProperties": { "type": "string", "maxLength": 512 } }
                }
            },
            "HostHttpResponseProjection": {
                "type": "object", "additionalProperties": false,
                "required": ["statusCode", "payload"],
                "properties": {
                    "statusCode": { "type": "integer", "minimum": 200, "maximum": 299 },
                    "payload": {}
                }
            },
            "PluginProxyRequest": {
                "type": "object", "additionalProperties": false,
                "required": ["protocolVersion", "pluginId", "pluginVersion", "artifactSha256", "deploymentGeneration", "requestId", "capabilityId", "capabilityVersion", "proxyRequestId", "request"],
                "properties": {
                    "protocolVersion": { "const": 1 },
                    "pluginId": { "type": "string", "pattern": "^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+$", "maxLength": 128 },
                    "pluginVersion": { "$ref": "#/$defs/PluginVersion" },
                    "artifactSha256": { "type": "string", "pattern": "^[a-f0-9]{64}$" },
                    "deploymentGeneration": { "type": "integer", "minimum": 1 },
                    "requestId": { "type": "string", "pattern": "^[A-Za-z0-9_.-]{1,128}$" },
                    "capabilityId": { "type": "string", "pattern": "^[a-z](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z](?:[a-z0-9-]*[a-z0-9])?)*$", "maxLength": 128 },
                    "capabilityVersion": { "type": "integer", "minimum": 1, "maximum": 65535 },
                    "proxyRequestId": { "type": "string", "pattern": "^[A-Za-z0-9_.-]{1,64}$" },
                    "request": { "$ref": "#/$defs/HostHttpRequestProposal" }
                }
            },
            "PluginProxyError": {
                "type": "object", "additionalProperties": false,
                "required": ["code"],
                "properties": { "code": { "enum": ["authentication_required", "forbidden", "rate_limited", "temporarily_unavailable", "unsupported", "invalid_response"] } }
            },
            "PluginProxyResult": {
                "oneOf": [
                    { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "response" }, "value": { "$ref": "#/$defs/HostHttpResponseProjection" } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "error" }, "value": { "$ref": "#/$defs/PluginProxyError" } } }
                ]
            },
            "PluginProxyResponse": {
                "type": "object", "additionalProperties": false,
                "required": ["protocolVersion", "pluginId", "pluginVersion", "artifactSha256", "deploymentGeneration", "requestId", "capabilityId", "capabilityVersion", "proxyRequestId", "result"],
                "properties": {
                    "protocolVersion": { "const": 1 },
                    "pluginId": { "type": "string", "pattern": "^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+$", "maxLength": 128 },
                    "pluginVersion": { "$ref": "#/$defs/PluginVersion" },
                    "artifactSha256": { "type": "string", "pattern": "^[a-f0-9]{64}$" },
                    "deploymentGeneration": { "type": "integer", "minimum": 1 },
                    "requestId": { "type": "string", "pattern": "^[A-Za-z0-9_.-]{1,128}$" },
                    "capabilityId": { "type": "string", "pattern": "^[a-z](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z](?:[a-z0-9-]*[a-z0-9])?)*$", "maxLength": 128 },
                    "capabilityVersion": { "type": "integer", "minimum": 1, "maximum": 65535 },
                    "proxyRequestId": { "type": "string", "pattern": "^[A-Za-z0-9_.-]{1,64}$" },
                    "result": { "$ref": "#/$defs/PluginProxyResult" }
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
            "ProviderRequestTransformInput": {
                "type": "object",
                "additionalProperties": false,
                "required": ["protocol", "method", "path", "model", "headers", "body"],
                "properties": {
                    "protocol": { "type": "string", "minLength": 1, "maxLength": 64, "pattern": "^[a-z0-9_]+$" },
                    "method": { "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"] },
                    "path": { "type": "string", "minLength": 1, "maxLength": 2048, "pattern": "^/[^?#\\\\]*$" },
                    "model": { "type": "string", "minLength": 1, "maxLength": 256 },
                    "upstreamModel": { "type": "string", "minLength": 1, "maxLength": 256 },
                    "headers": { "type": "object", "maxProperties": 64, "additionalProperties": { "type": "string", "maxLength": 4096 } },
                    "body": {}
                }
            },
            "ProviderHeaderPatch": {
                "type": "object",
                "additionalProperties": false,
                "required": ["set", "remove"],
                "properties": {
                    "set": { "type": "object", "maxProperties": 32, "additionalProperties": { "type": "string", "maxLength": 4096 } },
                    "remove": { "type": "array", "maxItems": 32, "items": { "type": "string", "minLength": 1, "maxLength": 128 } }
                }
            },
            "ProviderTransformRejection": {
                "type": "object",
                "additionalProperties": false,
                "required": ["code", "message"],
                "properties": {
                    "code": { "type": "string", "minLength": 1, "maxLength": 128 },
                    "message": { "type": "string", "minLength": 1, "maxLength": 512 }
                }
            },
            "ProviderRequestTransformOutput": {
                "type": "object",
                "additionalProperties": false,
                "required": ["action", "headers"],
                "properties": {
                    "action": { "enum": ["continue", "reject"] },
                    "path": { "type": "string", "minLength": 1, "maxLength": 2048, "pattern": "^/[^?#\\\\]*$" },
                    "body": {},
                    "headers": { "$ref": "#/$defs/ProviderHeaderPatch" },
                    "rejection": { "$ref": "#/$defs/ProviderTransformRejection" }
                }
            },
            "PluginTaskOperation": {
                "oneOf": [
                    {
                        "type": "object", "additionalProperties": false,
                        "required": ["kind", "model", "request"],
                        "properties": {
                            "kind": { "const": "submit" },
                            "model": { "type": "string", "minLength": 1, "maxLength": 128 },
                            "request": {},
                            "idempotency_key": { "type": "string", "minLength": 1, "maxLength": 256 }
                        }
                    },
                    {
                        "type": "object", "additionalProperties": false,
                        "required": ["kind", "task_id"],
                        "properties": {
                            "kind": { "const": "status" },
                            "task_id": { "type": "string", "minLength": 1, "maxLength": 512 }
                        }
                    },
                    {
                        "type": "object", "additionalProperties": false,
                        "required": ["kind", "task_id", "response"],
                        "properties": {
                            "kind": { "const": "result" },
                            "task_id": { "type": "string", "minLength": 1, "maxLength": 512 },
                            "response": {}
                        }
                    }
                ]
            },
            "PluginTaskResult": {
                "oneOf": [
                    {
                        "type": "object", "additionalProperties": false,
                        "required": ["kind", "value"],
                        "properties": {
                            "kind": { "const": "submitted" },
                            "value": {
                                "type": "object", "additionalProperties": false,
                                "required": ["task_id", "state"],
                                "properties": {
                                    "task_id": { "type": "string", "minLength": 1, "maxLength": 512 },
                                    "state": { "enum": ["submitted", "queued", "running"] },
                                    "progress_basis_points": { "type": "integer", "minimum": 0, "maximum": 10000 }
                                }
                            }
                        }
                    },
                    {
                        "type": "object", "additionalProperties": false,
                        "required": ["kind", "value"],
                        "properties": {
                            "kind": { "const": "status" },
                            "value": {
                                "type": "object", "additionalProperties": false,
                                "required": ["state"],
                                "properties": {
                                    "state": { "enum": ["submitted", "queued", "running", "succeeded", "failed"] },
                                    "progress_basis_points": { "type": "integer", "minimum": 0, "maximum": 10000 },
                                    "failure_code": { "type": "string", "minLength": 1, "maxLength": 128 }
                                }
                            }
                        }
                    },
                    {
                        "type": "object", "additionalProperties": false,
                        "required": ["kind", "value"],
                        "properties": {
                            "kind": { "const": "result" },
                            "value": {
                                "type": "object", "additionalProperties": false,
                                "required": ["task_id", "output"],
                                "properties": {
                                    "task_id": { "type": "string", "minLength": 1, "maxLength": 512 },
                                    "output": {}
                                }
                            }
                        }
                    }
                ]
            },
            "PluginHostOperation": {
                "oneOf": [
                    { "type": "object", "additionalProperties": false, "required": ["operation"], "properties": { "operation": { "const": "manifest" } } },
                    { "type": "object", "additionalProperties": false, "required": ["operation"], "properties": { "operation": { "const": "health" } } },
                    {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["operation", "connectionId", "publicConfig", "capabilityId", "capabilityVersion", "input"],
                        "properties": {
                            "operation": { "const": "invoke_capability" },
                            "connectionId": { "type": "string", "minLength": 1, "maxLength": 128 },
                            "publicConfig": { "type": "object" },
                            "capabilityId": { "type": "string", "pattern": "^[a-z](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z](?:[a-z0-9-]*[a-z0-9])?)*$", "minLength": 1, "maxLength": 128 },
                            "capabilityVersion": { "type": "integer", "minimum": 1, "maximum": 65535 },
                            "input": {}
                        }
                    },
                    {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["operation", "connectionId", "publicConfig", "capabilityId", "capabilityVersion", "task"],
                        "properties": {
                            "operation": { "const": "invoke_task" },
                            "connectionId": { "type": "string", "minLength": 1, "maxLength": 128 },
                            "publicConfig": { "type": "object" },
                            "capabilityId": { "type": "string", "pattern": "^[a-z](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z](?:[a-z0-9-]*[a-z0-9])?)*$", "minLength": 1, "maxLength": 128 },
                            "capabilityVersion": { "type": "integer", "minimum": 1, "maximum": 65535 },
                            "task": { "$ref": "#/$defs/PluginTaskOperation" }
                        }
                    }
                ]
            },
            "PluginHostRequest": {
                "type": "object",
                "additionalProperties": false,
                "required": ["protocolVersion", "pluginId", "pluginVersion", "artifactSha256", "deploymentGeneration", "requestId", "deadlineUnixMs", "operation"],
                "properties": {
                    "protocolVersion": { "const": 1 },
                    "pluginId": { "type": "string", "pattern": "^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+$", "maxLength": 128 },
                    "pluginVersion": { "type": "string", "pattern": "^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)(-(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\\.(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*)?(\\+[0-9A-Za-z-]+(\\.[0-9A-Za-z-]+)*)?$", "maxLength": 128 },
                    "artifactSha256": { "type": "string", "pattern": "^[a-f0-9]{64}$" },
                    "deploymentGeneration": { "type": "integer", "minimum": 1 },
                    "requestId": { "type": "string", "minLength": 1, "maxLength": 128 },
                    "deadlineUnixMs": { "type": "integer", "minimum": 1 },
                    "operation": { "$ref": "#/$defs/PluginHostOperation" }
                }
            },
            "PluginHostResult": {
                "oneOf": [
                    { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "manifest" }, "value": { "$ref": "#/$defs/PluginManifest" } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "health" }, "value": { "$ref": "#/$defs/ConnectorHealth" } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "capability" }, "value": { "$ref": "#/$defs/CapabilityResult" } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "task" }, "value": { "$ref": "#/$defs/PluginTaskResult" } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "error" }, "value": { "$ref": "#/$defs/ConnectorError" } } }
                ]
            },
            "PluginHostResponse": {
                "type": "object",
                "additionalProperties": false,
                "required": ["protocolVersion", "pluginId", "pluginVersion", "artifactSha256", "deploymentGeneration", "requestId", "result"],
                "properties": {
                    "protocolVersion": { "const": 1 },
                    "pluginId": { "type": "string", "pattern": "^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+$", "maxLength": 128 },
                    "pluginVersion": { "type": "string", "pattern": "^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)(-(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\\.(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*)?(\\+[0-9A-Za-z-]+(\\.[0-9A-Za-z-]+)*)?$", "maxLength": 128 },
                    "artifactSha256": { "type": "string", "pattern": "^[a-f0-9]{64}$" },
                    "deploymentGeneration": { "type": "integer", "minimum": 1 },
                    "requestId": { "type": "string", "minLength": 1, "maxLength": 128 },
                    "result": { "$ref": "#/$defs/PluginHostResult" }
                }
            },
            "RequestProcessingInput": {
                "type": "object",
                "additionalProperties": false,
                "required": ["stage", "requestId", "format", "path", "headers", "body"],
                "properties": {
                    "stage": { "enum": ["ingress", "request", "outbound", "egress", "reply", "report"] },
                    "requestId": { "type": "string", "minLength": 1, "maxLength": 128 },
                    "format": { "type": "string", "minLength": 1, "maxLength": 64 },
                    "path": { "type": "string", "minLength": 1, "maxLength": 2048 },
                    "model": { "type": "string", "maxLength": 256 },
                    "upstreamModel": { "type": "string", "maxLength": 256 },
                    "headers": { "type": "object", "maxProperties": 64, "additionalProperties": { "type": "string", "maxLength": 8192 } },
                    "body": {},
                    "responseStatus": { "type": "integer", "minimum": 100, "maximum": 599 },
                    "responseBody": {},
                    "usage": {}
                }
            },
            "JsonPatchOperation": {
                "type": "object",
                "additionalProperties": false,
                "required": ["op", "path"],
                "properties": {
                    "op": { "enum": ["add", "remove", "replace"] },
                    "path": { "type": "array", "maxItems": 32, "items": { "type": "string", "minLength": 1, "maxLength": 256 } },
                    "value": {}
                }
            },
            "RequestProcessingEffect": {
                "oneOf": [
                    { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "json_patch" }, "value": { "type": "object", "additionalProperties": false, "required": ["operations"], "properties": { "operations": { "type": "array", "maxItems": 128, "items": { "$ref": "#/$defs/JsonPatchOperation" } } } } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "header_patch" }, "value": { "type": "object", "additionalProperties": false, "required": ["set", "remove"], "properties": { "set": { "type": "object", "maxProperties": 64, "additionalProperties": { "type": "string", "maxLength": 8192 } }, "remove": { "type": "array", "maxItems": 64, "items": { "type": "string", "minLength": 1, "maxLength": 128 } } } } } }
                ]
            },
            "RequestProcessingFinding": {
                "type": "object",
                "additionalProperties": false,
                "required": ["code", "status", "message"],
                "properties": {
                    "code": { "type": "string", "minLength": 1, "maxLength": 128 },
                    "status": { "enum": ["info", "warning", "fail"] },
                    "message": { "type": "string", "minLength": 1, "maxLength": 1024 },
                    "recommendation": { "type": "string", "maxLength": 1024 }
                }
            },
            "RequestProcessingOutput": {
                "type": "object",
                "additionalProperties": false,
                "required": ["action", "effects", "findings"],
                "properties": {
                    "action": { "enum": ["continue", "reject", "stop"] },
                    "effects": { "type": "array", "maxItems": 32, "items": { "$ref": "#/$defs/RequestProcessingEffect" } },
                    "findings": { "type": "array", "maxItems": 32, "items": { "$ref": "#/$defs/RequestProcessingFinding" } },
                    "message": { "type": "string", "maxLength": 1024 }
                }
            },
            "PluginHostProtocol": {
                "type": "object",
                "additionalProperties": false,
                "required": ["encoding", "framing", "maxFrameBytes", "operations"],
                "properties": {
                    "encoding": { "const": "utf-8-json" },
                    "framing": { "const": "one-json-object-per-line" },
                    "maxFrameBytes": { "type": "integer", "minimum": 1, "maximum": 16777216 },
                    "operations": { "const": ["manifest", "health", "invoke_capability", "invoke_task"] }
                }
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
            "description": "Language-neutral connector contract. Sidecars use bounded UTF-8 JSON Lines over host-managed stdin/stdout; platform secrets, wallet authority, and database access are not part of this contract."
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
        ContractVersionRange, ExternalAccountProfile, ExternalBalanceSnapshot, HostHttpMethod,
        HostHttpRequestProposal, PluginHostOperation, PluginHostRequest, PluginHostResponse,
        PluginHostResult, PluginId, PluginManifest, PluginProxyError, PluginProxyErrorCode,
        PluginProxyRequest, PluginProxyResponse, PluginProxyResult, PluginVersion, SideEffectClass,
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
            event_subscriptions: Vec::new(),
            authentication_modes: vec!["api_key".to_owned()],
            limits: crate::ConnectorLimits {
                max_concurrency: 8,
                request_timeout_ms: 10_000,
                max_response_bytes: 1_048_576,
                max_page_size: 100,
            },
            config_schema: json!({ "type": "object" }),
            ui_contributions: vec![crate::PluginUiContribution {
                id: "service-summary".to_owned(),
                slot: crate::PluginUiSlot::PluginManagement,
                component: crate::PluginUiComponent::InfoCard,
                title: crate::PluginUiText {
                    en: Some("Service summary".to_owned()),
                    zh: Some("服务摘要".to_owned()),
                },
                description: None,
            }],
            ui_views: vec![crate::PluginUiView {
                id: "snapshot-summary".to_owned(),
                schema_version: 1,
                slot: crate::PluginUiViewSlot::ExternalAccount,
                blocks: vec![crate::PluginUiBlock::Metric {
                    label: crate::PluginUiText {
                        en: Some("Balance".to_owned()),
                        zh: None,
                    },
                    value: crate::PluginUiText {
                        en: Some("12.50 USD".to_owned()),
                        zh: None,
                    },
                }],
            }],
            ui_pages: Vec::new(),
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
        let host_request = PluginHostRequest {
            protocol_version: 1,
            plugin_id: manifest.plugin_id.clone(),
            plugin_version: manifest.plugin_version.clone(),
            artifact_sha256: "a".repeat(64),
            deployment_generation: 4,
            request_id: "request-1".to_owned(),
            deadline_unix_ms: 1_800_000_000_000,
            operation: PluginHostOperation::InvokeCapability {
                connection_id: "connection-1".to_owned(),
                public_config: json!({ "scope": "catalog" }),
                capability_id: "catalog.models.search".to_owned(),
                capability_version: 1,
                input: json!({ "query": "sample" }),
            },
        };
        let host_response = PluginHostResponse {
            protocol_version: 1,
            plugin_id: manifest.plugin_id.clone(),
            plugin_version: manifest.plugin_version.clone(),
            artifact_sha256: "a".repeat(64),
            deployment_generation: 4,
            request_id: "request-1".to_owned(),
            result: PluginHostResult::Manifest(manifest.clone()),
        };
        let proxy_request = PluginProxyRequest {
            protocol_version: 1,
            plugin_id: manifest.plugin_id.clone(),
            plugin_version: manifest.plugin_version.clone(),
            artifact_sha256: "a".repeat(64),
            deployment_generation: 4,
            request_id: "request-1".to_owned(),
            capability_id: CapabilityId::new("external.account.profile.read").unwrap(),
            capability_version: 1,
            proxy_request_id: "proxy-1".to_owned(),
            request: HostHttpRequestProposal {
                endpoint_id: "external.account.self.read".to_owned(),
                method: HostHttpMethod::Get,
                query: Default::default(),
            },
        };
        let proxy_response = PluginProxyResponse {
            protocol_version: 1,
            plugin_id: manifest.plugin_id.clone(),
            plugin_version: manifest.plugin_version.clone(),
            artifact_sha256: "a".repeat(64),
            deployment_generation: 4,
            request_id: "request-1".to_owned(),
            capability_id: CapabilityId::new("external.account.profile.read").unwrap(),
            capability_version: 1,
            proxy_request_id: "proxy-1".to_owned(),
            result: PluginProxyResult::Error(PluginProxyError {
                code: PluginProxyErrorCode::AuthenticationRequired,
            }),
        };
        let schema = schema_document();

        assert_schema_fields(
            &schema,
            "PluginManifest",
            &serde_json::to_value(manifest.clone()).unwrap(),
        );
        assert_schema_fields(
            &schema,
            "PluginUiView",
            &serde_json::to_value(manifest.ui_views[0].clone()).unwrap(),
        );
        assert_eq!(
            schema["$defs"]["PluginUiView"]["properties"]["schemaVersion"]["const"],
            1
        );
        assert_eq!(
            schema["$defs"]["PluginUiViewSlot"]["enum"],
            json!([
                "plugin_management",
                "channel_monitor",
                "channel_model_mapping",
                "merchant_service",
                "external_account"
            ])
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
        assert_schema_fields(
            &schema,
            "PluginHostRequest",
            &serde_json::to_value(host_request).unwrap(),
        );
        assert_schema_fields(
            &schema,
            "PluginHostResponse",
            &serde_json::to_value(host_response).unwrap(),
        );
        assert_schema_fields(
            &schema,
            "PluginProxyRequest",
            &serde_json::to_value(proxy_request).unwrap(),
        );
        assert_schema_fields(
            &schema,
            "PluginProxyResponse",
            &serde_json::to_value(proxy_response).unwrap(),
        );
        assert!(schema["$defs"]["PluginHostProtocol"].is_object());
        assert_eq!(
            PluginHostRequest {
                protocol_version: 2,
                plugin_id: manifest.plugin_id.clone(),
                plugin_version: manifest.plugin_version.clone(),
                artifact_sha256: "a".repeat(64),
                deployment_generation: 4,
                request_id: "request-1".to_owned(),
                deadline_unix_ms: 1_800_000_000_000,
                operation: PluginHostOperation::Health,
            }
            .validate(),
            Err(crate::ContractValidationError::InvalidExternalValue)
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
        assert!(schema["$defs"]["PluginHostProtocol"].is_object());
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
