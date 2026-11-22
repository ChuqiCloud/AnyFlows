package anyflowsplugin

import (
	"bytes"
	"encoding/json"
	"testing"
	"time"
)

func TestHandlerErrorsUseProtocolEnvelopeAndKeepStreamUsable(t *testing.T) {
	deadline := time.Now().Add(time.Minute).UnixMilli()
	frames := []map[string]any{
		{
			"protocolVersion":      1,
			"pluginId":             "org.example.test",
			"pluginVersion":        "1.0.0",
			"artifactSha256":       "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
			"deploymentGeneration": 7,
			"requestId":            "request-1",
			"deadlineUnixMs":       deadline,
			"operation": map[string]any{
				"operation":         "invoke_capability",
				"connectionId":      "test",
				"publicConfig":      map[string]any{},
				"capabilityId":      "catalog.models.read",
				"capabilityVersion": 1,
			},
		},
		{
			"protocolVersion":      1,
			"pluginId":             "org.example.test",
			"pluginVersion":        "1.0.0",
			"artifactSha256":       "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
			"deploymentGeneration": 7,
			"requestId":            "request-2",
			"deadlineUnixMs":       deadline,
			"operation":            map[string]any{"operation": "health"},
		},
	}
	var input bytes.Buffer
	for _, frame := range frames {
		encoded, err := json.Marshal(frame)
		if err != nil {
			t.Fatal(err)
		}
		input.Write(encoded)
		input.WriteByte('\n')
	}
	var output bytes.Buffer
	plugin := Plugin{
		Manifest: Manifest{"pluginId": "org.example.test", "pluginVersion": "1.0.0"},
		Handle: func(map[string]any) (any, error) {
			return nil, &Error{Code: "forbidden", Outcome: "not_applied"}
		},
	}
	if err := plugin.Run(&input, &output); err != nil {
		t.Fatal(err)
	}
	lines := bytes.Split(bytes.TrimSpace(output.Bytes()), []byte{'\n'})
	if len(lines) != 2 {
		t.Fatalf("expected two responses, got %d", len(lines))
	}
	var first map[string]any
	if err := json.Unmarshal(lines[0], &first); err != nil {
		t.Fatal(err)
	}
	result := first["result"].(map[string]any)
	value := result["value"].(map[string]any)
	if result["kind"] != "error" || value["code"] != "forbidden" || value["outcome"] != "not_applied" {
		t.Fatalf("unexpected error envelope: %v", result)
	}
	var second map[string]any
	if err := json.Unmarshal(lines[1], &second); err != nil {
		t.Fatal(err)
	}
	if second["result"].(map[string]any)["kind"] != "health" {
		t.Fatalf("stream did not continue after a handler error: %v", second)
	}
}
