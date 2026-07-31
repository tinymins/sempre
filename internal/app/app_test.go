package app

import (
	"context"
	"errors"
	"io"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/sempre-lab/sempre/internal/core"
	"github.com/sempre-lab/sempre/internal/layout"
	"github.com/sempre-lab/sempre/internal/state"
)

type fakeAdapter struct{}

func (fakeAdapter) ID() string { return "sing-box" }

func (fakeAdapter) Resolve(context.Context, string, core.Target) (core.Package, error) {
	return core.Package{}, nil
}

func (fakeAdapter) ExecutableName(core.Target) string { return "sing-box" }

func (fakeAdapter) Version(context.Context, string) (string, error) { return "1.2.3", nil }

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
	if err := os.MkdirAll(paths.CoreVersionDir("sing-box", "1.2.3"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(paths.CoreBinary("sing-box", "1.2.3"), []byte("fake"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := manager.store.Update(func(document *state.Document) error {
		coreState := document.Core("sing-box")
		coreState.Channels["stable"] = "1.2.3"
		coreState.Installed["1.2.3"] = &state.Installation{}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	return manager
}

func TestImportConfigBootstrapsActiveDeployment(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	source := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(source, []byte(`{"log":{"level":"info"}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	change, err := manager.ImportConfig(context.Background(), source)
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed || !change.NeedsRestart {
		t.Fatalf("change = %#v", change)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Active == nil || document.Active.Core != "sing-box" || document.Active.Version != "1.2.3" {
		t.Fatalf("active = %#v", document.Active)
	}
	if !document.Pending || document.Active.ConfigHash == "" {
		t.Fatalf("document = %#v", document)
	}
	if _, err := os.Stat(manager.paths.Config("sing-box", document.Active.ConfigHash)); err != nil {
		t.Fatal(err)
	}
}

func TestUseExactVersionPromotesExplicitReference(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Configs["sing-box"] = "hash"
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	change, err := manager.UseCore("sing-box@1.2.3")
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed {
		t.Fatalf("change = %#v", change)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if !document.Cores["sing-box"].Installed["1.2.3"].Explicit {
		t.Fatal("exact use did not create an explicit reference")
	}
}

func TestCollectWeakVersionRemovesOnlyUnreferencedInstall(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	versionDir := manager.paths.CoreVersionDir("sing-box", "1.2.3")
	if err := manager.store.Update(func(document *state.Document) error {
		delete(document.Cores["sing-box"].Channels, "stable")
		manager.collectWeakVersion(document, "sing-box", "1.2.3")
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Cores["sing-box"].Installed["1.2.3"] != nil {
		t.Fatal("weak unreferenced install was retained")
	}
	if _, err := os.Stat(versionDir); !os.IsNotExist(err) {
		t.Fatalf("version directory still exists: %v", err)
	}
}

func TestSubscriptionURLValidationAndRedaction(t *testing.T) {
	t.Parallel()
	if _, err := validateSubscriptionURL("https://example.com/config?token=secret"); err != nil {
		t.Fatal(err)
	}
	for _, value := range []string{"http://example.com/config", "https://user:pass@example.com/config", "https:///missing"} {
		if _, err := validateSubscriptionURL(value); err == nil {
			t.Errorf("accepted %q", value)
		}
	}
	if got := redactedURL("https://example.com/config?token=secret"); got != "https://example.com" {
		t.Fatalf("redacted URL = %q", got)
	}
	networkError := &url.Error{
		Op:  "Get",
		URL: "https://example.com/config?token=secret",
		Err: errors.New("connection failed"),
	}
	if got := safeNetworkError(networkError); strings.Contains(got, "secret") {
		t.Fatalf("network error leaked URL: %q", got)
	}
}

func TestScheduleRequiresMinimumInterval(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if _, err := manager.SetSubscriptionSchedule("1m"); err == nil {
		t.Fatal("short interval was accepted")
	}
	change, err := manager.SetSubscriptionSchedule("12h")
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed || !strings.Contains(change.Message, "12h") {
		t.Fatalf("change = %#v", change)
	}
}
