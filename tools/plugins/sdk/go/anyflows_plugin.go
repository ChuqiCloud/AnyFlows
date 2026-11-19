// Package anyflowsplugin implements the AnyFlows Plugin Protocol v1 envelope.
package anyflowsplugin

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"time"
)

type Manifest map[string]any
type Handler func(map[string]any) (any, error)

type Error struct{ Code string }
func (e *Error) Error() string { return e.Code }

type Plugin struct {
	Manifest Manifest
	Handle Handler
}

func (p Plugin) Run(in io.Reader, out io.Writer) error {
	scanner := bufio.NewScanner(in)
	scanner.Buffer(make([]byte, 64*1024), 1<<20)
	encoder := json.NewEncoder(out)
	for scanner.Scan() {
		var frame map[string]any
		if err := json.Unmarshal(scanner.Bytes(), &frame); err != nil { return fmt.Errorf("invalid host frame: %w", err) }
		operation, ok := frame["operation"].(map[string]any)
		if !ok { return &Error{Code: "invalid_configuration"} }
		kind, _ := operation["operation"].(string)
		var result map[string]any
		switch kind {
		case "manifest": result = map[string]any{"kind":"manifest", "value":p.Manifest}
		case "health": result = map[string]any{"kind":"health", "value":map[string]any{"status":"healthy", "checkedAtUnixMs":time.Now().UnixMilli()}}
		case "invoke_capability", "invoke_task", "invoke_provider", "invoke_credential", "plugin_route":
			value, err := p.Handle(operation)
			if err != nil { return err }
			outputKind := map[string]string{"invoke_capability":"capability","plugin_route":"capability","invoke_task":"task","invoke_provider":"provider","invoke_credential":"credential"}[kind]
			if outputKind == "capability" { result = map[string]any{"kind":outputKind, "value":map[string]any{"output":value}} } else if object, ok := value.(map[string]any); ok { result = map[string]any{"kind":outputKind, "value":object} } else { return &Error{Code:"invalid_response"} }
		default: return &Error{Code:"unsupported"}
		}
		response := map[string]any{"protocolVersion":frame["protocolVersion"], "pluginId":frame["pluginId"], "pluginVersion":frame["pluginVersion"], "artifactSha256":frame["artifactSha256"], "deploymentGeneration":frame["deploymentGeneration"], "requestId":frame["requestId"], "result":result}
		if err := encoder.Encode(response); err != nil { return err }
	}
	return scanner.Err()
}
