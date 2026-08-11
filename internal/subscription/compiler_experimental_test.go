package subscription

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/core/clashrs"
	"github.com/tinymins/sempre/internal/core/dae"
	"github.com/tinymins/sempre/internal/layout"
	"gopkg.in/yaml.v3"
)

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
