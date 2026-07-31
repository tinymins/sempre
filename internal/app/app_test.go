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
		document.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
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
	change, err := manager.UseCore(context.Background(), "sing-box@1.2.3")
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

func TestExactVersionCanBeSelectedBeforeConfiguration(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Selected = nil
		delete(document.Cores["sing-box"].Channels, "stable")
		document.Cores["sing-box"].Installed["1.2.3"].Explicit = true
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	change, err := manager.UseCore(context.Background(), "sing-box@1.2.3")
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed || change.NeedsRestart {
		t.Fatalf("selection change = %#v", change)
	}
	source := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(source, []byte(`{"log":{"level":"info"}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.ImportConfig(context.Background(), source); err != nil {
		t.Fatal(err)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Active == nil || document.Active.Ref != "1.2.3" || document.Active.Version != "1.2.3" {
		t.Fatalf("active = %#v", document.Active)
	}
}

func TestRemoveCoreDeletesVersionAndAliases(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Selected = nil
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	change, err := manager.RemoveCore("sing-box@1.2.3")
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
	if document.Cores["sing-box"] != nil {
		t.Fatalf("core state = %#v", document.Cores["sing-box"])
	}
	if _, err := os.Stat(manager.paths.CoreVersionDir("sing-box", "1.2.3")); !os.IsNotExist(err) {
		t.Fatalf("version directory still exists: %v", err)
	}
}

func TestRemoveCoreRejectsSelectedVersion(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if _, err := manager.RemoveCore("sing-box@1.2.3"); err == nil || !strings.Contains(err.Error(), "selected") {
		t.Fatalf("selected version removal error = %v", err)
	}
}

func TestCollectWeakVersionRemovesOnlyUnreferencedInstall(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	versionDir := manager.paths.CoreVersionDir("sing-box", "1.2.3")
	collected := false
	if err := manager.store.Update(func(document *state.Document) error {
		delete(document.Cores["sing-box"].Channels, "stable")
		collected = manager.collectWeakVersion(document, "sing-box", "1.2.3")
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if collected {
		if err := os.RemoveAll(versionDir); err != nil {
			t.Fatal(err)
		}
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

func TestClearSubscriptionRetainsConfiguration(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Configs["sing-box"] = "hash"
		document.Subscription = state.Subscription{
			URL:        "https://example.com/config?token=secret",
			Interval:   "24h",
			LastResult: "configuration updated",
		}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	change, err := manager.SetSubscription(context.Background(), "")
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
	if document.Subscription.URL != "" || document.Subscription.LastResult != "" {
		t.Fatalf("subscription = %#v", document.Subscription)
	}
	if document.Configs["sing-box"] != "hash" {
		t.Fatalf("configuration was removed: %#v", document.Configs)
	}
}

func TestStatusMarksDeadRuntimePIDAsStale(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Runtime = state.Runtime{State: "running", PID: 1 << 30}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	output, err := manager.Status(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output, "stale record") || !strings.Contains(output, "is not running") {
		t.Fatalf("status = %q", output)
	}
}

func TestResolveFailureRollsBackPendingDeploymentAndCollectsConfigs(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	oldDeployment := state.Deployment{
		Core:       "sing-box",
		Ref:        "stable",
		Version:    "1.2.3",
		ConfigHash: "old",
	}
	newDeployment := oldDeployment
	newDeployment.ConfigHash = "new"
	if err := manager.store.Update(func(document *state.Document) error {
		document.Configs["sing-box"] = "new"
		document.Active = &newDeployment
		document.Previous = &oldDeployment
		document.Pending = true
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	for _, hash := range []string{"old", "new", "unused"} {
		if err := state.WriteAtomic(manager.paths.Config("sing-box", hash), []byte("{}"), 0o600); err != nil {
			t.Fatal(err)
		}
	}

	retry, err := manager.rollbackPendingDeployment("resolve failed", errors.New("missing binary"))
	if err != nil {
		t.Fatal(err)
	}
	if !retry {
		t.Fatal("rollback did not request retry of the previous deployment")
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Pending || document.Previous != nil || !state.SameDeployment(document.Active, &oldDeployment) {
		t.Fatalf("document = %#v", document)
	}
	if document.Configs["sing-box"] != "old" {
		t.Fatalf("active config = %q", document.Configs["sing-box"])
	}
	if _, err := os.Stat(manager.paths.Config("sing-box", "old")); err != nil {
		t.Fatal(err)
	}
	for _, hash := range []string{"new", "unused"} {
		if _, err := os.Stat(manager.paths.Config("sing-box", hash)); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("configuration %s was retained: %v", hash, err)
		}
	}
}
