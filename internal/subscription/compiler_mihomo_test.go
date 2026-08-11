package subscription

import (
	"context"
	"testing"

	"gopkg.in/yaml.v3"
)

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
