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

func TestFiltersDoNotRemoveManualServers(t *testing.T) {
	profile := NewProfile("manual")
	profile.UseSystemFilters = false
	profile.Editor.Filter = `["blocked"]`
	profile.Editor.Servers = `[{"name":"blocked manual","type":"socks5","server":"127.0.0.1","port":1080}]`
	if err := ApplyEditorConfig(&profile); err != nil {
		t.Fatal(err)
	}
	nodes, _, _, _, _, err := (&Compiler{}).collectNodes(context.Background(), profile, Catalog{}, false)
	if err != nil {
		t.Fatal(err)
	}
	if len(nodes) != 1 || nodes[0].Name != "blocked manual" {
		t.Fatalf("manual nodes = %#v", nodes)
	}
}
