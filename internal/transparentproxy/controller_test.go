package transparentproxy

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"os"
	"path/filepath"
	"reflect"
	"strconv"
	"strings"
	"testing"
	"time"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"gopkg.in/yaml.v3"
)

type fakeBackend struct {
	inventory       Inventory
	forwarding      bool
	privilegeErr    error
	applyErr        error
	verifyTProxyErr error
	verifyTUNErr    error
	applyCalls      int
	cleanupCalls    int
}

func (*fakeBackend) Supported() bool { return true }

func (backend *fakeBackend) Inventory(context.Context) (Inventory, error) {
	return backend.inventory, nil
}

func (backend *fakeBackend) RequirePrivileges() error { return backend.privilegeErr }

func (backend *fakeBackend) IPv4Forwarding() (bool, error) { return backend.forwarding, nil }

func (backend *fakeBackend) ApplyTProxy(context.Context, Plan) error {
	backend.applyCalls++
	return backend.applyErr
}

func (backend *fakeBackend) VerifyTProxy(context.Context, Plan) error {
	return backend.verifyTProxyErr
}

func (backend *fakeBackend) VerifyTUN(context.Context, Plan) error { return backend.verifyTUNErr }

func (backend *fakeBackend) Diagnostics(context.Context, Plan) []Diagnostic { return nil }

func (backend *fakeBackend) Cleanup(context.Context) error {
	backend.cleanupCalls++
	return nil
}

func TestPrepareTUNResolvesAddressAndRouteExclusions(t *testing.T) {
	backend := &fakeBackend{
		forwarding: true,
		inventory: Inventory{
			Interfaces:               []Interface{{Name: "vmbr1", Up: true, Kind: "bridge"}},
			RecommendedLANInterfaces: []string{"vmbr1"},
			LocalPrefixes:            []string{"10.10.10.0/24", "172.19.0.0/30"},
			VPNPrefixes:              []string{"100.80.0.0/16"},
			OccupiedPrefixes:         []string{"10.10.10.0/24", "172.19.0.0/30"},
		},
	}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("gateway")
	profile.TransparentProxy.InterfaceMode = "include"
	profile.TransparentProxy.Interfaces = []string{"vmbr1"}
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{map[string]any{"type": "tun", "tag": "tun-in"}},
		"route":    map[string]any{},
	})

	plan, err := controller.Prepare(context.Background(), "sing-box", profile, path)
	if err != nil {
		t.Fatal(err)
	}
	if plan.TUNAddress != "172.19.0.5/30" {
		t.Fatalf("TUN address = %q", plan.TUNAddress)
	}
	if got := plan.RouteExclusions; !equalStrings(got, []string{"10.10.10.0/24", "100.80.0.0/16", "172.19.0.0/30"}) {
		t.Fatalf("route exclusions = %#v", got)
	}
	document := readRuntimeConfig(t, path)
	inbound := document["inbounds"].([]any)[0].(map[string]any)
	if inbound["auto_redirect"] != true || inbound["strict_route"] != true || inbound["stack"] != "system" {
		t.Fatalf("resolved TUN inbound = %#v", inbound)
	}
	if got := inbound["address"].([]any)[0]; got != "172.19.0.5/30" {
		t.Fatalf("runtime TUN address = %#v", got)
	}
	if got := inbound["include_interface"].([]any)[0]; got != "vmbr1" {
		t.Fatalf("runtime TUN include_interface = %#v", got)
	}
	if _, exists := inbound["route_include_interface"]; exists {
		t.Fatalf("runtime TUN contains unsupported route_include_interface: %#v", inbound)
	}
	if document["route"].(map[string]any)["auto_detect_interface"] != true {
		t.Fatalf("runtime route = %#v", document["route"])
	}
}

func TestPrepareTUNRejectsExplicitCollision(t *testing.T) {
	backend := &fakeBackend{inventory: Inventory{OccupiedPrefixes: []string{"172.19.0.0/24"}}}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("gateway")
	profile.TransparentProxy.TUN.Address = "172.19.0.1/30"
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{map[string]any{"type": "tun", "tag": "tun-in"}},
	})
	_, err := controller.Prepare(context.Background(), "sing-box", profile, path)
	if err == nil {
		t.Fatal("expected address collision")
	}
}

func TestPrepareTProxyUsesRecommendedLANAndMarksCoreOutbounds(t *testing.T) {
	backend := &fakeBackend{
		forwarding: true,
		inventory: Inventory{
			Interfaces:               []Interface{{Name: "vmbr1"}},
			RecommendedLANInterfaces: []string{"vmbr1"},
			LocalPrefixes:            []string{"10.10.10.0/24"},
		},
	}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("gateway")
	profile.TransparentProxy.Mode = subscriptions.TransparentProxyTProxy
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{
			map[string]any{"type": "direct", "tag": "dns-in"},
			map[string]any{"type": "tproxy", "tag": "tproxy-in"},
		},
		"outbounds": []any{map[string]any{"type": "trojan", "server": "203.0.113.9"}},
		"route":     map[string]any{},
	})
	plan, err := controller.Prepare(context.Background(), "sing-box", profile, path)
	if err != nil {
		t.Fatal(err)
	}
	if !equalStrings(plan.LANInterfaces, []string{"vmbr1"}) {
		t.Fatalf("LAN interfaces = %#v", plan.LANInterfaces)
	}
	if !contains(plan.ExcludedPrefixes, "203.0.113.9/32") {
		t.Fatalf("excluded prefixes = %#v", plan.ExcludedPrefixes)
	}
	document := readRuntimeConfig(t, path)
	if got := document["route"].(map[string]any)["default_mark"]; got != float64(BypassMark) {
		t.Fatalf("default mark = %#v", got)
	}
}

func TestPrepareSystemDNSTakeoverRequiresSystemSingBox(t *testing.T) {
	backend := &fakeBackend{}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("gateway")
	profile.DNS = map[string]any{"shared": map[string]any{"systemDnsTakeoverEnabled": true}}
	path := writeRuntimeConfig(t, map[string]any{})
	_, err := controller.Prepare(context.Background(), "sing-box", profile, path)
	if err == nil || !strings.Contains(err.Error(), "Linux system sing-box") {
		t.Fatalf("prepare system DNS error = %v", err)
	}
}

func TestPrepareGatewayRequiresIPForwarding(t *testing.T) {
	backend := &fakeBackend{
		inventory: Inventory{
			Interfaces:               []Interface{{Name: "vmbr1"}},
			RecommendedLANInterfaces: []string{"vmbr1"},
		},
	}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("gateway")
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{map[string]any{"type": "tun", "tag": "tun-in"}},
	})
	_, err := controller.Prepare(context.Background(), "sing-box", profile, path)
	if err == nil {
		t.Fatal("expected forwarding error")
	}
}

func TestPrepareMihomoTUNAndTProxyRuntime(t *testing.T) {
	backend := &fakeBackend{
		forwarding: true,
		inventory: Inventory{
			Interfaces:               []Interface{{Name: "vmbr1"}},
			RecommendedLANInterfaces: []string{"vmbr1"},
			LocalPrefixes:            []string{"10.10.10.0/24"},
		},
	}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("mihomo")
	profile.TransparentProxy.InterfaceMode = "exclude"
	profile.TransparentProxy.Interfaces = []string{"vmbr1"}
	path := writeYAMLRuntimeConfig(t, map[string]any{
		"tun":   map[string]any{},
		"dns":   map[string]any{"respect-rules": true, "nameserver": []string{"tls://dns.google:853#proxy"}, "nameserver-policy": map[string]any{"geosite:cn": []string{"127.0.0.1:53"}}},
		"rules": []string{"GEOIP,CN,DIRECT,no-resolve", "MATCH,proxy"},
	})
	plan, err := controller.Prepare(context.Background(), "mihomo", profile, path)
	if err != nil {
		t.Fatal(err)
	}
	if plan.Core != "mihomo" || plan.TUNInterface != "sempre-tun" {
		t.Fatalf("Mihomo TUN plan = %#v", plan)
	}
	document := readYAMLRuntimeConfig(t, path)
	tun := document["tun"].(map[string]any)
	if tun["auto-redirect"] != true || tun["exclude-interface"].([]any)[0] != "vmbr1" {
		t.Fatalf("Mihomo TUN runtime = %#v", tun)
	}

	profile.TransparentProxy.Mode = subscriptions.TransparentProxyTProxy
	profile.TransparentProxy.CaptureHost = true
	profile.TransparentProxy.LANInterfaces = []string{"vmbr1"}
	path = writeYAMLRuntimeConfig(t, map[string]any{
		"tproxy-port": 7893,
		"listeners":   []any{map[string]any{"name": "sempre-dns-in", "type": "tproxy", "port": 1053}},
		"proxies":     []any{map[string]any{"name": "edge", "type": "trojan", "server": "203.0.113.9"}},
	})
	plan, err = controller.Prepare(context.Background(), "mihomo", profile, path)
	if err != nil {
		t.Fatal(err)
	}
	if !contains(plan.ExcludedPrefixes, "203.0.113.9/32") {
		t.Fatalf("Mihomo exclusions = %#v", plan.ExcludedPrefixes)
	}
	document = readYAMLRuntimeConfig(t, path)
	if mark, ok := numberAsUint32(document["routing-mark"]); !ok || mark != BypassMark {
		t.Fatalf("Mihomo routing mark = %#v", document["routing-mark"])
	}
}

func TestPrepareXrayTUNAndV2RayTProxyShareRuntimePlan(t *testing.T) {
	backend := &fakeBackend{
		forwarding: true,
		inventory: Inventory{
			Interfaces:               []Interface{{Name: "vmbr1"}},
			RecommendedLANInterfaces: []string{"vmbr1"},
			LocalPrefixes:            []string{"10.10.10.0/24"},
		},
	}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("xray")
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{map[string]any{"tag": "tun-in", "protocol": "tun", "settings": map[string]any{}}},
		"routing":  map[string]any{"rules": []any{}},
	})
	plan, err := controller.Prepare(context.Background(), "xray", profile, path)
	if err != nil {
		t.Fatal(err)
	}
	if plan.TUNInterface != "sempre-tun" || plan.TUNAddress == "" {
		t.Fatalf("Xray plan = %#v", plan)
	}
	document := readRuntimeConfig(t, path)
	settings := document["inbounds"].([]any)[0].(map[string]any)["settings"].(map[string]any)
	if settings["autoOutboundsInterface"] != "auto" || firstString(settings["autoSystemRoutingTable"]) != "0.0.0.0/0" {
		t.Fatalf("Xray TUN settings = %#v", settings)
	}

	profile.TransparentProxy.Mode = subscriptions.TransparentProxyTProxy
	path = writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{
			map[string]any{"tag": "tproxy-in", "protocol": "dokodemo-door"},
			map[string]any{"tag": "dns-in", "protocol": "dokodemo-door"},
		},
		"outbounds": []any{map[string]any{"tag": "edge", "protocol": "trojan", "settings": map[string]any{"address": "203.0.113.9"}}},
	})
	plan, err = controller.Prepare(context.Background(), "v2ray", profile, path)
	if err != nil {
		t.Fatal(err)
	}
	if !contains(plan.ExcludedPrefixes, "203.0.113.9/32") {
		t.Fatalf("V2Ray exclusions = %#v", plan.ExcludedPrefixes)
	}
	document = readRuntimeConfig(t, path)
	outbound := document["outbounds"].([]any)[0].(map[string]any)
	mark := object(object(outbound["streamSettings"])["sockopt"])["mark"]
	if number, ok := numberAsUint32(mark); !ok || number != BypassMark {
		t.Fatalf("V2Ray bypass mark = %#v", mark)
	}
}

func TestPrepareClashRSTUNUsesNativeRuntimeFields(t *testing.T) {
	backend := &fakeBackend{forwarding: true, inventory: Inventory{OccupiedPrefixes: []string{"10.10.10.0/24"}}}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("clash-rs")
	path := writeYAMLRuntimeConfig(t, map[string]any{"tun": map[string]any{"enable": true}})
	plan, err := controller.Prepare(context.Background(), "clash-rs", profile, path)
	if err != nil {
		t.Fatal(err)
	}
	document := readYAMLRuntimeConfig(t, path)
	tun := object(document["tun"])
	if tun["route-all"] != true || tun["dns-hijack"] != true || tun["gateway"] != plan.TUNAddress {
		t.Fatalf("clash-rs TUN = %#v", tun)
	}
	for _, incompatible := range []string{"auto-route", "auto-redirect", "strict-route", "stack"} {
		if _, exists := tun[incompatible]; exists {
			t.Fatalf("clash-rs TUN contains Mihomo field %q: %#v", incompatible, tun)
		}
	}
}

func TestApplyTProxyRollsBackFailedVerification(t *testing.T) {
	first := listenTCP(t)
	defer first.Close()
	second := listenTCP(t)
	defer second.Close()
	backend := &fakeBackend{verifyTProxyErr: errors.New("missing rule")}
	controller := &Controller{backend: backend}
	plan := Plan{
		Mode:          subscriptions.TransparentProxyTProxy,
		TProxyPort:    listenerPort(t, first),
		DNSPort:       listenerPort(t, second),
		LANInterfaces: []string{"vmbr1"},
	}
	err := controller.Apply(context.Background(), plan)
	if err == nil {
		t.Fatal("expected verification failure")
	}
	if backend.applyCalls != 1 || backend.cleanupCalls != 1 {
		t.Fatalf("apply calls = %d, cleanup calls = %d", backend.applyCalls, backend.cleanupCalls)
	}
}

func TestSystemDNSTakeoverWritesAndRestoresResolvConf(t *testing.T) {
	stubSystemDNSChattr(t, nil)
	root := t.TempDir()
	resolv := filepath.Join(root, "resolv.conf")
	if err := os.WriteFile(resolv, []byte("nameserver 10.251.1.1\nnameserver 223.6.6.6\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	listener := listenTCP(t)
	defer listener.Close()
	controller := &Controller{
		backend:   &fakeBackend{},
		systemDNS: &systemDNSManager{allowed: true, stateDir: filepath.Join(root, "state"), resolvConf: resolv},
	}
	plan := Plan{SystemDNS: true, SystemDNSPort: listenerPort(t, listener)}
	if err := controller.Apply(context.Background(), plan); err != nil {
		t.Fatal(err)
	}
	current, err := os.ReadFile(resolv)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(current), "nameserver 127.0.0.1") {
		t.Fatalf("resolv.conf was not taken over: %q", current)
	}
	if err := controller.Cleanup(context.Background()); err != nil {
		t.Fatal(err)
	}
	current, err = os.ReadFile(resolv)
	if err != nil {
		t.Fatal(err)
	}
	if string(current) != "nameserver 10.251.1.1\nnameserver 223.6.6.6\n" {
		t.Fatalf("resolv.conf was not restored: %q", current)
	}
}

func TestSystemDNSTakeoverDoesNotOverwriteUserChangedResolvConf(t *testing.T) {
	stubSystemDNSChattr(t, nil)
	root := t.TempDir()
	resolv := filepath.Join(root, "resolv.conf")
	if err := os.WriteFile(resolv, []byte("nameserver 10.251.1.1\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	manager := &systemDNSManager{allowed: true, stateDir: filepath.Join(root, "state"), resolvConf: resolv}
	if err := manager.Apply(); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(resolv, []byte("nameserver 9.9.9.9\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := manager.Restore(); err != nil {
		t.Fatal(err)
	}
	current, err := os.ReadFile(resolv)
	if err != nil {
		t.Fatal(err)
	}
	if string(current) != "nameserver 9.9.9.9\n" {
		t.Fatalf("user resolv.conf change was overwritten: %q", current)
	}
}

func TestSystemDNSTakeoverLocksAndUnlocksResolvConf(t *testing.T) {
	var calls []bool
	stubSystemDNSChattr(t, func(_ string, immutable bool) error {
		calls = append(calls, immutable)
		return nil
	})
	root := t.TempDir()
	resolv := filepath.Join(root, "resolv.conf")
	if err := os.WriteFile(resolv, []byte("nameserver 10.251.1.1\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	manager := &systemDNSManager{allowed: true, stateDir: filepath.Join(root, "state"), resolvConf: resolv}
	if err := manager.Apply(); err != nil {
		t.Fatal(err)
	}
	if err := manager.Restore(); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(calls, []bool{true, false}) {
		t.Fatalf("chattr calls = %#v", calls)
	}
}

func TestSystemDNSManagedRequiresFirstNameserver(t *testing.T) {
	if !systemDNSManaged([]byte("# comment\noptions timeout:1\nnameserver 127.0.0.1\nnameserver 10.251.1.1\n")) {
		t.Fatal("expected first nameserver 127.0.0.1 to be managed")
	}
	if systemDNSManaged([]byte("nameserver 10.251.1.1\nnameserver 127.0.0.1\n")) {
		t.Fatal("expected later 127.0.0.1 nameserver to be unmanaged")
	}
}

func stubSystemDNSChattr(t *testing.T, replacement func(string, bool) error) {
	t.Helper()
	previous := systemDNSChattr
	if replacement == nil {
		replacement = func(string, bool) error { return nil }
	}
	systemDNSChattr = replacement
	t.Cleanup(func() {
		systemDNSChattr = previous
	})
}

func TestApplyTUNReadinessTimeoutExplainsInterface(t *testing.T) {
	backend := &fakeBackend{verifyTUNErr: errors.New("Link not found")}
	controller := &Controller{backend: backend}
	oldTimeout := tunReadinessTimeout
	oldPollInterval := readinessPollInterval
	tunReadinessTimeout = 5 * time.Millisecond
	readinessPollInterval = time.Millisecond
	defer func() {
		tunReadinessTimeout = oldTimeout
		readinessPollInterval = oldPollInterval
	}()

	err := controller.Apply(context.Background(), Plan{Mode: subscriptions.TransparentProxyTUN, TUNInterface: "sempre-tun"})
	if err == nil {
		t.Fatal("expected TUN readiness timeout")
	}
	for _, part := range []string{"timed out waiting for TUN interface sempre-tun", "after 5ms", "Link not found"} {
		if !strings.Contains(err.Error(), part) {
			t.Fatalf("timeout error %q does not contain %q", err, part)
		}
	}
}

func TestDiagnosticsValidateLinuxRoutingAndDNSStructure(t *testing.T) {
	backend := &fakeBackend{}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("gateway")
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{map[string]any{
			"type": "tun", "tag": "tun-in", "interface_name": "sing-box",
			"address": []string{"172.19.0.1/30"}, "auto_route": true,
			"auto_redirect": true, "strict_route": true, "stack": "system",
		}},
		"dns": map[string]any{
			"final":   "remote",
			"servers": []any{map[string]any{"tag": "remote", "detour": "foreign"}},
			"rules":   []any{map[string]any{"rule_set": []string{"geosite-cn"}, "server": "local"}},
		},
		"route": map[string]any{
			"auto_detect_interface": true,
			"final":                 "foreign",
			"rules":                 []any{map[string]any{"rule_set": []string{"geosite-cn"}, "outbound": "direct"}},
		},
	})
	diagnostics := controller.Diagnostics(context.Background(), "sing-box", profile, path)
	if len(diagnostics) != 3 {
		t.Fatalf("diagnostics = %#v", diagnostics)
	}
	for _, diagnostic := range diagnostics {
		if diagnostic.Err != nil {
			t.Fatalf("%s: %v", diagnostic.Name, diagnostic.Err)
		}
	}
	document := readRuntimeConfig(t, path)
	document["dns"].(map[string]any)["final"] = "local"
	path = writeRuntimeConfig(t, document)
	diagnostics = controller.Diagnostics(context.Background(), "sing-box", profile, path)
	if diagnostics[1].Err == nil {
		t.Fatal("expected invalid DNS final to fail diagnostics")
	}
}

func writeRuntimeConfig(t *testing.T, value map[string]any) string {
	t.Helper()
	data, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func readRuntimeConfig(t *testing.T, path string) map[string]any {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var result map[string]any
	if err := json.Unmarshal(data, &result); err != nil {
		t.Fatal(err)
	}
	return result
}

func writeYAMLRuntimeConfig(t *testing.T, value map[string]any) string {
	t.Helper()
	data, err := yaml.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "config.yaml")
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func readYAMLRuntimeConfig(t *testing.T, path string) map[string]any {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	result := map[string]any{}
	if err := yaml.Unmarshal(data, &result); err != nil {
		t.Fatal(err)
	}
	return result
}

func listenTCP(t *testing.T) net.Listener {
	t.Helper()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	return listener
}

func listenerPort(t *testing.T, listener net.Listener) int {
	t.Helper()
	_, value, err := net.SplitHostPort(listener.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	port, err := strconv.Atoi(value)
	if err != nil {
		t.Fatal(err)
	}
	return port
}

func equalStrings(left, right []string) bool {
	if len(left) != len(right) {
		return false
	}
	for index := range left {
		if left[index] != right[index] {
			return false
		}
	}
	return true
}

func contains(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}
