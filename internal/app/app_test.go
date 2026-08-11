package app

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

type fakeAdapter struct{}

type fakeMihomoAdapter struct{ fakeAdapter }

type rejectingAdapter struct{ fakeAdapter }

func (rejectingAdapter) Validate(context.Context, string, string, string, io.Writer, io.Writer) error {
	return errors.New("rejected configuration")
}

var (
	testHashA = strings.Repeat("a", 64)
	testHashB = strings.Repeat("b", 64)
	testHashC = strings.Repeat("c", 64)
)

const testSubscription = "proxies:\n  - name: edge\n    type: ss\n    server: edge.example.com\n    port: 443\n    cipher: aes-128-gcm\n    password: secret\n"

func TestNewManagerRegistersOfficialCoreAdapters(t *testing.T) {
	t.Parallel()
	manager, err := New(layout.At(t.TempDir()), io.Discard, io.Discard)
	if err != nil {
		t.Fatal(err)
	}
	if actual := strings.Join(manager.CoreIDs(), ","); actual != "clash-rs,dae,mihomo,sing-box,v2ray,xray" {
		t.Fatalf("supported cores = %q", actual)
	}
	definitions := manager.CoreDefinitions()
	if len(definitions) != 6 || definitions[0].Stability != core.StabilityExperimental || definitions[5].ControlProtocol != core.ControlProtocolGRPC {
		t.Fatalf("core catalog = %#v", definitions)
	}
}

func TestNoSelectedCoreUsesOnlyStableCommonCapabilities(t *testing.T) {
	t.Parallel()
	manager, err := New(layout.At(t.TempDir()), io.Discard, io.Discard)
	if err != nil {
		t.Fatal(err)
	}
	configuration, err := manager.SubscriptionConfigurationContext()
	if err != nil {
		t.Fatal(err)
	}
	if configuration.Target != nil || configuration.Key != "common" {
		t.Fatalf("configuration target = %#v", configuration)
	}
	has := func(feature string) bool {
		for _, current := range configuration.Capabilities.Features {
			if current == feature {
				return true
			}
		}
		return false
	}
	for _, feature := range []string{core.CapabilityLoggingLevel, core.CapabilityDNSLocalUpstream, core.CapabilityRoutingRules, core.CapabilityLocalProxy} {
		if !has(feature) {
			t.Fatalf("stable common capability %q is missing: %#v", feature, configuration.Capabilities.Features)
		}
	}
	for _, feature := range []string{core.CapabilityTransparentEBPF, core.CapabilityManagementExternalAPI, core.CapabilityPrivateAccess} {
		if has(feature) {
			t.Fatalf("core-specific capability %q leaked into common settings: %#v", feature, configuration.Capabilities.Features)
		}
	}
}

func (fakeAdapter) ID() string { return "sing-box" }

func (fakeMihomoAdapter) ID() string { return "mihomo" }

func (fakeMihomoAdapter) DefaultRepository() string { return "MetaCubeX/mihomo" }

func (fakeMihomoAdapter) CompilerTarget(string, core.Target) (core.CompilerTarget, error) {
	return core.CompilerTarget{Format: "clash-meta"}, nil
}

func (fakeMihomoAdapter) ExecutableName(target core.Target) string {
	if target.OS == "windows" {
		return "mihomo-core.exe"
	}
	return "mihomo-core"
}

func (fakeAdapter) DefaultRepository() string { return "SagerNet/sing-box" }
func (fakeAdapter) CompilerTarget(version string, target core.Target) (core.CompilerTarget, error) {
	return core.CompilerTarget{Format: "sing-box-v13", Version: "13", Platform: "default"}, nil
}

func (fakeAdapter) Resolve(context.Context, string, string, core.Target) (core.Package, error) {
	return core.Package{}, nil
}

func (fakeAdapter) ExecutableName(target core.Target) string {
	if target.OS == "windows" {
		return "sing-box.exe"
	}
	return "sing-box"
}

func (fakeAdapter) Version(_ context.Context, binary string) (string, error) {
	if _, err := os.Stat(binary); err != nil {
		return "", err
	}
	return "1.2.3", nil
}

func (fakeAdapter) Validate(context.Context, string, string, string, io.Writer, io.Writer) error {
	return nil
}

func (fakeAdapter) Run(binary, config, dataDir string) core.RunSpec {
	return core.RunSpec{Path: binary, Args: []string{config}, WorkingDir: dataDir}
}

func newTestManager(t *testing.T) *Manager {
	t.Helper()
	paths := layout.At(t.TempDir())
	manager, err := New(paths, io.Discard, io.Discard)
	if err != nil {
		t.Fatal(err)
	}
	manager.registry = core.NewRegistry(fakeAdapter{})
	manager.commands = testCommandRegistrar{}
	if err := manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
		catalog.Profiles[0].UseSystemGroups = false
		catalog.Profiles[0].UseSystemRules = false
		catalog.Profiles[0].UseSystemFilters = false
		catalog.Profiles[0].UseSystemDNS = false
		catalog.Profiles[0].UseSystemCustomConfig = false
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(paths.CoreVersionDir("sing-box", "", "1.2.3"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(paths.CoreBinary("sing-box", "", "1.2.3"), []byte("fake"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := manager.store.Update(func(document *state.Document) error {
		source := document.Core("sing-box").Source("")
		source.Channels["stable"] = "1.2.3"
		source.Installed["1.2.3"] = &state.Installation{}
		document.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	return manager
}

type testCommandRegistrar struct{}

func (testCommandRegistrar) Register(paths layout.Layout) (func() error, error) {
	if err := os.MkdirAll(filepath.Dir(paths.CommandExecutable), 0o755); err != nil {
		return nil, err
	}
	data, err := os.ReadFile(paths.CommandExecutable)
	if err == nil {
		if string(data) == paths.ServiceExecutable {
			return func() error { return nil }, nil
		}
		return nil, fmt.Errorf("command path is not owned by Sempre")
	}
	if !errors.Is(err, os.ErrNotExist) {
		return nil, err
	}
	if err := os.WriteFile(paths.CommandExecutable, []byte(paths.ServiceExecutable), 0o600); err != nil {
		return nil, err
	}
	return func() error { return os.Remove(paths.CommandExecutable) }, nil
}

func (testCommandRegistrar) Unregister(paths layout.Layout) error {
	data, err := os.ReadFile(paths.CommandExecutable)
	if errors.Is(err, os.ErrNotExist) || (err == nil && string(data) != paths.ServiceExecutable) {
		return nil
	}
	if err != nil {
		return err
	}
	return os.Remove(paths.CommandExecutable)
}

func (testCommandRegistrar) Check(paths layout.Layout) error {
	data, err := os.ReadFile(paths.CommandExecutable)
	if err != nil {
		return err
	}
	if string(data) != paths.ServiceExecutable {
		return fmt.Errorf("command path is not owned by Sempre")
	}
	return nil
}
