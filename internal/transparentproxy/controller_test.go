package transparentproxy

import (
	"context"
	"reflect"
	"strings"
	"testing"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
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

func TestPrepareTUNKeepsFakeIPOutOfRouteExclusions(t *testing.T) {
	backend := &fakeBackend{
		forwarding: true,
		inventory: Inventory{
			Interfaces:               []Interface{{Name: "vmbr1", Up: true, Kind: "bridge"}},
			RecommendedLANInterfaces: []string{"vmbr1"},
			LocalPrefixes:            []string{"10.10.10.0/24"},
			VPNPrefixes:              []string{"198.18.0.0/16"},
			OccupiedPrefixes:         []string{"10.10.10.0/24"},
		},
	}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("gateway")
	profile.TransparentProxy.RouteExclusions = []string{"198.18.10.0/24"}
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{map[string]any{"type": "tun", "tag": "tun-in"}},
		"dns": map[string]any{"servers": []any{
			map[string]any{"tag": "fakeip", "type": "fakeip", "inet4_range": "198.18.0.0/15"},
		}},
		"route": map[string]any{},
	})

	plan, err := controller.Prepare(context.Background(), "sing-box", profile, path)
	if err != nil {
		t.Fatal(err)
	}
	if !equalStrings(plan.RouteExclusions, []string{"10.10.10.0/24"}) {
		t.Fatalf("route exclusions = %#v", plan.RouteExclusions)
	}
	if !equalStrings(plan.FakeIPPrefixes, []string{"198.18.0.0/15"}) {
		t.Fatalf("fake-ip prefixes = %#v", plan.FakeIPPrefixes)
	}
	if len(plan.FakeIPConflicts) == 0 || !strings.Contains(plan.FakeIPConflicts[0], "VPN route 198.18.0.0/16") {
		t.Fatalf("fake-ip conflicts = %#v", plan.FakeIPConflicts)
	}
	document := readRuntimeConfig(t, path)
	inbound := document["inbounds"].([]any)[0].(map[string]any)
	if got := stringValues(inbound["route_exclude_address"]); !equalStrings(got, []string{"10.10.10.0/24"}) {
		t.Fatalf("runtime route_exclude_address = %#v", got)
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

func TestPrepareSystemDNSTakeoverAcceptsSelectedListenHosts(t *testing.T) {
	backend := &fakeBackend{}
	controller := &Controller{backend: backend, systemDNS: &systemDNSManager{allowed: true}}
	profile := subscriptions.NewProfile("gateway")
	profile.TransparentProxy.Mode = subscriptions.TransparentProxyDisabled
	profile.DNS = map[string]any{"shared": map[string]any{
		"systemDnsTakeoverEnabled": true,
		"systemDnsListenHosts":     []any{"127.0.0.1", "10.10.10.1"},
	}}
	path := writeRuntimeConfig(t, map[string]any{
		"inbounds": []any{
			map[string]any{"type": "direct", "tag": "system-dns-in", "listen": "127.0.0.1", "listen_port": 53, "override_address": "1.1.1.1", "override_port": 53},
			map[string]any{"type": "direct", "tag": "system-dns-in-1", "listen": "10.10.10.1", "listen_port": 53, "override_address": "1.1.1.1", "override_port": 53},
		},
		"route": map[string]any{"rules": []any{
			map[string]any{"inbound": "system-dns-in", "action": "sniff"},
			map[string]any{"inbound": "system-dns-in", "protocol": "dns", "action": "hijack-dns"},
			map[string]any{"inbound": "system-dns-in-1", "action": "sniff"},
			map[string]any{"inbound": "system-dns-in-1", "protocol": "dns", "action": "hijack-dns"},
		}},
	})

	plan, err := controller.Prepare(context.Background(), "sing-box", profile, path)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(plan.SystemDNSHosts, []string{"127.0.0.1", "10.10.10.1"}) {
		t.Fatalf("system DNS hosts = %#v", plan.SystemDNSHosts)
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

func TestPrepareMihomoTUNKeepsFakeIPOutOfRouteExclusions(t *testing.T) {
	backend := &fakeBackend{
		forwarding: true,
		inventory: Inventory{
			Interfaces:               []Interface{{Name: "vmbr1"}},
			RecommendedLANInterfaces: []string{"vmbr1"},
			LocalPrefixes:            []string{"10.10.10.0/24"},
			VPNPrefixes:              []string{"198.18.0.0/16"},
		},
	}
	controller := &Controller{backend: backend}
	profile := subscriptions.NewProfile("mihomo")
	path := writeYAMLRuntimeConfig(t, map[string]any{
		"tun": map[string]any{},
		"dns": map[string]any{
			"enhanced-mode":     "fake-ip",
			"fake-ip-range":     "198.18.0.0/15",
			"respect-rules":     true,
			"nameserver":        []string{"tls://dns.google:853#proxy"},
			"nameserver-policy": map[string]any{"geosite:cn": []string{"127.0.0.1:53"}},
		},
		"rules": []string{"GEOIP,CN,DIRECT,no-resolve", "MATCH,proxy"},
	})

	plan, err := controller.Prepare(context.Background(), "mihomo", profile, path)
	if err != nil {
		t.Fatal(err)
	}
	if !equalStrings(plan.RouteExclusions, []string{"10.10.10.0/24"}) {
		t.Fatalf("route exclusions = %#v", plan.RouteExclusions)
	}
	document := readYAMLRuntimeConfig(t, path)
	tun := object(document["tun"])
	if got := stringValues(tun["route-exclude-address"]); !equalStrings(got, []string{"10.10.10.0/24"}) {
		t.Fatalf("runtime route-exclude-address = %#v", got)
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
