package core

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"
)

func PrepareClashYAMLRuntime(coreID, config, runtimeDirectory string) (RuntimeSpec, error) {
	data, err := os.ReadFile(config)
	if err != nil {
		return RuntimeSpec{}, fmt.Errorf("read %s configuration: %w", coreID, err)
	}
	document := map[string]any{}
	if err := yaml.Unmarshal(data, &document); err != nil {
		return RuntimeSpec{}, fmt.Errorf("decode %s configuration: %w", coreID, err)
	}
	control, err := NewPrivateControl(coreID, ControlProtocolClashREST)
	if err != nil {
		return RuntimeSpec{}, err
	}
	for _, key := range []string{
		"external-controller-tls", "external-controller-unix", "external-controller-pipe",
		"external-doh-server", "external-ui", "external-ui-name", "external-ui-url", "external-ui-headers",
	} {
		delete(document, key)
	}
	document["external-controller"] = strings.TrimPrefix(control.BaseURL, "http://")
	document["secret"] = control.Secret
	if coreID == "clash-rs" {
		delete(document, "external-controller-cors")
		document["cors-allow-origins"] = []string{"http://localhost.invalid"}
	} else {
		document["external-controller-cors"] = map[string]any{
			"allow-origins": []string{"http://localhost.invalid"}, "allow-private-network": false,
		}
	}
	encoded, err := yaml.Marshal(document)
	if err != nil {
		return RuntimeSpec{}, fmt.Errorf("encode %s runtime configuration: %w", coreID, err)
	}
	if err := os.MkdirAll(runtimeDirectory, 0o700); err != nil {
		return RuntimeSpec{}, err
	}
	runtimeConfig := filepath.Join(runtimeDirectory, "config.yaml")
	if err := os.WriteFile(runtimeConfig, encoded, 0o600); err != nil {
		return RuntimeSpec{}, fmt.Errorf("write %s runtime configuration: %w", coreID, err)
	}
	return RuntimeSpec{Config: runtimeConfig, Control: control}, nil
}
