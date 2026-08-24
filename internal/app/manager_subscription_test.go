package app

import (
	"context"
	"errors"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

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
	catalog, _, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	if err := manager.subscriptions.Update(func(stored *subscriptions.Catalog) error {
		profile, findErr := subscriptions.FindProfile(stored, catalog.Profiles[0].ID)
		if findErr != nil {
			return findErr
		}
		profile.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true, URL: "https://example.com/config?token=secret"}}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	change, err := manager.SetSubscription(context.Background(), "")
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed || change.NeedsRestart {
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

func TestNewSubscriptionSaveDoesNotFetchOrOverwriteRuntimeMetadata(t *testing.T) {
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
	if _, err := manager.SetSubscription(context.Background(), server.URL); err != nil {
		t.Fatal(err)
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
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, _ := subscriptions.FindProfile(&catalog, active)
	if len(profile.Sources) != 1 || profile.Sources[0].URL != server.URL {
		t.Fatalf("source was not saved: %#v", profile.Sources)
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
	_, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	if err := manager.subscriptions.Update(func(stored *subscriptions.Catalog) error {
		profile, findErr := subscriptions.FindProfile(stored, active)
		if findErr != nil {
			return findErr
		}
		profile.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true, URL: server.URL, UserAgent: subscriptions.DefaultUserAgent, FetchMode: subscriptions.FetchAuto}}
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
	if !document.Subscription.LastCheck.After(oldCheck) || document.Subscription.LastResult != "update failed" {
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
