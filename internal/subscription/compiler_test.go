package subscription

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/core/clashrs"
	"github.com/tinymins/sempre/internal/core/dae"
	v2raycore "github.com/tinymins/sempre/internal/core/v2ray"
	xraycore "github.com/tinymins/sempre/internal/core/xray"
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
	local := mapByKey(t, servers, "tag", "local")
	if local["type"] != "local" {
		t.Fatalf("default local DNS should use system resolver: %#v", local)
	}
	bootstrap := mapByKey(t, servers, "tag", "bootstrap")
	if _, ok := bootstrap["detour"]; ok {
		t.Fatalf("bootstrap DNS should use the default direct dialer: %#v", bootstrap)
	}
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

func TestSingBoxManagedDNSUsesExplicitLocalUpstream(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{
		"fakeipEnabled": false, "localDns": "223.5.5.5", "localDnsPort": 53,
	}}

	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	local := mapByKey(t, config["dns"].(map[string]any)["servers"].([]any), "tag", "local")
	if local["type"] != "udp" || local["server"] != "223.5.5.5" || local["server_port"] != float64(53) {
		t.Fatalf("explicit local DNS = %#v", local)
	}
}

func TestSingBoxManagedDNSUsesFirstCommaSeparatedLocalUpstream(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{
		"fakeipEnabled": false, "localDns": "223.5.5.5, 223.6.6.6", "localDnsPort": 53,
	}}

	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	local := mapByKey(t, config["dns"].(map[string]any)["servers"].([]any), "tag", "local")
	if local["type"] != "udp" || local["server"] != "223.5.5.5" {
		t.Fatalf("comma-separated local DNS = %#v", local)
	}
}

func TestSingBoxSystemDNSTakeoverAddsLocalDNSListener(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{
		"fakeipEnabled": false, "localDns": "223.5.5.5", "systemDnsTakeoverEnabled": true, "systemDnsListenPort": 53,
	}}

	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	inbound := mapByKey(t, config["inbounds"].([]any), "tag", "system-dns-in")
	if inbound["type"] != "direct" || inbound["listen"] != "127.0.0.1" || inbound["listen_port"] != float64(53) {
		t.Fatalf("system DNS inbound = %#v", inbound)
	}
}

func TestSingBoxSystemDNSTakeoverRequiresExplicitLocalDNS(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{"systemDnsTakeoverEnabled": true}}

	_, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err == nil || !strings.Contains(err.Error(), "explicit local DNS") {
		t.Fatalf("system DNS takeover error = %v", err)
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
	result, _, err = compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	assertLocalSingBoxInbounds(t, config["inbounds"].([]any), profile.LocalProxy)
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
			profile.CoreOverrides[test.coreID] = test.override
			if _, _, err := compiler.Render(context.Background(), profile, catalog, test.target, false); err == nil {
				t.Fatal("native controller override was silently accepted")
			}
		})
	}
}

func TestLegacySingBoxOmitsDirectDNSDetours(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{
		"fakeipEnabled": false,
		"remoteDetour":  "direct",
	}}

	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	servers := config["dns"].(map[string]any)["servers"].([]any)
	for _, tag := range []string{"local", "bootstrap", "remote"} {
		server := mapByKey(t, servers, "tag", tag)
		if _, ok := server["detour"]; ok {
			t.Fatalf("%s DNS should use the default direct dialer: %#v", tag, server)
		}
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

func TestXrayRendererUsesCurrentFieldsAndSplitDNS(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.Groups = []ProxyGroup{{Name: "foreign", Type: "select", IncludeAll: true, Default: "edge"}}
	profile.DNS = map[string]any{"shared": map[string]any{"fakeipEnabled": false, "preferIpv4": true}}
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Core: "xray", Format: "xray"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	outbound := mapByKey(t, config["outbounds"].([]any), "tag", "edge")
	settings := outbound["settings"].(map[string]any)
	if outbound["protocol"] != "socks" || settings["address"] != "edge.example.com" || settings["port"] != float64(1080) {
		t.Fatalf("Xray outbound = %#v", outbound)
	}
	inbounds := config["inbounds"].([]any)
	tun := mapByKey(t, inbounds, "tag", "tun-in")
	if tun["protocol"] != "tun" || tun["settings"].(map[string]any)["autoOutboundsInterface"] != "auto" {
		t.Fatalf("Xray TUN = %#v", tun)
	}
	socks := mapByKey(t, inbounds, "tag", "sempre-socks-in")
	if socks["listen"] != "127.0.0.1" || socks["settings"].(map[string]any)["users"] == nil {
		t.Fatalf("Xray local proxy = %#v", socks)
	}
	dns := config["dns"].(map[string]any)
	if dns["queryStrategy"] != "UseIPv4" || dns["disableFallbackIfMatch"] != true {
		t.Fatalf("Xray DNS = %#v", dns)
	}
	routing := config["routing"].(map[string]any)
	rules := routing["rules"].([]any)
	foreign := mapByKey(t, routing["balancers"].([]any), "tag", "foreign")
	selectors := foreign["selector"].([]any)
	if len(selectors) != 1 || selectors[0] != "edge" {
		t.Fatalf("Xray persistent selector default = %#v", foreign)
	}
	var remoteDNSRule map[string]any
	for _, value := range rules {
		rule := value.(map[string]any)
		tags, _ := rule["inboundTag"].([]any)
		if len(tags) == 1 && tags[0] == "remote-dns" {
			remoteDNSRule = rule
			break
		}
	}
	if remoteDNSRule == nil {
		t.Fatalf("missing remote DNS route in %#v", rules)
	}
	if remoteDNSRule["balancerTag"] != "foreign" {
		t.Fatalf("remote DNS route = %#v", remoteDNSRule)
	}
}

func TestV2RayRendererUsesIndependentLegacyFields(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.TransparentProxy.Mode = TransparentProxyTProxy
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Core: "v2ray", Format: "v2ray"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	outbound := mapByKey(t, config["outbounds"].([]any), "tag", "edge")
	servers := outbound["settings"].(map[string]any)["servers"].([]any)
	if outbound["protocol"] != "socks" || len(servers) != 1 {
		t.Fatalf("V2Ray outbound = %#v", outbound)
	}
	inbounds := config["inbounds"].([]any)
	tproxy := mapByKey(t, inbounds, "tag", "tproxy-in")
	if tproxy["protocol"] != "dokodemo-door" || mapByKey(t, inbounds, "tag", "dns-in")["port"] != float64(profile.TransparentProxy.TProxy.DNSListenPort) {
		t.Fatalf("V2Ray TProxy = %#v", tproxy)
	}
}

func TestV2RayRendererRemovesUnsupportedNodesFromGroups(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.Sources = []Source{{ID: NewID(), Type: SourceRaw, Enabled: true, Content: `proxies:
- name: supported
  type: socks5
  server: edge.example.com
  port: 1080
- name: unsupported
  type: hysteria2
  server: edge.example.com
  port: 443
  password: secret
`}}
	profile.Groups = []ProxyGroup{{Name: "foreign", Type: "url-test", IncludeAll: true, Default: "unsupported"}}
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Core: "v2ray", Format: "v2ray"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	balancer := mapByKey(t, config["routing"].(map[string]any)["balancers"].([]any), "tag", "foreign")
	selectors := balancer["selector"].([]any)
	if len(selectors) != 1 || selectors[0] != "supported" || result.NodeCount != 1 {
		t.Fatalf("filtered V2Ray group = %#v, nodes = %d", balancer, result.NodeCount)
	}
}

func TestXrayRendererAppliesNativeDNSOverride(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{
		"modes": map[string]any{"xray": "native"},
		"overrides": map[string]any{"xray": map[string]any{
			"servers": []any{"https://1.1.1.1/dns-query"}, "queryStrategy": "UseIPv6",
		}},
	}
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Core: "xray", Format: "xray"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	dns := config["dns"].(map[string]any)
	if dns["queryStrategy"] != "UseIPv6" || dns["servers"].([]any)[0] != "https://1.1.1.1/dns-query" {
		t.Fatalf("native Xray DNS = %#v", dns)
	}
}

func TestV2RayFamilyGeneratedConfigPassesOfficialValidation(t *testing.T) {
	tests := []struct {
		name, environment string
		adapter           core.Adapter
	}{
		{name: "xray", environment: "SEMPRE_TEST_XRAY", adapter: xraycore.New()},
		{name: "v2ray", environment: "SEMPRE_TEST_V2RAY", adapter: v2raycore.New()},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			binary := os.Getenv(test.environment)
			if binary == "" {
				t.Skip(test.environment + " is not set")
			}
			if version, err := test.adapter.Version(context.Background(), binary); err != nil || version == "" {
				t.Fatalf("official version detection failed: version=%q err=%v", version, err)
			}
			profile, catalog, compiler := compilerFixture(t)
			profile.TransparentProxy.Mode = TransparentProxyDisabled
			profile.Sources = []Source{{ID: NewID(), Type: SourceRaw, Enabled: true, Content: v2RayFamilyValidationSource(test.name)}}
			profile.Groups = []ProxyGroup{
				{Name: "foreign", Type: "select", IncludeAll: true, Default: "vmess-ws"},
				{Name: "automatic", Type: "url-test", IncludeAll: true, Interval: 300},
			}
			result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Core: test.name, Format: test.name}, false)
			if err != nil {
				t.Fatal(err)
			}
			root := t.TempDir()
			config := filepath.Join(root, "config.json")
			if err := os.WriteFile(config, []byte(result.Content), 0o600); err != nil {
				t.Fatal(err)
			}
			preparer := test.adapter.(core.RuntimePreparer)
			runtimeSpec, err := preparer.PrepareRuntime(config, filepath.Join(root, "control"))
			if err != nil {
				t.Fatal(err)
			}
			var output bytes.Buffer
			if err := test.adapter.Validate(context.Background(), binary, runtimeSpec.Config, root, &output, &output); err != nil {
				t.Fatalf("official validation failed: %v\n%s", err, output.String())
			}
		})
	}
}

func v2RayFamilyValidationSource(coreID string) string {
	common := `proxies:
- name: vmess-ws
  type: vmess
  server: vmess.example.com
  port: 443
  uuid: 11111111-1111-4111-8111-111111111111
  alterId: 0
  cipher: auto
  network: ws
  tls: true
  servername: vmess.example.com
  ws-opts:
    path: /ws
- name: trojan-grpc
  type: trojan
  server: trojan.example.com
  port: 443
  password: secret
  network: grpc
  tls: true
  servername: trojan.example.com
  grpc-opts:
    grpc-service-name: tunnel
- name: shadowsocks
  type: ss
  server: ss.example.com
  port: 8388
  cipher: aes-256-gcm
  password: secret
`
	if coreID == "xray" {
		return common + `- name: vless-reality
  type: vless
  server: reality.example.com
  port: 443
  uuid: 22222222-2222-4222-8222-222222222222
  network: tcp
  servername: www.microsoft.com
  client-fingerprint: chrome
  reality-opts:
    public-key: K7q8L3LJT_Us_y8Rmo8cnfH3IVP2I4gQ8c7GqfI6uxM
    short-id: 0123456789abcdef
`
	}
	return common + `- name: vless-ws
  type: vless
  server: vless.example.com
  port: 443
  uuid: 22222222-2222-4222-8222-222222222222
  network: ws
  tls: true
  servername: vless.example.com
  ws-opts:
    path: /vless
`
}

func TestClashRSRendererUsesCompatibilityRuntimeFields(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.TransparentProxy.Mode = TransparentProxyTProxy
	profile.CoreOverrides["clash-rs"] = map[string]any{"ipv6": false}
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Core: "clash-rs", Format: "clash-rs"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := yaml.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	if config["tproxy-port"] != profile.TransparentProxy.TProxy.ListenPort || config["listeners"] == nil || config["socks-port"] != profile.LocalProxy.SOCKSPort || config["authentication"] == nil || config["ipv6"] != false {
		t.Fatalf("clash-rs runtime = %#v", config)
	}
}

func TestClashRSRendererDropsUnsupportedHTTPProxy(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.Sources = []Source{{ID: NewID(), Type: SourceRaw, Enabled: true, Content: `proxies:
- name: supported
  type: socks5
  server: socks.example.com
  port: 1080
- name: unsupported
  type: http
  server: http.example.com
  port: 8080
`}}
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "clash-rs"}, false)
	if err != nil {
		t.Fatal(err)
	}
	if result.NodeCount != 1 || len(result.Warnings) == 0 || !strings.Contains(result.Warnings[len(result.Warnings)-1], "unsupported proxy type http") {
		t.Fatalf("clash-rs result = %#v", result)
	}
	var config map[string]any
	if err := yaml.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	proxies := config["proxies"].([]any)
	if len(proxies) != 1 || proxies[0].(map[string]any)["name"] != "supported" || config["socks-port"] == nil {
		t.Fatalf("clash-rs proxies = %#v", config)
	}
}

func TestExperimentalGeneratedConfigPassesOfficialValidation(t *testing.T) {
	tests := []struct {
		name, environment, extension string
		adapter                      core.Adapter
	}{
		{name: "clash-rs", environment: "SEMPRE_TEST_CLASH_RS", extension: ".yaml", adapter: clashrs.New()},
		{name: "dae", environment: "SEMPRE_TEST_DAE", extension: ".dae", adapter: dae.New()},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			binary := os.Getenv(test.environment)
			if binary == "" {
				t.Skip(test.environment + " is not set")
			}
			if version, err := test.adapter.Version(context.Background(), binary); err != nil || version == "" {
				t.Fatalf("official version detection failed: version=%q err=%v", version, err)
			}
			profile, catalog, compiler := compilerFixture(t)
			profile.Sources = []Source{{ID: NewID(), Type: SourceRaw, Enabled: true, Content: experimentalValidationSource()}}
			if test.name == "dae" {
				profile.TransparentProxy.Mode = TransparentProxyEBPF
			}
			result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Core: test.name, Format: test.name}, false)
			if err != nil {
				t.Fatal(err)
			}
			root := t.TempDir()
			config := filepath.Join(root, "config"+test.extension)
			if err := os.WriteFile(config, []byte(result.Content), 0o600); err != nil {
				t.Fatal(err)
			}
			if preparer, ok := test.adapter.(core.RuntimePreparer); ok {
				runtimeSpec, prepareErr := preparer.PrepareRuntime(config, filepath.Join(root, "control"))
				if prepareErr != nil {
					t.Fatal(prepareErr)
				}
				config = runtimeSpec.Config
			}
			var output bytes.Buffer
			if err := test.adapter.Validate(context.Background(), binary, config, root, &output, &output); err != nil {
				validated, _ := os.ReadFile(config)
				t.Fatalf("official validation failed: %v\n%s\n%s", err, output.String(), validated)
			}
		})
	}
}

func experimentalValidationSource() string {
	return `proxies:
- name: vless-reality
  type: vless
  server: reality.example.com
  port: 443
  uuid: 22222222-2222-4222-8222-222222222222
  network: tcp
  servername: www.microsoft.com
  client-fingerprint: chrome
  reality-opts:
    public-key: K7q8L3LJT_Us_y8Rmo8cnfH3IVP2I4gQ8c7GqfI6uxM
    short-id: 0123456789abcdef
- name: vmess-ws
  type: vmess
  server: vmess.example.com
  port: 443
  uuid: 11111111-1111-4111-8111-111111111111
  alterId: 0
  cipher: auto
  network: ws
  tls: true
  ws-opts:
    path: /ws
- name: trojan-grpc
  type: trojan
  server: trojan.example.com
  port: 443
  password: secret
  network: grpc
  tls: true
  grpc-opts:
    grpc-service-name: tunnel
- name: shadowsocks
  type: ss
  server: ss.example.com
  port: 8388
  cipher: aes-256-gcm
  password: secret
- name: https-proxy
  type: http
  server: http.example.com
  port: 443
  username: user
  password: secret
  tls: true
- name: socks-proxy
  type: socks5
  server: socks.example.com
  port: 1080
  username: user
  password: secret
- name: hysteria2
  type: hysteria2
  server: hy2.example.com
  port: 443
  password: secret
  sni: hy2.example.com
- name: tuic
  type: tuic
  server: tuic.example.com
  port: 443
  uuid: 33333333-3333-4333-8333-333333333333
  password: secret
  sni: tuic.example.com
- name: anytls
  type: anytls
  server: anytls.example.com
  port: 443
  password: secret
  sni: anytls.example.com
`
}

func TestDaeRendererUsesNativeEBPFIntent(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.TransparentProxy.Mode = TransparentProxyEBPF
	profile.TransparentProxy.LANInterfaces = []string{"vmbr1"}
	profile.TransparentProxy.EBPF.WANInterface = "vmbr0"
	profile.TransparentProxy.EBPF.AutoConfigKernelParameter = true
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Core: "dae", Format: "dae"}, false)
	if err != nil {
		t.Fatal(err)
	}
	for _, expected := range []string{
		"lan_interface: vmbr1", "wan_interface: vmbr0", "auto_config_kernel_parameter: true",
		"qname(geosite:cn) -> local", "bootstrap_resolver: \"223.5.5.5:53\"",
		"remote: \"tls://8.8.8.8:853\"", "domain(geosite:cn) -> direct", "fallback: sempre_group_1",
	} {
		if !strings.Contains(result.Content, expected) {
			t.Fatalf("dae config missing %q:\n%s", expected, result.Content)
		}
	}
}

func TestDaeHTTPSNodeUsesTLSURI(t *testing.T) {
	link, ok := daeNodeURI(Proxy{
		Name: "secure-http", Type: "http", Server: "proxy.example.com", Port: 443,
		Extra: map[string]any{"tls": true, "username": "user", "password": "secret"},
	})
	if !ok || !strings.HasPrefix(link, "https://user:secret@proxy.example.com:443/") {
		t.Fatalf("dae HTTPS link = %q, supported = %t", link, ok)
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
