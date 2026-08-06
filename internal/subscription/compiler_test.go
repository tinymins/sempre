package subscription

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
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

func TestLinuxSingBoxTUNAndRemoteDNS(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{
		"fakeipEnabled": false,
		"preferIpv4":    true,
		"remoteDetour":  "foreign",
	}}
	profile.Groups = []ProxyGroup{{Name: "foreign", Type: "select", IncludeAll: true, Default: "edge"}}
	profile.TransparentProxy.TUN.Address = "172.30.0.1/30"
	profile.TransparentProxy.RouteExclusions = []string{"10.10.10.0/24"}

	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	inbounds := config["inbounds"].([]any)
	tun := mapByKey(t, inbounds, "tag", "tun-in")
	if tun["type"] != "tun" || tun["interface_name"] != "sempre-tun" || tun["auto_route"] != true || tun["auto_redirect"] != true || tun["strict_route"] != true || tun["stack"] != "system" {
		t.Fatalf("TUN inbound = %#v", tun)
	}
	if tun["address"].([]any)[0] != "172.30.0.1/30" || tun["route_exclude_address"].([]any)[0] != "10.10.10.0/24" {
		t.Fatalf("TUN address configuration = %#v", tun)
	}
	dns := config["dns"].(map[string]any)
	if dns["final"] != "remote" || dns["strategy"] != "prefer_ipv4" {
		t.Fatalf("DNS defaults = %#v", dns)
	}
	servers := dns["servers"].([]any)
	remote := servers[len(servers)-1].(map[string]any)
	if remote["tag"] != "remote" || remote["detour"] != "foreign" {
		t.Fatalf("remote DNS = %#v", remote)
	}
	route := config["route"].(map[string]any)
	if route["auto_detect_interface"] != true {
		t.Fatalf("route = %#v", route)
	}
	foundDefault := false
	for _, value := range config["outbounds"].([]any) {
		outbound := value.(map[string]any)
		if outbound["tag"] == "foreign" && outbound["default"] == "edge" {
			foundDefault = true
		}
	}
	if !foundDefault {
		t.Fatalf("foreign selector does not persist edge as its default: %#v", config["outbounds"])
	}
}

func TestSingBoxSelectorDefaultMustResolveToFinalMember(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.Groups = []ProxyGroup{{Name: "foreign", Type: "select", IncludeAll: true, Default: "missing"}}
	_, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err == nil {
		t.Fatal("expected unavailable selector default to fail compilation")
	}
}

func TestCatalogAllowsDynamicSelectorDefault(t *testing.T) {
	catalog := NewCatalog("")
	profile := &catalog.Profiles[0]
	profile.Groups = []ProxyGroup{{Name: "foreign", Type: "select", IncludeAll: true, Default: "subscription-node"}}
	if err := ValidateCatalog(catalog); err != nil {
		t.Fatal(err)
	}
}

func TestLinuxSingBoxTProxyAndDisabledModes(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.TransparentProxy.Mode = TransparentProxyTProxy
	profile.TransparentProxy.TProxy.ListenPort = 17893
	profile.TransparentProxy.TProxy.DNSListenPort = 11053
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	inbounds := config["inbounds"].([]any)
	if len(inbounds) != 4 || mapByKey(t, inbounds, "tag", "dns-in")["listen_port"] != float64(11053) || mapByKey(t, inbounds, "tag", "tproxy-in")["listen_port"] != float64(17893) {
		t.Fatalf("TProxy inbounds = %#v", inbounds)
	}
	assertLocalSingBoxInbounds(t, inbounds, profile.LocalProxy)

	profile.TransparentProxy.Mode = TransparentProxyDisabled
	profile.LocalProxy.Enabled = false
	profile.CoreOverrides["sing-box"] = map[string]any{"inbounds": []any{map[string]any{"type": "mixed", "tag": "manual", "listen": "127.0.0.1", "listen_port": 1080}}}
	result, _, err = compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	if config["inbounds"].([]any)[0].(map[string]any)["tag"] != "manual" {
		t.Fatalf("disabled/manual inbounds = %#v", config["inbounds"])
	}
}

func TestLinuxManagedInboundOverrideIsRejected(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.CoreOverrides["sing-box"] = map[string]any{"inbounds": []any{}}
	_, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err == nil {
		t.Fatal("managed inbound override was accepted")
	}
}

func TestNativeManagementControllerOverridesAreRejected(t *testing.T) {
	tests := []struct {
		name     string
		coreID   string
		target   Target
		override map[string]any
	}{
		{
			name: "sing-box", coreID: "sing-box", target: Target{Format: "sing-box-v13"},
			override: map[string]any{"experimental": map[string]any{"clash_api": map[string]any{"external_controller": "0.0.0.0:9090"}}},
		},
		{
			name: "mihomo", coreID: "mihomo", target: Target{Core: "mihomo", Format: "clash-meta"},
			override: map[string]any{"external-controller": "0.0.0.0:9090"},
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			profile, catalog, compiler := compilerFixture(t)
			profile.ManagementAPI.Enabled = false
			profile.CoreOverrides[test.coreID] = test.override
			if _, _, err := compiler.Render(context.Background(), profile, catalog, test.target, false); err == nil {
				t.Fatal("native controller override was silently accepted")
			}
		})
	}
}

func TestMihomoManagedDNSAndTransparentModes(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{
		"fakeipEnabled": false, "localDns": "127.0.0.1", "remoteDns": "8.8.4.4", "remoteServerName": "ignored.example", "remoteDetour": "proxy",
	}}
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Core: "mihomo", Format: "clash-meta"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := yaml.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	tun := config["tun"].(map[string]any)
	if tun["enable"] != true || tun["auto-route"] != true || tun["auto-redirect"] != true || tun["strict-route"] != true || tun["device"] != "sempre-tun" {
		t.Fatalf("Mihomo TUN = %#v", tun)
	}
	dns := config["dns"].(map[string]any)
	if dns["respect-rules"] != true || dns["enhanced-mode"] != "redir-host" {
		t.Fatalf("Mihomo DNS = %#v", dns)
	}
	nameservers := dns["nameserver"].([]any)
	if len(nameservers) != 1 || nameservers[0] != "tls://8.8.4.4:853#proxy&disable-qtype-65=true" {
		t.Fatalf("Mihomo remote DNS = %#v", nameservers)
	}
	if _, exists := dns["tproxyPort"]; exists {
		t.Fatalf("deprecated runtime field leaked into DNS: %#v", dns)
	}

	profile.TransparentProxy.Mode = TransparentProxyTProxy
	profile.TransparentProxy.TProxy.ListenPort = 17893
	profile.TransparentProxy.TProxy.DNSListenPort = 11053
	result, _, err = compiler.Render(context.Background(), profile, catalog, Target{Core: "mihomo", Format: "clash-meta"}, false)
	if err != nil {
		t.Fatal(err)
	}
	if err := yaml.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	if config["tproxy-port"] != 17893 {
		t.Fatalf("Mihomo TProxy port = %#v", config["tproxy-port"])
	}
	listeners := config["listeners"].([]any)
	listener := mapByKey(t, listeners, "name", "sempre-dns-in")
	if listener["type"] != "tproxy" || listener["port"] != 11053 {
		t.Fatalf("Mihomo DNS listener = %#v", listeners)
	}
	for _, name := range []string{"sempre-socks-in", "sempre-http-in"} {
		local := mapByKey(t, listeners, "name", name)
		if local["listen"] != "127.0.0.1" || len(local["users"].([]any)) != 1 {
			t.Fatalf("Mihomo local proxy listener = %#v", local)
		}
	}
	rules := config["rules"].([]any)
	if rules[0] != "DST-PORT,53,sempre-dns-out" {
		t.Fatalf("Mihomo DNS capture rule = %#v", rules)
	}
}

func mapByKey(t *testing.T, values []any, key string, expected any) map[string]any {
	t.Helper()
	for _, value := range values {
		item, ok := value.(map[string]any)
		if ok && item[key] == expected {
			return item
		}
	}
	t.Fatalf("missing %s=%v in %#v", key, expected, values)
	return nil
}

func assertLocalSingBoxInbounds(t *testing.T, inbounds []any, config LocalProxyConfig) {
	t.Helper()
	for _, tag := range []string{"sempre-socks-in", "sempre-http-in"} {
		inbound := mapByKey(t, inbounds, "tag", tag)
		if inbound["listen"] != "127.0.0.1" {
			t.Fatalf("local proxy is not loopback-only: %#v", inbound)
		}
		users := inbound["users"].([]any)
		user := users[0].(map[string]any)
		if user["username"] != config.Username || user["password"] != config.Password {
			t.Fatalf("local proxy authentication = %#v", users)
		}
	}
}

func TestMihomoNativeDNSAndOpenOverrides(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{
		"modes":     map[string]any{"mihomo": "native"},
		"overrides": map[string]any{"mihomo": map[string]any{"enable": false}},
	}
	profile.TransparentProxy.Mode = TransparentProxyDisabled
	profile.CoreOverrides["mihomo"] = map[string]any{"ipv6": false}
	profile.CoreOverrides["future-core"] = map[string]any{"preserved": true}
	result, updated, err := compiler.Render(context.Background(), profile, catalog, Target{Core: "mihomo", Format: "clash-meta"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := yaml.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	if config["dns"].(map[string]any)["enable"] != false || config["ipv6"] != false {
		t.Fatalf("Mihomo native overrides = %#v", config)
	}
	if updated.CoreOverrides["future-core"]["preserved"] != true {
		t.Fatalf("unknown core override was not preserved: %#v", updated.CoreOverrides)
	}
}

func compilerFixture(t *testing.T) (Profile, Catalog, *Compiler) {
	t.Helper()
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
	profile.UseSystemGroups = false
	profile.UseSystemRules = false
	profile.UseSystemFilters = false
	profile.UseSystemDNS = false
	profile.UseSystemCustomConfig = false
	profile.Sources = []Source{{ID: NewID(), Type: SourceRaw, Enabled: true, Content: "proxies:\n- name: edge\n  type: socks5\n  server: edge.example.com\n  port: 1080\n"}}
	return profile, catalog, NewCompiler(store)
}
