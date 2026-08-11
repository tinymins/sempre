package subscription

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/tinymins/sempre/internal/core"
	v2raycore "github.com/tinymins/sempre/internal/core/v2ray"
	xraycore "github.com/tinymins/sempre/internal/core/xray"
)

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
