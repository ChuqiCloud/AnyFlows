// Package anyflowsplugin implements the AnyFlows Plugin Protocol v1 envelope.
package anyflowsplugin

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
	"time"
)

type Manifest map[string]any
type Handler func(map[string]any) (any, error)

type Error struct {
	Code    string
	Outcome string
}

func (e *Error) Error() string { return e.Code }

func connectorFailure(err error) map[string]any {
	code := "upstream_failure"
	outcome := "unknown"
	var pluginError *Error
	if errors.As(err, &pluginError) {
		code = pluginError.Code
		if pluginError.Outcome != "" {
			outcome = pluginError.Outcome
		}
	}
	switch code {
	case "invalid_configuration", "unsupported", "authentication_required", "forbidden",
		"rate_limited", "temporarily_unavailable", "conflict", "upstream_failure",
		"partial_success", "outcome_unknown", "invalid_response":
	default:
		code = "upstream_failure"
	}
	switch outcome {
	case "not_applied", "applied", "unknown":
	default:
		outcome = "unknown"
	}
	return map[string]any{"kind": "error", "value": map[string]any{"code": code, "outcome": outcome}}
}

func failureResult(code, outcome string) map[string]any {
	return connectorFailure(&Error{Code: code, Outcome: outcome})
}

func validHostFrame(frame map[string]any) bool {
	const maxSafeInteger = 9_007_199_254_740_991
	if len(frame) != 8 {
		return false
	}
	for _, key := range []string{
		"protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
		"deploymentGeneration", "requestId", "deadlineUnixMs", "operation",
	} {
		if _, ok := frame[key]; !ok {
			return false
		}
	}
	protocolVersion, protocolOK := frame["protocolVersion"].(float64)
	pluginID, pluginOK := frame["pluginId"].(string)
	pluginVersion, versionOK := frame["pluginVersion"].(string)
	digest, digestOK := frame["artifactSha256"].(string)
	generation, generationOK := frame["deploymentGeneration"].(float64)
	requestID, requestOK := frame["requestId"].(string)
	deadline, deadlineOK := frame["deadlineUnixMs"].(float64)
	_, operationOK := frame["operation"].(map[string]any)
	return protocolOK && protocolVersion == 1 && pluginOK && pluginID != "" && strings.TrimSpace(pluginID) == pluginID &&
		versionOK && pluginVersion != "" && strings.TrimSpace(pluginVersion) == pluginVersion &&
		digestOK && len(digest) == 64 && strings.Trim(digest, "0123456789abcdef") == "" &&
		generationOK && generation >= 1 && generation <= maxSafeInteger && generation == float64(uint64(generation)) &&
		requestOK && strings.TrimSpace(requestID) != "" && len([]byte(requestID)) <= 128 && strings.TrimSpace(requestID) == requestID &&
		deadlineOK && deadline >= 1 && deadline <= maxSafeInteger && deadline == float64(uint64(deadline)) && operationOK
}

type Plugin struct {
	Manifest Manifest
	Handle   Handler
}

func (p Plugin) Run(in io.Reader, out io.Writer) error {
	scanner := bufio.NewScanner(in)
	scanner.Buffer(make([]byte, 64*1024), 1<<20)
	encoder := json.NewEncoder(out)
	for scanner.Scan() {
		var frame map[string]any
		if err := json.Unmarshal(scanner.Bytes(), &frame); err != nil {
			return fmt.Errorf("invalid host frame: %w", err)
		}
		if !validHostFrame(frame) {
			return &Error{Code: "invalid_configuration"}
		}
		operation, ok := frame["operation"].(map[string]any)
		if !ok {
			return &Error{Code: "invalid_configuration"}
		}
		kind, _ := operation["operation"].(string)
		var result map[string]any
		deadline := int64(frame["deadlineUnixMs"].(float64))
		if time.Now().UnixMilli() >= deadline {
			result = failureResult("temporarily_unavailable", "not_applied")
		} else {
			switch kind {
			case "manifest":
				result = map[string]any{"kind": "manifest", "value": p.Manifest}
			case "health":
				result = map[string]any{"kind": "health", "value": map[string]any{"status": "healthy", "checkedAtUnixMs": time.Now().UnixMilli()}}
			case "invoke_capability", "invoke_task", "invoke_provider", "invoke_credential", "plugin_route":
				value, err := p.Handle(operation)
				if err != nil {
					result = connectorFailure(err)
					break
				}
				if time.Now().UnixMilli() >= deadline {
					result = failureResult("outcome_unknown", "unknown")
					break
				}
				outputKind := map[string]string{"invoke_capability": "capability", "plugin_route": "capability", "invoke_task": "task", "invoke_provider": "provider", "invoke_credential": "credential"}[kind]
				if outputKind == "capability" {
					result = map[string]any{"kind": outputKind, "value": map[string]any{"output": value}}
				} else if object, ok := value.(map[string]any); ok {
					result = map[string]any{"kind": outputKind, "value": object}
				} else {
					result = connectorFailure(&Error{Code: "invalid_response", Outcome: "unknown"})
				}
			default:
				result = failureResult("unsupported", "not_applied")
			}
		}
		response := map[string]any{"protocolVersion": frame["protocolVersion"], "pluginId": frame["pluginId"], "pluginVersion": frame["pluginVersion"], "artifactSha256": frame["artifactSha256"], "deploymentGeneration": frame["deploymentGeneration"], "requestId": frame["requestId"], "result": result}
		if err := encoder.Encode(response); err != nil {
			return err
		}
	}
	return scanner.Err()
}
