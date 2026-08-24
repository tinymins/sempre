package subscription

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/layout"
)

func TestApplyEditorConfigParsesJSONCAndPreservesSource(t *testing.T) {
	profile := NewProfile("editor")
	profile.UseSystemGroups = false
	profile.UseSystemRules = false
	profile.UseSystemFilters = false
	profile.UseSystemDNS = false
	profile.UseSystemCustomConfig = false
	profile.Editor = EditorConfig{
		RuleList: `{
  // The URL contains // and must survive comment removal.
  "proxy": [{"name": "example", "url": "https://example.com/rules", "type": "domain",}],
}`,
		Group:  `[{"name":"proxy","type":"select","proxies":["DIRECT"],}]`,
		Filter: `["expired",]`,
		CustomConfig: `[
  "DOMAIN-SUFFIX,example.com,proxy",
]`,
		DNSConfig:           `{"shared":{"localDns":"127.0.0.1",},}`,
		PrivateAccessConfig: `{"enabled":true,"connectors":[],}`,
		Servers: `[
  {"name":"日本手动","type":"socks5","server":"127.0.0.1","port":1080,},
  "name: US YAML\ntype: http\nserver: 127.0.0.2\nport: 8080",
]`,
	}
	originalRuleList := profile.Editor.RuleList
	if err := ApplyEditorConfig(&profile); err != nil {
		t.Fatal(err)
	}
	if profile.Editor.RuleList != originalRuleList {
		t.Fatal("applying editor config changed the raw JSONC source")
	}
	if len(profile.RuleProviders) != 1 || profile.RuleProviders[0].URL != "https://example.com/rules" || profile.RuleProviders[0].Behavior != "domain" {
		t.Fatalf("rule providers = %#v", profile.RuleProviders)
	}
	if len(profile.Groups) != 1 || profile.Groups[0].Name != "proxy" || len(profile.Filters) != 1 || len(profile.Rules) != 1 {
		t.Fatalf("derived editor fields = groups %#v, filters %#v, rules %#v", profile.Groups, profile.Filters, profile.Rules)
	}
	servers, err := ManualServers(profile)
	if err != nil {
		t.Fatal(err)
	}
	if len(servers) != 2 || servers[0].Name != "日本手动" || servers[1].Name != "US YAML" {
		t.Fatalf("manual servers = %#v", servers)
	}
}

func TestApplyEditorConfigRejectsInvalidJSONC(t *testing.T) {
	profile := NewProfile("invalid")
	profile.Editor.Group = `[{"name":"broken"}`
	if err := ApplyEditorConfig(&profile); err == nil || !strings.Contains(err.Error(), "group JSONC") {
		t.Fatalf("error = %v", err)
	}
	profile = NewProfile("invalid-log")
	profile.LogLevel = "verbose"
	if err := ApplyEditorConfig(&profile); err == nil || !strings.Contains(err.Error(), "log level") {
		t.Fatalf("error = %v", err)
	}
}

func TestStoreReadsSchemaOneProfilesWithEditorConfig(t *testing.T) {
	paths := layout.At(filepath.Join(t.TempDir(), "root"))
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	profile := NewProfile("legacy")
	profile.Revision = 0
	profile.Editor = EditorConfig{}
	profile.Groups = []ProxyGroup{{Name: "proxy", Type: "select", Proxies: []string{"DIRECT"}}}
	profile.Filters = []string{"expired"}
	profile.Rules = []string{"DOMAIN-SUFFIX,example.com,proxy"}
	profile.RuleProviders = []RuleProvider{{Tag: "example", URL: "https://example.com/rules", Outbound: "proxy"}}
	catalog := Catalog{Schema: 1, Profiles: []Profile{profile}, CustomNodes: []CustomNode{}}
	data, err := json.Marshal(catalog)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(paths.SubscriptionStore, data, 0o600); err != nil {
		t.Fatal(err)
	}
	loaded, err := NewStore(paths).Read()
	if err != nil {
		t.Fatal(err)
	}
	if loaded.Schema != CatalogSchema {
		t.Fatalf("schema = %d", loaded.Schema)
	}
	migrated := loaded.Profiles[0]
	if migrated.Revision != 1 {
		t.Fatalf("revision = %d", migrated.Revision)
	}
	if migrated.Editor.Group == "" || migrated.Editor.RuleList == "" || migrated.Editor.Filter == "" || migrated.Editor.CustomConfig == "" || migrated.Editor.Servers != "[]" {
		t.Fatalf("editor config = %#v", migrated.Editor)
	}
	if len(migrated.Groups) != 1 || len(migrated.RuleProviders) != 1 || len(migrated.Filters) != 1 || len(migrated.Rules) != 1 {
		t.Fatalf("typed fields changed during migration: %#v", migrated)
	}
}

func TestStoreMigratesOpenCoreOverridesAndRemovesDuplicateDNSFields(t *testing.T) {
	paths := layout.At(filepath.Join(t.TempDir(), "root"))
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	profile := NewProfile("legacy")
	profile.CoreOverrides["future-core"] = map[string]any{"future": true}
	profile.DNS = map[string]any{
		"shared":    map[string]any{"tproxyPort": 17893, "dnsListenPort": 11053, "clashApiSecret": "dns-secret"},
		"overrides": map[string]any{"singboxV12": map[string]any{"final": "remote"}, "future-core": map[string]any{"native": true}},
	}
	profileDocument := map[string]any{}
	profileData, err := json.Marshal(profile)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(profileData, &profileDocument); err != nil {
		t.Fatal(err)
	}
	delete(profileDocument, "management_api")
	profileDocument["custom_config"] = map[string]any{"route": map[string]any{"final": "proxy"}}
	profileDocument["clash_api"] = ManagementAPIConfig{ExternalController: "127.0.0.1:9090", Secret: "legacy-secret", AllowOrigins: []string{}}
	profileDocument["transparent_proxy"] = map[string]any{
		"mode": "tun-router",
		"tun": map[string]any{
			"interface_name": "sing-box", "address": "172.30.0.1/30",
			"route_exclude_address": []string{"10.10.10.0/24"}, "interface_mode": "exclude",
			"interfaces": []string{"docker0"}, "auto_exclude_local_routes": true, "auto_exclude_vpn_routes": true,
		},
		"tproxy": map[string]any{
			"listen_port": 7893, "dns_listen_port": 1053, "capture_host": true, "lan_interfaces": []string{"vmbr1"},
		},
	}
	legacyCatalog := map[string]any{"schema": 4, "profiles": []any{profileDocument}, "custom_nodes": []any{}}
	data, err := json.Marshal(legacyCatalog)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(paths.SubscriptionStore, data, 0o600); err != nil {
		t.Fatal(err)
	}
	loaded, err := NewStore(paths).Read()
	if err != nil {
		t.Fatal(err)
	}
	migrated := loaded.Profiles[0]
	if migrated.CoreOverrides["sing-box"]["route"] == nil || migrated.CoreOverrides["future-core"]["future"] != true {
		t.Fatalf("core overrides = %#v", migrated.CoreOverrides)
	}
	if migrated.ManagementAPI.Secret != "legacy-secret" {
		t.Fatalf("management migration = %#v", migrated)
	}
	encoded, err := json.Marshal(migrated)
	if err != nil {
		t.Fatal(err)
	}
	profileKeys := map[string]any{}
	if err := json.Unmarshal(encoded, &profileKeys); err != nil {
		t.Fatal(err)
	}
	if _, exists := profileKeys["custom_config"]; exists {
		t.Fatalf("removed custom_config remains in migrated profile: %s", encoded)
	}
	if _, exists := profileKeys["clash_api"]; exists {
		t.Fatalf("removed fields remain in migrated profile: %s", encoded)
	}
	for _, field := range []string{"local_proxy", "management_api"} {
		if _, exists := profileKeys[field].(map[string]any)["enabled"]; exists {
			t.Fatalf("removed %s.enabled remains in migrated profile: %s", field, encoded)
		}
	}
	if migrated.TransparentProxy.TProxy.ListenPort != 17893 || migrated.TransparentProxy.TProxy.DNSListenPort != 11053 {
		t.Fatalf("transparent proxy migration = %#v", migrated.TransparentProxy)
	}
	if migrated.TransparentProxy.TUN.InterfaceName != "sempre-tun" || !migrated.TransparentProxy.CaptureHost || migrated.TransparentProxy.InterfaceMode != "exclude" || len(migrated.TransparentProxy.RouteExclusions) != 1 || len(migrated.TransparentProxy.LANInterfaces) != 1 {
		t.Fatalf("flattened transparent runtime intent = %#v", migrated.TransparentProxy)
	}
	if migrated.LocalProxy.SOCKSPort != 1080 || migrated.LocalProxy.HTTPPort != 1081 || migrated.LocalProxy.Username != "sempre" || len(migrated.LocalProxy.Password) < 40 {
		t.Fatalf("authenticated local proxy migration = %#v", migrated.LocalProxy)
	}
	shared := migrated.DNS["shared"].(map[string]any)
	for _, key := range []string{"tproxyPort", "dnsListenPort", "clashApiSecret"} {
		if _, exists := shared[key]; exists {
			t.Fatalf("deprecated DNS field %q survived: %#v", key, shared)
		}
	}
	overrides := migrated.DNS["overrides"].(map[string]any)
	if overrides["sing_box_v12"] == nil || overrides["future-core"] == nil {
		t.Fatalf("DNS overrides = %#v", overrides)
	}
}

func TestPreviewReturnsFilteredNodesWithToolboxNameEnrichment(t *testing.T) {
	paths := layout.At(filepath.Join(t.TempDir(), "root"))
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	store := NewStore(paths)
	if err := store.Initialize(""); err != nil {
		t.Fatal(err)
	}
	catalog, err := store.Read()
	if err != nil {
		t.Fatal(err)
	}
	profile := catalog.Profiles[0]
	profile.UseSystemFilters = false
	profile.Editor.Filter = `["日本"]`
	profile.Sources = []Source{{
		ID: NewID(), Type: SourceRaw, Enabled: true,
		Content: "proxies:\n- name: 日本节点\n  type: socks5\n  server: edge.example.com\n  port: 1080\n",
	}}
	if err := ApplyEditorConfig(&profile); err != nil {
		t.Fatal(err)
	}
	nodes, err := NewCompiler(store).PreviewNodes(context.Background(), profile, catalog, true)
	if err != nil {
		t.Fatal(err)
	}
	if len(nodes) != 1 || !nodes[0].Filtered || nodes[0].FilteredBy != "日本" || nodes[0].Name != "🇯🇵 日本节点" {
		t.Fatalf("preview nodes = %#v", nodes)
	}
	trace, err := NewCompiler(store).TraceNode(context.Background(), profile, catalog, nodes[0].Name, "clash-meta")
	if err != nil {
		t.Fatal(err)
	}
	steps := trace["steps"].([]any)
	enrich := steps[3].(map[string]any)["data"].(map[string]any)
	if enrich["originalName"] != "日本节点" || enrich["enrichedName"] != "🇯🇵 日本节点" {
		t.Fatalf("enrich step = %#v", enrich)
	}
}

func TestPreviewLocalNodesIncludesManualAndSelectedCustomNodes(t *testing.T) {
	profile := NewProfile("local")
	profile.Editor.Servers = `[{"name":"日本手动","type":"socks5","server":"127.0.0.1","port":1080}]`
	profile.CustomNodeIDs = []string{"custom-selected"}
	catalog := Catalog{CustomNodes: []CustomNode{
		{ID: "custom-unselected", Name: "unused", Proxy: map[string]any{"name": "未选择", "type": "http", "server": "127.0.0.2", "port": 8080}},
		{ID: "custom-selected", Name: "selected", Proxy: map[string]any{"name": "美国自定义", "type": "http", "server": "127.0.0.3", "port": 8081}},
	}}

	nodes, err := PreviewLocalNodes(profile, catalog)
	if err != nil {
		t.Fatal(err)
	}
	if len(nodes) != 2 {
		t.Fatalf("local nodes = %#v", nodes)
	}
	if nodes[0].Name != "🇯🇵 日本手动" || nodes[0].SourceIndex != 0 || nodes[0].SourceURL != "manual" {
		t.Fatalf("manual node = %#v", nodes[0])
	}
	if nodes[1].Name != "🇺🇸 美国自定义" || nodes[1].SourceIndex != 0 || nodes[1].SourceURL != "custom-node:custom-selected" {
		t.Fatalf("custom node = %#v", nodes[1])
	}
}

func TestFiltersDoNotRemoveManualServers(t *testing.T) {
	profile := NewProfile("manual")
	profile.UseSystemFilters = false
	profile.Editor.Filter = `["blocked"]`
	profile.Editor.Servers = `[{"name":"blocked manual","type":"socks5","server":"127.0.0.1","port":1080}]`
	if err := ApplyEditorConfig(&profile); err != nil {
		t.Fatal(err)
	}
	nodes, _, _, _, _, err := (&Compiler{}).collectNodes(context.Background(), profile, Catalog{}, false, false)
	if err != nil {
		t.Fatal(err)
	}
	if len(nodes) != 1 || nodes[0].Name != "blocked manual" {
		t.Fatalf("manual nodes = %#v", nodes)
	}
}
