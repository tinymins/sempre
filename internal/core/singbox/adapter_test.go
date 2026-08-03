package singbox

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestPrepareRuntimeIsolatesControlAPIFromUserConfig(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	config := filepath.Join(root, "source.json")
	original := []byte(`{
  "custom": {"preserved": true},
  "experimental": {"other": "value", "clash_api": {"external_controller": "0.0.0.0:9090", "secret": "user-secret", "custom": 42}}
}`)
	if err := os.WriteFile(config, original, 0o600); err != nil {
		t.Fatal(err)
	}
	spec, err := New().PrepareRuntime(config, filepath.Join(root, "runtime"))
	if err != nil {
		t.Fatal(err)
	}
	after, err := os.ReadFile(config)
	if err != nil || string(after) != string(original) {
		t.Fatalf("source configuration changed: %v", err)
	}
	data, err := os.ReadFile(spec.Config)
	if err != nil {
		t.Fatal(err)
	}
	var document map[string]any
	if err := json.Unmarshal(data, &document); err != nil {
		t.Fatal(err)
	}
	experimental := document["experimental"].(map[string]any)
	clashAPI := experimental["clash_api"].(map[string]any)
	if document["custom"].(map[string]any)["preserved"] != true || experimental["other"] != "value" || clashAPI["custom"] != float64(42) {
		t.Fatalf("unrelated configuration was not preserved: %#v", document)
	}
	if clashAPI["external_controller"] == "0.0.0.0:9090" || clashAPI["external_controller"] != spec.Control.BaseURL[len("http://"):] {
		t.Fatalf("external controller = %#v, control = %#v", clashAPI["external_controller"], spec.Control)
	}
	if clashAPI["secret"] == "user-secret" || clashAPI["secret"] != spec.Control.Secret || spec.Control.Secret == "" {
		t.Fatalf("control secret was not isolated")
	}
	if clashAPI["external_ui"] != "" || clashAPI["access_control_allow_private_network"] != false {
		t.Fatalf("unsafe Clash API settings remain: %#v", clashAPI)
	}
}
