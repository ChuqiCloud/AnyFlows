use std::sync::Arc;

use jsonschema::{Draft, JSONSchema};
use regex::RegexBuilder;
use serde_json::Value;

use af_plugin_contract::ContractValidationError;

/// Compiled JSON Schema validator owned by the trusted host runtime.
#[derive(Clone)]
pub(crate) struct CompiledJsonSchema(Arc<JSONSchema>);

impl CompiledJsonSchema {
    pub(crate) fn compile(schema: &Value) -> Result<Self, ContractValidationError> {
        if !schema.is_object()
            || serde_json::to_vec(schema)
                .map_err(|_| ContractValidationError::InvalidJsonSchema)?
                .len()
                > 64 * 1024
            || schema.get("$schema").is_some_and(|draft| {
                draft.as_str() != Some("https://json-schema.org/draft/2020-12/schema")
            })
            || schema_uses_unsupported_references(schema)
            || !schema_patterns_are_bounded(schema)
        {
            return Err(ContractValidationError::InvalidJsonSchema);
        }
        JSONSchema::options()
            .with_draft(Draft::Draft202012)
            .compile(schema)
            .map(|validator| Self(Arc::new(validator)))
            .map_err(|_| ContractValidationError::InvalidJsonSchema)
    }

    /// Validate without returning schema error details or instance values.
    pub(crate) fn validate(&self, instance: &Value) -> Result<(), ContractValidationError> {
        if self.0.is_valid(instance) {
            Ok(())
        } else {
            Err(ContractValidationError::InvalidConfiguration)
        }
    }
}

/// Validate plugin-owned configuration with the host's bounded JSON Schema
/// implementation. Errors contain no submitted configuration values.
pub fn validate_plugin_configuration(
    schema: &Value,
    configuration: &Value,
) -> Result<(), ContractValidationError> {
    CompiledJsonSchema::compile(schema)?.validate(configuration)
}

/// Restrict schema patterns to Rust's linear-time regex subset before the
/// validator compiles them, avoiding unbounded backtracking from manifests.
fn schema_patterns_are_bounded(schema: &Value) -> bool {
    let Some(object) = schema.as_object() else {
        return schema.is_boolean();
    };
    // Draft 2020-12 removed these keywords, but the pinned validator still
    // executes them as extensions; reject them rather than bypass schema checks.
    if object.contains_key("dependencies") || object.contains_key("additionalItems") {
        return false;
    }
    if object
        .get("pattern")
        .is_some_and(|pattern| !pattern.as_str().is_some_and(safe_pattern))
    {
        return false;
    }
    if let Some(patterns) = object.get("patternProperties").and_then(Value::as_object)
        && (patterns.keys().any(|pattern| !safe_pattern(pattern))
            || patterns
                .values()
                .any(|schema| !schema_patterns_are_bounded(schema)))
    {
        return false;
    }

    object
        .iter()
        .all(|(keyword, child)| match keyword.as_str() {
            "$defs" | "definitions" | "properties" | "dependentSchemas" => child
                .as_object()
                .is_some_and(|schemas| schemas.values().all(schema_patterns_are_bounded)),
            "allOf" | "anyOf" | "oneOf" | "prefixItems" => child
                .as_array()
                .is_some_and(|schemas| schemas.iter().all(schema_patterns_are_bounded)),
            "additionalProperties"
            | "items"
            | "contains"
            | "propertyNames"
            | "unevaluatedItems"
            | "unevaluatedProperties"
            | "not"
            | "if"
            | "then"
            | "else"
            | "contentSchema" => schema_patterns_are_bounded(child),
            _ => true,
        })
}

fn safe_pattern(pattern: &str) -> bool {
    RegexBuilder::new(pattern)
        .size_limit(64 * 1024)
        .dfa_size_limit(64 * 1024)
        .build()
        .is_ok()
}

fn schema_uses_unsupported_references(schema: &Value) -> bool {
    let Some(object) = schema.as_object() else {
        return false;
    };
    if object.contains_key("dependencies") || object.contains_key("additionalItems") {
        return true;
    }
    // Dynamic references are not implemented by the pinned validator, even
    // when fragment-local. All remote references are rejected as well.
    if object.contains_key("$dynamicRef")
        || object
            .get("$ref")
            .and_then(Value::as_str)
            .is_some_and(|reference| !reference.starts_with('#'))
    {
        return true;
    }

    object
        .iter()
        .any(|(keyword, child)| match keyword.as_str() {
            "$defs" | "definitions" | "properties" | "patternProperties" | "dependentSchemas" => {
                child
                    .as_object()
                    .is_some_and(|schemas| schemas.values().any(schema_uses_unsupported_references))
            }
            "allOf" | "anyOf" | "oneOf" | "prefixItems" => child
                .as_array()
                .is_some_and(|schemas| schemas.iter().any(schema_uses_unsupported_references)),
            "additionalProperties"
            | "items"
            | "contains"
            | "propertyNames"
            | "unevaluatedItems"
            | "unevaluatedProperties"
            | "not"
            | "if"
            | "then"
            | "else"
            | "contentSchema" => schema_uses_unsupported_references(child),
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{CompiledJsonSchema, validate_plugin_configuration};
    use af_plugin_contract::ContractValidationError;

    #[test]
    fn validates_draft_2020_12_and_redacts_invalid_values() {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "array",
            "prefixItems": [{ "type": "integer" }, { "type": "string" }],
            "items": false
        });
        let validator = CompiledJsonSchema::compile(&schema).unwrap();
        assert_eq!(validator.validate(&json!([7, "safe"])), Ok(()));
        let error = validator
            .validate(&json!([7, "private-token", true]))
            .unwrap_err();
        assert_eq!(error, ContractValidationError::InvalidConfiguration);
        assert!(!error.to_string().contains("private-token"));
    }

    #[test]
    fn validates_deployment_configuration_without_exposing_values() {
        let schema = json!({
            "type": "object",
            "properties": { "region": { "type": "string", "enum": ["east", "west"] } },
            "required": ["region"],
            "additionalProperties": false
        });
        assert_eq!(
            validate_plugin_configuration(&schema, &json!({ "region": "east" })),
            Ok(())
        );
        let error = validate_plugin_configuration(&schema, &json!({ "region": "private-token" }))
            .unwrap_err();
        assert_eq!(error, ContractValidationError::InvalidConfiguration);
        assert!(!error.to_string().contains("private-token"));
    }

    #[test]
    fn rejects_invalid_schema_and_external_references() {
        assert_eq!(
            CompiledJsonSchema::compile(&json!({ "type": "not-a-json-schema-type" })).err(),
            Some(ContractValidationError::InvalidJsonSchema)
        );
        assert_eq!(
            CompiledJsonSchema::compile(&json!({ "$ref": "https://example.invalid/schema" })).err(),
            Some(ContractValidationError::InvalidJsonSchema)
        );
        assert_eq!(
            CompiledJsonSchema::compile(
                &json!({ "$schema": "https://json-schema.org/draft/07/schema#", "type": "string" })
            )
            .err(),
            Some(ContractValidationError::InvalidJsonSchema)
        );
        assert_eq!(
            CompiledJsonSchema::compile(&json!({ "$dynamicRef": "#local" })).err(),
            Some(ContractValidationError::InvalidJsonSchema)
        );
        let literal_data = json!({ "type": "object", "properties": { "metadata": { "const": { "$ref": "literal data" } } } });
        assert!(CompiledJsonSchema::compile(&literal_data).is_ok());
    }

    #[test]
    fn enforces_config_and_capability_schema_payload_bounds() {
        let oversized = Value::String("x".repeat(64 * 1024));
        let schema = json!({
            "type": "object",
            "properties": { "value": { "const": oversized } }
        });
        assert_eq!(
            CompiledJsonSchema::compile(&schema).err(),
            Some(ContractValidationError::InvalidJsonSchema)
        );
    }

    #[test]
    fn rejects_patterns_outside_the_bounded_regex_subset() {
        assert_eq!(
            CompiledJsonSchema::compile(&json!({
                "type": "string",
                "pattern": "[invalid"
            }))
            .err(),
            Some(ContractValidationError::InvalidJsonSchema)
        );
        assert_eq!(
            CompiledJsonSchema::compile(&json!({
                "type": "object",
                "patternProperties": { "(?=secret)": { "type": "string" } }
            }))
            .err(),
            Some(ContractValidationError::InvalidJsonSchema)
        );
        assert_eq!(
            CompiledJsonSchema::compile(&json!({
                "type": "object",
                "dependencies": { "secret": { "pattern": "(?=secret)" } }
            }))
            .err(),
            Some(ContractValidationError::InvalidJsonSchema)
        );
        assert_eq!(
            CompiledJsonSchema::compile(&json!({
                "type": "array",
                "additionalItems": { "$ref": "#/$defs/local" },
                "$defs": { "local": { "type": "string" } }
            }))
            .err(),
            Some(ContractValidationError::InvalidJsonSchema)
        );
    }
}
