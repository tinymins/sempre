package app

import (
	"context"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

type observedAdapter struct {
	fakeAdapter
	validations *atomic.Int32
	validation  error
}

func (adapter observedAdapter) Validate(context.Context, string, string, string, io.Writer, io.Writer) error {
	adapter.validations.Add(1)
	return adapter.validation
}

func TestSubscriptionSavePersistsDraftWithoutRuntimeWork(t *testing.T) {
	manager := readyRuntimeManager(t)
	var requests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		requests.Add(1)
		_, _ = writer.Write([]byte(testSubscription))
	}))
	defer server.Close()
	var validations atomic.Int32
	manager.registry = core.NewRegistry(observedAdapter{validations: &validations})

	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		t.Fatal(err)
	}
	beforeRevision := profile.Revision
	profile.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true, URL: server.URL}}
	profile.Editor.CustomConfig = "{ temporarily invalid JSONC"
	profile.TransparentProxy.TUN.InterfaceName = " "

	started := time.Now()
	change, rendered, err := manager.SaveSubscriptionProfile(context.Background(), active, *profile)
	if err != nil {
		t.Fatal(err)
	}
	if elapsed := time.Since(started); elapsed > 200*time.Millisecond {
		t.Fatalf("save took %s", elapsed)
	}
	if requests.Load() != 0 || validations.Load() != 0 {
		t.Fatalf("save performed runtime work: HTTP=%d validations=%d", requests.Load(), validations.Load())
	}
	if !change.Changed || !change.NeedsRestart || rendered.Content != "" {
		t.Fatalf("save result = %#v, render = %#v", change, rendered)
	}

	stored, _, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	saved, err := subscriptions.FindProfile(&stored, active)
	if err != nil {
		t.Fatal(err)
	}
	if saved.Revision != beforeRevision+1 || saved.Editor.CustomConfig != profile.Editor.CustomConfig || saved.TransparentProxy.TUN.InterfaceName != " " {
		t.Fatalf("saved profile = %#v", saved)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Active == nil || document.Active.ConfigHash != testHashA || document.Pending {
		t.Fatalf("save replaced the old deployment: %#v", document)
	}
	status, err := manager.ManagedRuntimeStatus()
	if err != nil {
		t.Fatal(err)
	}
	if !status.Pending {
		t.Fatalf("saved revision was not marked pending: %#v", status)
	}
}

func TestSubscriptionSaveDoesNotContactUnreachableSource(t *testing.T) {
	manager := newTestManager(t)
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, _ := subscriptions.FindProfile(&catalog, active)
	profile.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true, URL: "http://127.0.0.1:1/unreachable"}}

	started := time.Now()
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, *profile); err != nil {
		t.Fatal(err)
	}
	if elapsed := time.Since(started); elapsed > 200*time.Millisecond {
		t.Fatalf("unreachable source delayed save by %s", elapsed)
	}
}

func TestExplicitSubscriptionRefreshStillFetches(t *testing.T) {
	manager := newTestManager(t)
	var requests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		requests.Add(1)
		_, _ = writer.Write([]byte(testSubscription))
	}))
	defer server.Close()
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, _ := subscriptions.FindProfile(&catalog, active)
	profile.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true, URL: server.URL}}
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, *profile); err != nil {
		t.Fatal(err)
	}
	if requests.Load() != 0 {
		t.Fatalf("save fetched the subscription %d time(s)", requests.Load())
	}
	if _, _, err := manager.RefreshSubscriptionProfile(context.Background(), active); err != nil {
		t.Fatal(err)
	}
	if requests.Load() == 0 {
		t.Fatal("explicit refresh did not fetch the subscription")
	}
	refreshed, _, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, _ = subscriptions.FindProfile(&refreshed, active)
	if profile.Sources[0].SnapshotHash == "" {
		t.Fatalf("refresh did not persist the local snapshot: %#v", profile.Sources[0])
	}
	requests.Store(0)
	profile.Remark = "compile the next revision from the local snapshot"
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, *profile); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.ManagedRuntimeAction(RuntimeRestart); err != nil {
		t.Fatal(err)
	}
	if requests.Load() != 0 {
		t.Fatalf("runtime preparation fetched the subscription %d time(s)", requests.Load())
	}
}

func TestRuntimeRestartCompilesLatestSavedRevisionBeforeStaging(t *testing.T) {
	manager := readyRuntimeManager(t)
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, _ := subscriptions.FindProfile(&catalog, active)
	profile.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: testSubscription}}
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, *profile); err != nil {
		t.Fatal(err)
	}
	stored, _, _, _, _ := manager.SubscriptionCatalog()
	saved, _ := subscriptions.FindProfile(&stored, active)
	before, _ := manager.store.Read()
	if before.Active == nil || before.Active.ConfigHash != testHashA || before.Pending {
		t.Fatalf("save staged before restart: %#v", before)
	}

	if _, err := manager.ManagedRuntimeAction(RuntimeRestart); err != nil {
		t.Fatal(err)
	}
	after, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	build := after.ConfigBuilds["sing-box"]
	if build.ProfileRevision != saved.Revision || build.ProfileID != active {
		t.Fatalf("staged build = %#v, saved revision = %d", build, saved.Revision)
	}
	if after.Active == nil || after.Active.ConfigHash == testHashA || !after.Pending || after.Previous == nil || after.Previous.ConfigHash != testHashA {
		t.Fatalf("latest revision was not staged transactionally: %#v", after)
	}
}

func TestRuntimePreparationFailurePreservesSavedDraftAndActiveDeployment(t *testing.T) {
	for _, test := range []struct {
		name      string
		configure func(*subscriptions.Profile, *Manager, *atomic.Int32)
		message   string
	}{
		{
			name: "compile",
			configure: func(profile *subscriptions.Profile, _ *Manager, _ *atomic.Int32) {
				profile.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: testSubscription}}
				profile.Editor.CustomConfig = "{ invalid JSONC"
			},
			message: "JSONC",
		},
		{
			name: "core validation",
			configure: func(profile *subscriptions.Profile, manager *Manager, validations *atomic.Int32) {
				profile.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: testSubscription}}
				manager.registry = core.NewRegistry(observedAdapter{validations: validations, validation: errors.New("core rejected latest config")})
			},
			message: "core rejected latest config",
		},
	} {
		t.Run(test.name, func(t *testing.T) {
			manager := readyRuntimeManager(t)
			catalog, active, _, _, err := manager.SubscriptionCatalog()
			if err != nil {
				t.Fatal(err)
			}
			profile, _ := subscriptions.FindProfile(&catalog, active)
			var validations atomic.Int32
			test.configure(profile, manager, &validations)
			if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, *profile); err != nil {
				t.Fatal(err)
			}
			if validations.Load() != 0 {
				t.Fatal("core validation ran during save")
			}

			if _, err := manager.ManagedRuntimeAction(RuntimeRestart); err == nil || !strings.Contains(err.Error(), test.message) {
				t.Fatalf("restart error = %v", err)
			}
			document, err := manager.store.Read()
			if err != nil {
				t.Fatal(err)
			}
			if document.Active == nil || document.Active.ConfigHash != testHashA || document.Pending || document.Previous != nil {
				t.Fatalf("failed preparation damaged active deployment: %#v", document)
			}
			if document.Runtime.LastError == "" || !strings.Contains(document.Runtime.LastError, test.message) {
				t.Fatalf("runtime error was not recorded: %#v", document.Runtime)
			}
			stored, _, _, _, err := manager.SubscriptionCatalog()
			if err != nil {
				t.Fatal(err)
			}
			saved, _ := subscriptions.FindProfile(&stored, active)
			if saved.Revision != profile.Revision+1 || saved.Editor.CustomConfig != profile.Editor.CustomConfig {
				t.Fatalf("failed preparation changed saved draft: %#v", saved)
			}
		})
	}
}

func TestSavedRevisionDoesNotChangeStateDeploymentUntilRuntimePreparation(t *testing.T) {
	manager := readyRuntimeManager(t)
	before, _ := manager.store.Read()
	catalog, active, _, _, _ := manager.SubscriptionCatalog()
	profile, _ := subscriptions.FindProfile(&catalog, active)
	profile.Remark = "draft only"
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, *profile); err != nil {
		t.Fatal(err)
	}
	after, _ := manager.store.Read()
	if !state.SameDeployment(before.Active, after.Active) || before.Configs["sing-box"] != after.Configs["sing-box"] {
		t.Fatalf("save changed deployment: before=%#v after=%#v", before, after)
	}
}
