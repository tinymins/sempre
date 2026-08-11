package subscription

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/layout"
	"gopkg.in/yaml.v3"
)

func TestResolveSingBoxTargetFallback(t *testing.T) {
	target, warnings := ResolveSingBoxTarget("1.12.9", "windows")
	if target.Format != "sing-box-v12-windows" || len(warnings) != 0 {
		t.Fatalf("unexpected target: %#v %#v", target, warnings)
	}
	target, warnings = ResolveSingBoxTarget("2.0.0", "linux")
	if target.Version != "13" || len(warnings) == 0 {
		t.Fatalf("unknown major did not fall back: %#v %#v", target, warnings)
	}
	target, warnings = ResolveSingBoxTarget("1.14.0-beta.13", "macos")
	if target.Format != "sing-box-v14-macos" || len(warnings) != 0 {
		t.Fatalf("v14 target: %#v %#v", target, warnings)
	}
}

func TestMacOSSingBoxCompatibilityModes(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{"fakeipEnabled": true}}
	tests := []struct {
		format             string
		wantTUN            bool
		wantLegacyOverride bool
		wantTUNDNSMode     string
		wantFakeIP         bool
		binaryEnvironment  string
	}{
		{format: "sing-box-macos", wantTUN: true, wantLegacyOverride: true, binaryEnvironment: "SEMPRE_TEST_SING_BOX_V11"},
		{format: "sing-box-v12-macos", wantTUN: true, wantLegacyOverride: true, binaryEnvironment: "SEMPRE_TEST_SING_BOX_V12"},
		{format: "sing-box-v13-macos", wantTUN: true, binaryEnvironment: "SEMPRE_TEST_SING_BOX_V13"},
		{format: "sing-box-v14-macos", wantTUN: true, wantTUNDNSMode: "hijack", binaryEnvironment: "SEMPRE_TEST_SING_BOX_V14"},
	}
	for _, test := range tests {
		t.Run(test.format, func(t *testing.T) {
			result, updated, err := compiler.Render(context.Background(), profile, catalog, Target{Format: test.format}, false)
			if err != nil {
				t.Fatal(err)
			}
			var config map[string]any
			if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
				t.Fatal(err)
			}
			inbounds := config["inbounds"].([]any)
			tun := optionalMapByKey(inbounds, "tag", "tun-in")
			if (tun != nil) != test.wantTUN {
				t.Fatalf("TUN inbound = %#v", tun)
			}
			if tun != nil {
				if (tun["sniff_override_destination"] == true) != test.wantLegacyOverride || stringValue(tun["dns_mode"]) != test.wantTUNDNSMode {
					t.Fatalf("TUN compatibility = %#v", tun)
				}
			}
			httpInbound := mapByKey(t, inbounds, "tag", "sempre-http-in")
			if _, exists := httpInbound["set_system_proxy"]; exists {
				t.Fatalf("HTTP inbound = %#v", httpInbound)
			}
			if singBoxConfigHasFakeIP(config) != test.wantFakeIP {
				t.Fatalf("FakeIP config = %#v", config["dns"])
			}
			if !containsWarning(result.Warnings, "FakeIP is unavailable") {
				t.Fatalf("missing FakeIP compatibility warning: %#v", result.Warnings)
			}
			if test.format == "sing-box-v14-macos" {
				dnsRules := config["dns"].(map[string]any)["rules"].([]any)
				if optionalMapByKey(dnsRules, "action", "evaluate") == nil || optionalMapByKey(dnsRules, "action", "respond")["match_response"] != true {
					t.Fatalf("v14 response matching rules = %#v", dnsRules)
				}
			}
			shared := updated.DNS["shared"].(map[string]any)
			if shared["fakeipEnabled"] != true {
				t.Fatalf("stored FakeIP preference changed: %#v", updated.DNS)
			}
			if test.format == "sing-box-v13-macos" {
				rules := config["route"].(map[string]any)["rules"].([]any)
				if !reflect.DeepEqual(rules[0], map[string]any{"action": "sniff"}) ||
					!reflect.DeepEqual(rules[1], map[string]any{"protocol": "dns", "action": "hijack-dns"}) {
					t.Fatalf("v13 TUN route actions = %#v", rules[:2])
				}
			}
			if binary := os.Getenv(test.binaryEnvironment); binary != "" {
				configPath := filepath.Join(t.TempDir(), "config.json")
				if err := os.WriteFile(configPath, []byte(result.Content), 0o600); err != nil {
					t.Fatal(err)
				}
				dataDirectory := filepath.Join(t.TempDir(), "data")
				if err := os.MkdirAll(dataDirectory, 0o700); err != nil {
					t.Fatal(err)
				}
				output, err := exec.Command(binary, "check", "-c", configPath, "-D", dataDirectory, "--disable-color").CombinedOutput()
				if err != nil {
					t.Fatalf("%s rejected config: %v\n%s", binary, err, output)
				}
			}
		})
	}
}

func TestMacOSSingBoxNativeDNSOverrideDropsFakeIP(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{
		"shared": map[string]any{"fakeipEnabled": true},
		"modes":  map[string]any{"sing_box_v12": "native"},
		"overrides": map[string]any{"sing_box_v12": map[string]any{
			"servers": []any{
				map[string]any{"tag": "local", "type": "local"},
				map[string]any{"tag": "fakeip", "type": "fakeip"},
				map[string]any{"tag": "remote", "type": "tls", "server": "8.8.8.8"},
			},
			"rules": []any{
				map[string]any{"server": "local", "domain_suffix": []any{".cn"}},
				map[string]any{"server": "fakeip", "query_type": []any{"A", "AAAA"}},
			},
			"fakeip": map[string]any{"enabled": true},
		}},
	}
	profile.CoreOverrides["sing-box"] = map[string]any{"dns": map[string]any{
		"servers": []any{
			map[string]any{"tag": "local", "type": "local"},
			map[string]any{"tag": "fakeip", "type": "fakeip"},
			map[string]any{"tag": "remote", "type": "tls", "server": "8.8.8.8"},
		},
		"rules": []any{
			map[string]any{"server": "local", "domain_suffix": []any{".cn"}},
			map[string]any{"server": "fakeip", "query_type": []any{"A", "AAAA"}},
		},
		"fakeip": map[string]any{"enabled": true},
	}}
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v12-macos"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	if singBoxConfigHasFakeIP(config) {
		t.Fatalf("native DNS override retained FakeIP: %#v", config["dns"])
	}
	dns := config["dns"].(map[string]any)
	if optionalMapByKey(dns["servers"].([]any), "tag", "remote") == nil || optionalMapByKey(dns["rules"].([]any), "server", "local") == nil {
		t.Fatalf("native DNS override lost non-FakeIP entries: %#v", dns)
	}
}

func optionalMapByKey(values []any, key, expected string) map[string]any {
	for _, raw := range values {
		value, ok := raw.(map[string]any)
		if ok && value[key] == expected {
			return value
		}
	}
	return nil
}

func singBoxConfigHasFakeIP(config map[string]any) bool {
	dns := config["dns"].(map[string]any)
	if _, exists := dns["fakeip"]; exists {
		return true
	}
	servers, _ := dns["servers"].([]any)
	for _, raw := range servers {
		server, ok := raw.(map[string]any)
		if ok && (server["type"] == "fakeip" || server["address"] == "fakeip") {
			return true
		}
	}
	return false
}

func containsWarning(warnings []string, part string) bool {
	for _, warning := range warnings {
		if strings.Contains(warning, part) {
			return true
		}
	}
	return false
}

func TestCompilerRendersRawSourceWithoutNetwork(t *testing.T) {
	paths := layout.At(filepath.Join(t.TempDir(), "root"))
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	store := NewStore(paths)
	if err := store.Initialize(""); err != nil {
		t.Fatal(err)
	}
	catalog, _ := store.Read()
	profile := catalog.Profiles[0]
	profile.UseSystemGroups = false
	profile.UseSystemRules = false
	profile.UseSystemFilters = false
	profile.UseSystemDNS = false
	profile.UseSystemCustomConfig = false
	profile.Sources = []Source{{ID: NewID(), Type: SourceRaw, Enabled: true, Content: "proxies:\n- name: edge\n  type: ss\n  server: example.com\n  port: 443\n  cipher: aes-128-gcm\n  password: secret\n"}}
	result, _, err := NewCompiler(store).Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	if result.NodeCount != 1 || result.Content == "" {
		t.Fatalf("unexpected render: %#v", result)
	}
	if _, err := os.Stat(filepath.Join(paths.SubscriptionBlobs, result.SourceResults[0].ContentHash)); err != nil {
		t.Fatal(err)
	}
}

func TestSystemDefaultsDriveClashCompilation(t *testing.T) {
	profile := EffectiveProfile(NewProfile(""))
	content, err := buildClash(profile, []Proxy{{Name: "edge", Type: "socks5", Server: "edge.example.com", Port: 1080, Extra: map[string]any{}}}, true, "")
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := yaml.Unmarshal([]byte(content), &config); err != nil {
		t.Fatal(err)
	}
	providers, ok := config["rule-providers"].(map[string]any)
	if !ok || len(providers) != len(SystemDefaults().RuleProviders) {
		t.Fatalf("rule providers = %#v", config["rule-providers"])
	}
	groups, ok := config["proxy-groups"].([]any)
	if !ok || len(groups) != len(SystemDefaults().Groups) {
		t.Fatalf("proxy groups = %#v", config["proxy-groups"])
	}
	if config["unified-delay"] != true {
		t.Fatalf("clash-meta options = %#v", config)
	}
	if _, exists := config["global-client-fingerprint"]; exists {
		t.Fatalf("removed Mihomo option remains: %#v", config)
	}
}

func TestSystemDefaultForeignSelectorPrefersFirstProxy(t *testing.T) {
	paths := layout.At(filepath.Join(t.TempDir(), "root"))
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	store := NewStore(paths)
	if err := store.Initialize(""); err != nil {
		t.Fatal(err)
	}
	catalog, _ := store.Read()
	profile := EffectiveProfile(catalog.Profiles[0])
	profile.UseSystemRules = false
	profile.UseSystemFilters = false
	profile.UseSystemDNS = false
	profile.UseSystemCustomConfig = false
	profile.Sources = []Source{{ID: NewID(), Type: SourceRaw, Enabled: true, Content: "proxies:\n- name: edge\n  type: socks5\n  server: edge.example.com\n  port: 1080\n"}}

	result, _, err := NewCompiler(store).Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	foreign := mapByKey(t, config["outbounds"].([]any), "tag", "🔰 国外流量")
	if foreign["default"] != "edge" {
		t.Fatalf("foreign selector default = %#v", foreign)
	}
}

func TestSingBoxEmbedsFetchedRuleProvider(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path == "/rules" {
			_, _ = writer.Write([]byte("payload:\n  - DOMAIN-SUFFIX,example.com\n"))
			return
		}
		http.NotFound(writer, request)
	}))
	defer server.Close()
	paths := layout.At(filepath.Join(t.TempDir(), "root"))
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	store := NewStore(paths)
	if err := store.Initialize(""); err != nil {
		t.Fatal(err)
	}
	catalog, _ := store.Read()
	profile := catalog.Profiles[0]
	profile.UseSystemGroups = false
	profile.UseSystemRules = false
	profile.UseSystemFilters = false
	profile.UseSystemDNS = false
	profile.UseSystemCustomConfig = false
	profile.Sources = []Source{{ID: NewID(), Type: SourceRaw, Enabled: true, Content: "proxies:\n- name: edge\n  type: socks5\n  server: edge.example.com\n  port: 1080\n"}}
	profile.RuleProviders = []RuleProvider{{Tag: "example", URL: server.URL + "/rules", Outbound: "proxy"}}
	result, _, err := NewCompiler(store).Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, true)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	route, _ := config["route"].(map[string]any)
	ruleSets, _ := route["rule_set"].([]any)
	found := false
	for _, item := range ruleSets {
		ruleSet, _ := item.(map[string]any)
		if ruleSet["tag"] == "example" && ruleSet["type"] == "inline" {
			found = true
		}
	}
	if !found {
		t.Fatalf("inline provider missing: %#v", ruleSets)
	}
}

func TestRuleProviderConversionSupportsToolboxRuleForms(t *testing.T) {
	rules, diagnostics, err := parseRuleProvider([]byte("- example.com\n- HOST-SUFFIX,example.net\n- IP-CIDR6,2001:db8::/32\n- DST-PORT,443\n- SRC-PORT,invalid\n- PROCESS-NAME,client\n"))
	if err != nil {
		t.Fatal(err)
	}
	if len(rules) != 1 || len(diagnostics) != 1 {
		t.Fatalf("rules = %#v, diagnostics = %#v", rules, diagnostics)
	}
	grouped, ok := rules[0].(map[string]any)
	if !ok {
		t.Fatalf("grouped rule = %#v", rules[0])
	}
	if len(grouped["domain"].([]string)) != 1 || len(grouped["domain_suffix"].([]string)) != 1 || len(grouped["ip_cidr"].([]string)) != 1 || grouped["port"].([]int)[0] != 443 {
		t.Fatalf("grouped rule = %#v", grouped)
	}
}

func TestSingBoxFieldOriginsTraceConversions(t *testing.T) {
	proxy := Proxy{Name: "edge", Type: "vless", Server: "edge.example.com", Port: 443, Extra: map[string]any{
		"uuid": "id", "tls": true, "servername": "origin.example.com", "client-fingerprint": "chrome",
		"ws-opts": map[string]any{"path": "/socket"},
	}}
	outbound, diff, ok := ConvertProxy(proxy)
	if !ok {
		t.Fatal("vless conversion failed")
	}
	if diff.Outbound["tag"] != outbound["tag"] {
		t.Fatalf("trace outbound = %#v", diff.Outbound)
	}
	checks := map[string]string{
		"tag": "name", "server_port": "port", "uuid": "uuid", "tls.server_name": "servername", "tls.utls.fingerprint": "client-fingerprint", "transport.path": "ws-opts.path",
	}
	for output, source := range checks {
		if diff.FieldOrigins[output].SourceKey != source {
			t.Fatalf("origin %q = %#v", output, diff.FieldOrigins[output])
		}
	}
}

func TestSingBoxV11CountsOnlyRepresentableNodes(t *testing.T) {
	paths := layout.At(filepath.Join(t.TempDir(), "root"))
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	store := NewStore(paths)
	if err := store.Initialize(""); err != nil {
		t.Fatal(err)
	}
	catalog, _ := store.Read()
	profile := catalog.Profiles[0]
	profile.UseSystemGroups = false
	profile.UseSystemRules = false
	profile.UseSystemFilters = false
	profile.UseSystemDNS = false
	profile.UseSystemCustomConfig = false
	profile.Sources = []Source{{ID: NewID(), Type: SourceRaw, Enabled: true, Content: "proxies:\n- name: edge\n  type: socks5\n  server: edge.example.com\n  port: 1080\n- name: modern\n  type: anytls\n  server: modern.example.com\n  port: 443\n  password: secret\n"}}
	result, _, err := NewCompiler(store).Render(context.Background(), profile, catalog, Target{Format: "sing-box"}, false)
	if err != nil {
		t.Fatal(err)
	}
	if result.NodeCount != 1 || len(result.FieldDiffs) != 2 {
		t.Fatalf("result = %#v", result)
	}
}
