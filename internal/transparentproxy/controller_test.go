package transparentproxy

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"os"
	"path/filepath"
	"strconv"
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
