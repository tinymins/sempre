package app

import (
	"context"
	"errors"
	"io"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
)

type fakeAdapter struct{}

type rejectingAdapter struct{ fakeAdapter }

func (rejectingAdapter) Validate(context.Context, string, string, string, io.Writer, io.Writer) error {
	return errors.New("rejected configuration")
}

var (
	testHashA = strings.Repeat("a", 64)
	testHashB = strings.Repeat("b", 64)
	testHashC = strings.Repeat("c", 64)
)

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
		document.Configs["sing-box"] = testHashA
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
		document.Selected = nil
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
		document.Configs["sing-box"] = testHashA
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
	if document.Configs["sing-box"] != testHashA {
		t.Fatalf("configuration was removed: %#v", document.Configs)
	}
}

func TestNewSubscriptionFailureDoesNotOverwriteSavedMetadata(t *testing.T) {
	manager := newTestManager(t)
	oldCheck := time.Date(2025, 1, 2, 3, 4, 5, 0, time.UTC)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Subscription.URL = "https://old.example/config.json"
		document.Subscription.LastCheck = oldCheck
		document.Subscription.LastResult = "no change"
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	server := httptest.NewTLSServer(nil)
	server.Close()
	if _, err := manager.SetSubscription(context.Background(), server.URL); err == nil {
		t.Fatal("new subscription unexpectedly succeeded")
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Subscription.URL != "https://old.example/config.json" ||
		!document.Subscription.LastCheck.Equal(oldCheck) ||
		document.Subscription.LastResult != "no change" {
		t.Fatalf("subscription metadata changed: %#v", document.Subscription)
	}
}

func TestSavedSubscriptionFailureRecordsAttempt(t *testing.T) {
	manager := newTestManager(t)
	server := httptest.NewTLSServer(nil)
	server.Close()
	oldCheck := time.Date(2025, 1, 2, 3, 4, 5, 0, time.UTC)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Subscription.URL = server.URL
		document.Subscription.LastCheck = oldCheck
		document.Subscription.LastResult = "no change"
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.UpdateSubscription(context.Background()); err == nil {
		t.Fatal("subscription update unexpectedly succeeded")
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if !document.Subscription.LastCheck.After(oldCheck) || document.Subscription.LastResult != "download failed" {
		t.Fatalf("subscription failure was not recorded: %#v", document.Subscription)
	}
}

func TestLocalValidationFailureDoesNotChangeSubscriptionMetadata(t *testing.T) {
	manager := newTestManager(t)
	manager.registry = core.NewRegistry(rejectingAdapter{})
	oldCheck := time.Date(2025, 1, 2, 3, 4, 5, 0, time.UTC)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Subscription.URL = "https://old.example/config.json"
		document.Subscription.LastCheck = oldCheck
		document.Subscription.LastResult = "no change"
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	config := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(config, []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.ImportConfig(context.Background(), config); err == nil {
		t.Fatal("invalid local configuration unexpectedly succeeded")
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if !document.Subscription.LastCheck.Equal(oldCheck) || document.Subscription.LastResult != "no change" {
		t.Fatalf("subscription metadata changed: %#v", document.Subscription)
	}
}

func TestDoctorReturnsFailureWhenChecksFail(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	report, err := manager.Doctor(context.Background())
	if !errors.Is(err, ErrDoctorFailed) {
		t.Fatalf("doctor error = %v", err)
	}
	if !strings.Contains(report, "[FAIL] active core") {
		t.Fatalf("doctor report = %q", report)
	}
}

func TestLogDeltaHandlesRotationAndLongLines(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	path := filepath.Join(root, "sempre.log")
	if err := os.WriteFile(path, []byte("first\npartial"), 0o600); err != nil {
		t.Fatal(err)
	}
	var output strings.Builder
	cursor, err := printLogDelta(&output, "sempre.log", path, logCursor{}, false)
	if err != nil {
		t.Fatal(err)
	}
	if output.String() != "[sempre.log] first\n" {
		t.Fatalf("initial output = %q", output.String())
	}
	if err := os.Rename(path, path+".1"); err != nil {
		t.Fatal(err)
	}
	longLine := strings.Repeat("x", 128*1024)
	if err := os.WriteFile(path, []byte("rotated\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	cursor, err = printLogDelta(&output, "sempre.log", path, cursor, false)
	if err != nil {
		t.Fatal(err)
	}
	file, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := file.WriteString(longLine); err != nil {
		file.Close()
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}
	_, err = printLogDelta(&output, "sempre.log", path, cursor, true)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), "[sempre.log] rotated\n") || !strings.Contains(output.String(), longLine) {
		t.Fatalf("rotated output length = %d", output.Len())
	}
}

func TestLogDeltaResetsAfterTruncation(t *testing.T) {
	t.Parallel()
	path := filepath.Join(t.TempDir(), "core.log")
	if err := os.WriteFile(path, []byte("before\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	var output strings.Builder
	cursor, err := printLogDelta(&output, "core.log", path, logCursor{}, false)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("after\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	_, err = printLogDelta(&output, "core.log", path, cursor, false)
	if err != nil {
		t.Fatal(err)
	}
	if output.String() != "[core.log] before\n[core.log] after\n" {
		t.Fatalf("truncated output = %q", output.String())
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
		ConfigHash: testHashA,
	}
	newDeployment := oldDeployment
	newDeployment.ConfigHash = testHashB
	if err := manager.store.Update(func(document *state.Document) error {
		document.Configs["sing-box"] = testHashB
		document.Active = &newDeployment
		document.Previous = &oldDeployment
		document.Pending = true
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	for _, hash := range []string{testHashA, testHashB, testHashC} {
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
	if document.Configs["sing-box"] != testHashA {
		t.Fatalf("active config = %q", document.Configs["sing-box"])
	}
	if _, err := os.Stat(manager.paths.Config("sing-box", testHashA)); err != nil {
		t.Fatal(err)
	}
	for _, hash := range []string{testHashB, testHashC} {
		if _, err := os.Stat(manager.paths.Config("sing-box", hash)); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("configuration %s was retained: %v", hash, err)
		}
	}
}
