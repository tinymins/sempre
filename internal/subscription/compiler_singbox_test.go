package subscription

import (
	"context"
	"encoding/json"
	"reflect"
	"strings"
	"testing"
)

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
	if inbound["type"] != "direct" || inbound["listen"] != "127.0.0.1" || inbound["listen_port"] != float64(53) || inbound["override_address"] != "1.1.1.1" || inbound["override_port"] != float64(53) {
		t.Fatalf("system DNS inbound = %#v", inbound)
	}
	rules := config["route"].(map[string]any)["rules"].([]any)
	if !reflect.DeepEqual(rules[0], map[string]any{"inbound": "system-dns-in", "action": "sniff"}) ||
		!reflect.DeepEqual(rules[1], map[string]any{"inbound": "system-dns-in", "protocol": "dns", "action": "hijack-dns"}) {
		t.Fatalf("system DNS route rules = %#v", rules[:2])
	}
}

func TestSingBoxSystemDNSTakeoverAddsSelectedDNSListeners(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{
		"fakeipEnabled": false, "localDns": "223.5.5.5", "systemDnsTakeoverEnabled": true, "systemDnsListenHosts": []any{"127.0.0.1", "10.10.10.1"},
	}}

	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	inbounds := config["inbounds"].([]any)
	loopback := mapByKey(t, inbounds, "tag", "system-dns-in")
	lan := mapByKey(t, inbounds, "tag", "system-dns-in-1")
	if loopback["listen"] != "127.0.0.1" || lan["listen"] != "10.10.10.1" {
		t.Fatalf("system DNS inbounds = %#v %#v", loopback, lan)
	}
	rules := config["route"].(map[string]any)["rules"].([]any)
	if !reflect.DeepEqual(rules[0], map[string]any{"inbound": "system-dns-in", "action": "sniff"}) ||
		!reflect.DeepEqual(rules[2], map[string]any{"inbound": "system-dns-in-1", "action": "sniff"}) {
		t.Fatalf("system DNS route rules = %#v", rules[:4])
	}
}

func TestSingBoxSystemDNSTakeoverWildcardDNSListenerIsExclusive(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{
		"fakeipEnabled": false, "localDns": "223.5.5.5", "systemDnsTakeoverEnabled": true, "systemDnsListenHosts": []any{"127.0.0.1", "0.0.0.0", "10.10.10.1"},
	}}

	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	inbound := mapByKey(t, config["inbounds"].([]any), "tag", "system-dns-in-any")
	if inbound["listen"] != "0.0.0.0" || inbound["listen_port"] != float64(53) {
		t.Fatalf("system DNS wildcard inbound = %#v", inbound)
	}
}

func TestSingBoxSystemDNSTakeoverRequiresLoopbackOrWildcard(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.DNS = map[string]any{"shared": map[string]any{
		"fakeipEnabled": false, "localDns": "223.5.5.5", "systemDnsTakeoverEnabled": true, "systemDnsListenHosts": []any{"10.10.10.1"},
	}}

	_, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13"}, false)
	if err == nil || !strings.Contains(err.Error(), "127.0.0.1 or 0.0.0.0") {
		t.Fatalf("system DNS takeover error = %v", err)
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
