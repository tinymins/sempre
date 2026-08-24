package subscription

import (
	"context"
	"encoding/json"
	"testing"
)

func TestRecommendedDefaultsAreTargetSpecific(t *testing.T) {
	singBox := RecommendedDefaults("sing-box").DNS["shared"].(map[string]any)
	if singBox["localDnsTransport"] != "tls" || singBox["localDnsPort"] != 853 || singBox["localServerName"] != "dns.alidns.com" {
		t.Fatalf("sing-box local DNS recommendation = %#v", singBox)
	}
	mihomo := RecommendedDefaults("mihomo").DNS["shared"].(map[string]any)
	if mihomo["localDnsTransport"] != "udp" || mihomo["localDnsPort"] != 53 || mihomo["localServerName"] != "" {
		t.Fatalf("mihomo local DNS recommendation = %#v", mihomo)
	}
}

func TestEditorDefaultsExposeRecommendationsByCore(t *testing.T) {
	defaults := RecommendedEditorDefaults()
	payload, err := json.Marshal(defaults)
	if err != nil {
		t.Fatal(err)
	}
	var response map[string]any
	if err := json.Unmarshal(payload, &response); err != nil {
		t.Fatal(err)
	}
	if response["dns_config"] == nil || response["by_core"] == nil {
		t.Fatalf("editor defaults API shape = %#v", response)
	}
	var singBoxDNS map[string]any
	if err := json.Unmarshal([]byte(defaults.ByCore["sing-box"].DNSConfig), &singBoxDNS); err != nil {
		t.Fatal(err)
	}
	shared := singBoxDNS["shared"].(map[string]any)
	if shared["localDnsTransport"] != "tls" || shared["localDnsPort"] != float64(853) {
		t.Fatalf("sing-box editor DNS recommendation = %#v", shared)
	}
	if _, found := defaults.ByCore["mihomo"]; !found {
		t.Fatal("mihomo editor recommendation is missing")
	}
}

func TestCompilerUsesTargetDNSRecommendation(t *testing.T) {
	profile, catalog, compiler := compilerFixture(t)
	profile.UseSystemDNS = true
	result, _, err := compiler.Render(context.Background(), profile, catalog, Target{Format: "sing-box-v13-windows"}, false)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal([]byte(result.Content), &config); err != nil {
		t.Fatal(err)
	}
	local := mapByKey(t, config["dns"].(map[string]any)["servers"].([]any), "tag", "local")
	if local["type"] != "tls" || local["server_port"] != float64(853) || local["tls"].(map[string]any)["server_name"] != "dns.alidns.com" {
		t.Fatalf("compiled sing-box local DNS = %#v", local)
	}
}
