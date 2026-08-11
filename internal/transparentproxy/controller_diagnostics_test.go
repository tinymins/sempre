package transparentproxy

import (
	"context"
	"encoding/json"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"gopkg.in/yaml.v3"
)

func TestDiagnosticsValidateLinuxRoutingAndDNSStructure(t *testing.T) {
	backend := &fakeBackend{}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("gateway")
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{map[string]any{
			"type": "tun", "tag": "tun-in", "interface_name": "sing-box",
			"address": []string{"172.19.0.1/30"}, "auto_route": true,
			"auto_redirect": true, "strict_route": true, "stack": "system",
		}},
		"dns": map[string]any{
			"final":   "remote",
			"servers": []any{map[string]any{"tag": "remote", "detour": "foreign"}},
			"rules":   []any{map[string]any{"rule_set": []string{"geosite-cn"}, "server": "local"}},
		},
		"route": map[string]any{
			"auto_detect_interface": true,
			"final":                 "foreign",
			"rules":                 []any{map[string]any{"rule_set": []string{"geosite-cn"}, "outbound": "direct"}},
		},
	})
	diagnostics := controller.Diagnostics(context.Background(), "sing-box", profile, path)
	if len(diagnostics) != 3 {
		t.Fatalf("diagnostics = %#v", diagnostics)
	}
	for _, diagnostic := range diagnostics {
		if diagnostic.Err != nil {
			t.Fatalf("%s: %v", diagnostic.Name, diagnostic.Err)
		}
	}
	document := readRuntimeConfig(t, path)
	document["dns"].(map[string]any)["final"] = "local"
	path = writeRuntimeConfig(t, document)
	diagnostics = controller.Diagnostics(context.Background(), "sing-box", profile, path)
	if diagnostics[1].Err == nil {
		t.Fatal("expected invalid DNS final to fail diagnostics")
	}
}

func TestDiagnosticsWarnsWhenFakeIPOverlapsLocalRoute(t *testing.T) {
	backend := &fakeBackend{inventory: Inventory{VPNPrefixes: []string{"198.18.0.0/16"}}}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("gateway")
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{map[string]any{
			"type": "tun", "tag": "tun-in", "interface_name": "sempre-tun",
			"address": []string{"172.19.0.1/30"}, "auto_route": true,
			"auto_redirect": true, "strict_route": true, "stack": "system",
		}},
		"dns": map[string]any{
			"final": "remote",
			"servers": []any{
				map[string]any{"tag": "fakeip", "type": "fakeip", "inet4_range": "198.18.0.0/15"},
				map[string]any{"tag": "remote", "detour": "foreign"},
			},
			"rules": []any{map[string]any{"rule_set": []string{"geosite-cn"}, "server": "local"}},
		},
		"route": map[string]any{
			"auto_detect_interface": true,
			"final":                 "foreign",
			"rules":                 []any{map[string]any{"rule_set": []string{"geosite-cn"}, "outbound": "direct"}},
		},
	})

	diagnostics := controller.Diagnostics(context.Background(), "sing-box", profile, path)
	diagnostic := findDiagnostic(diagnostics, "Linux fake-ip route overlap")
	if diagnostic == nil || !diagnostic.Warning || diagnostic.Err == nil {
		t.Fatalf("fake-ip warning diagnostic = %#v", diagnostic)
	}
	if !strings.Contains(diagnostic.Err.Error(), "VPN route 198.18.0.0/16") {
		t.Fatalf("fake-ip warning = %v", diagnostic.Err)
	}
}

func TestDiagnosticsRejectsFakeIPRouteExclusion(t *testing.T) {
	backend := &fakeBackend{}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("gateway")
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{map[string]any{
			"type": "tun", "tag": "tun-in", "interface_name": "sempre-tun",
			"address": []string{"172.19.0.1/30"}, "auto_route": true,
			"auto_redirect": true, "strict_route": true, "stack": "system",
			"route_exclude_address": []string{"198.18.0.0/16"},
		}},
		"dns": map[string]any{
			"final": "remote",
			"servers": []any{
				map[string]any{"tag": "fakeip", "type": "fakeip", "inet4_range": "198.18.0.0/15"},
				map[string]any{"tag": "remote", "detour": "foreign"},
			},
			"rules": []any{map[string]any{"rule_set": []string{"geosite-cn"}, "server": "local"}},
		},
		"route": map[string]any{
			"auto_detect_interface": true,
			"final":                 "foreign",
			"rules":                 []any{map[string]any{"rule_set": []string{"geosite-cn"}, "outbound": "direct"}},
		},
	})

	diagnostics := controller.Diagnostics(context.Background(), "sing-box", profile, path)
	diagnostic := findDiagnostic(diagnostics, "Linux fake-ip route capture")
	if diagnostic == nil || diagnostic.Warning || diagnostic.Err == nil {
		t.Fatalf("fake-ip error diagnostic = %#v", diagnostic)
	}
	if !strings.Contains(diagnostic.Err.Error(), "excluded route 198.18.0.0/16") {
		t.Fatalf("fake-ip error = %v", diagnostic.Err)
	}
}

func writeRuntimeConfig(t *testing.T, value map[string]any) string {
	t.Helper()
	data, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func readRuntimeConfig(t *testing.T, path string) map[string]any {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var result map[string]any
	if err := json.Unmarshal(data, &result); err != nil {
		t.Fatal(err)
	}
	return result
}

func writeYAMLRuntimeConfig(t *testing.T, value map[string]any) string {
	t.Helper()
	data, err := yaml.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "config.yaml")
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func readYAMLRuntimeConfig(t *testing.T, path string) map[string]any {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	result := map[string]any{}
	if err := yaml.Unmarshal(data, &result); err != nil {
		t.Fatal(err)
	}
	return result
}

func listenTCP(t *testing.T) net.Listener {
	t.Helper()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	return listener
}

func listenerPort(t *testing.T, listener net.Listener) int {
	t.Helper()
	_, value, err := net.SplitHostPort(listener.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	port, err := strconv.Atoi(value)
	if err != nil {
		t.Fatal(err)
	}
	return port
}

func equalStrings(left, right []string) bool {
	if len(left) != len(right) {
		return false
	}
	for index := range left {
		if left[index] != right[index] {
			return false
		}
	}
	return true
}

func contains(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}

func findDiagnostic(diagnostics []Diagnostic, name string) *Diagnostic {
	for index := range diagnostics {
		if diagnostics[index].Name == name {
			return &diagnostics[index]
		}
	}
	return nil
}
