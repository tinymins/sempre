package tunnel

import (
	"slices"
	"strings"
	"testing"
)

func TestConfigValidatesPoolAndBuildsMultipleForwards(t *testing.T) {
	config := validConfig()
	config.Instances[0].Forwards = append(config.Instances[0].Forwards, Forward{ID: "wg-backup", Name: "Backup", ListenPort: 52002, RemoteHost: "::1", RemotePort: 31089})
	if err := config.Validate(); err != nil {
		t.Fatal(err)
	}
	args := BuildArgs(config.Instances[0])
	if !slices.Contains(args, "--tls-verify-certificate") || !slices.Contains(args, "dns://192.0.2.53:53") {
		t.Fatalf("args = %#v", args)
	}
	joined := strings.Join(args, " ")
	if !strings.Contains(joined, "udp://127.0.0.1:52001:127.0.0.1:31088?timeout_sec=0") || !strings.Contains(joined, "udp://127.0.0.1:52002:[::1]:31089?timeout_sec=0") {
		t.Fatalf("forward args = %s", joined)
	}
}

func TestConfigRejectsUnsafeOrAmbiguousInputs(t *testing.T) {
	tests := map[string]func(*Config){
		"cleartext server": func(config *Config) { config.Instances[0].ServerURL = "ws://example.com" },
		"duplicate port": func(config *Config) {
			config.Instances = append(config.Instances, Instance{ID: "sh", Name: "Shanghai", DesiredState: DesiredRunning, ServerURL: "wss://sh.example.com", WebsocketPing: "15s", ConnectionRetryMaxBackoff: "30s", Forwards: []Forward{{ID: "sh-wg", Name: "WG", ListenPort: 52001, RemoteHost: "127.0.0.1", RemotePort: 31088}}})
		},
		"invalid resolver": func(config *Config) { config.Instances[0].DNSResolvers = []string{"https://1.1.1.1"} },
		"host with port":   func(config *Config) { config.Instances[0].Forwards[0].RemoteHost = "127.0.0.1:31088" },
	}
	for name, mutate := range tests {
		t.Run(name, func(t *testing.T) {
			config := validConfig()
			mutate(&config)
			if err := config.Validate(); err == nil {
				t.Fatal("expected validation failure")
			}
		})
	}
}

func TestPackageForSupportedTargets(t *testing.T) {
	for _, target := range [][2]string{{"windows", "amd64"}, {"windows", "arm64"}, {"linux", "amd64"}, {"linux", "arm64"}, {"darwin", "amd64"}, {"darwin", "arm64"}} {
		item, err := PackageFor(target[0], target[1])
		if err != nil || item.Size <= 0 || !strings.HasPrefix(item.Digest, "sha256:") {
			t.Fatalf("%s/%s package = %#v, %v", target[0], target[1], item, err)
		}
	}
}

func validConfig() Config {
	return Config{Schema: SchemaVersion, Instances: []Instance{{ID: "hz", Name: "Hangzhou", DesiredState: DesiredRunning, ServerURL: "wss://hz.example.com:443", DNSResolvers: []string{"dns://192.0.2.53:53"}, PreferIPv4: true, WebsocketPing: "15s", ConnectionRetryMaxBackoff: "30s", Forwards: []Forward{{ID: "hz-wg", Name: "WG", ListenPort: 52001, RemoteHost: "127.0.0.1", RemotePort: 31088}}}}}
}
