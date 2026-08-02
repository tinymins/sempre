package state

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/layout"
)

func TestStoreInitializesAndPersists(t *testing.T) {
	t.Parallel()
	paths := layout.At(t.TempDir())
	store := New(paths)
	if err := store.Initialize(); err != nil {
		t.Fatal(err)
	}
	if err := store.Update(func(document *Document) error {
		document.Subscription.URL = "https://example.com/config.json?token=secret"
		coreState := document.Core("sing-box")
		coreState.Installed["1.2.3"] = &Installation{}
		coreState.Channels["stable"] = "1.2.3"
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	document, err := store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Schema != SchemaVersion {
		t.Fatalf("schema = %d", document.Schema)
	}
	if document.Subscription.Interval != "24h" {
		t.Fatalf("interval = %q", document.Subscription.Interval)
	}
	if got := document.Cores["sing-box"].Channels["stable"]; got != "1.2.3" {
		t.Fatalf("stable = %q", got)
	}
	info, err := os.Stat(paths.State)
	if err != nil {
		t.Fatal(err)
	}
	if runtime.GOOS != "windows" && info.Mode().Perm()&0o077 != 0 {
		t.Fatalf("state permissions = %o", info.Mode().Perm())
	}
	if _, err := os.Stat(paths.State + ".previous"); !os.IsNotExist(err) {
		t.Fatalf("state backup was left behind: %v", err)
	}
}

func TestStoreMigratesSelectedCoreFromSchemaOne(t *testing.T) {
	t.Parallel()
	paths := layout.At(t.TempDir())
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	legacy := map[string]any{
		"schema": 1,
		"active": map[string]any{
			"core":        "sing-box",
			"ref":         "1.2.3",
			"version":     "1.2.3",
			"config_hash": strings.Repeat("a", 64),
		},
		"pending": false,
		"cores": map[string]any{
			"sing-box": map[string]any{
				"channels": map[string]any{"stable": "1.2.3"},
				"installed": map[string]any{
					"1.2.3": map[string]any{},
				},
			},
		},
		"configs":      map[string]any{"sing-box": strings.Repeat("a", 64)},
		"subscription": map[string]any{"interval": "24h"},
		"runtime":      map[string]any{},
	}
	data, err := json.Marshal(legacy)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(paths.State, data, 0o600); err != nil {
		t.Fatal(err)
	}
	store := New(paths)
	document, err := store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Schema != SchemaVersion ||
		document.Selected == nil ||
		document.Selected.Core != "sing-box" ||
		document.Selected.Ref != "1.2.3" {
		t.Fatalf("migrated document = %#v", document)
	}
}

func TestInitializeRecoversValidPreviousState(t *testing.T) {
	t.Parallel()
	paths := layout.At(t.TempDir())
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	document := NewDocument()
	document.Subscription.Interval = "12h"
	data, err := json.Marshal(document)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(paths.State+".previous", data, 0o600); err != nil {
		t.Fatal(err)
	}
	store := New(paths)
	if err := store.Initialize(); err != nil {
		t.Fatal(err)
	}
	recovered, err := store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if recovered.Subscription.Interval != "12h" {
		t.Fatalf("interval = %q", recovered.Subscription.Interval)
	}
	if _, err := os.Stat(paths.State + ".previous"); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("previous state was not consumed: %v", err)
	}
}

func TestStoreRejectsUnsafePersistedPaths(t *testing.T) {
	t.Parallel()
	paths := layout.At(t.TempDir())
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	document := NewDocument()
	document.Cores["../escape"] = &CoreState{
		Channels:  map[string]string{},
		Installed: map[string]*Installation{},
	}
	data, err := json.Marshal(document)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(paths.State, data, 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := New(paths).Read(); err == nil || !strings.Contains(err.Error(), "invalid core ID") {
		t.Fatalf("unsafe state error = %v", err)
	}
}

func TestDocumentRejectsInvalidConfigurationHash(t *testing.T) {
	t.Parallel()
	document := NewDocument()
	coreState := document.Core("sing-box")
	coreState.Installed["1.2.3"] = &Installation{}
	coreState.Channels["stable"] = "1.2.3"
	document.Configs["sing-box"] = "short"
	if err := document.Validate(); err == nil || !strings.Contains(err.Error(), "invalid configuration hash") {
		t.Fatalf("invalid hash error = %v", err)
	}
}

func TestOperationLeaseSerializesWriters(t *testing.T) {
	t.Parallel()
	store := New(layout.At(t.TempDir()))
	if err := store.Initialize(); err != nil {
		t.Fatal(err)
	}
	first, err := store.AcquireOperation()
	if err != nil {
		t.Fatal(err)
	}
	started := make(chan struct{})
	acquired := make(chan error, 1)
	go func() {
		close(started)
		second, err := store.AcquireOperation()
		if err == nil {
			second.Release()
		}
		acquired <- err
	}()
	<-started
	select {
	case err := <-acquired:
		first.Release()
		t.Fatalf("second lease did not block: %v", err)
	case <-time.After(50 * time.Millisecond):
	}
	first.Release()
	select {
	case err := <-acquired:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("second lease did not acquire after release")
	}
}

func TestStagePreservesLastKnownGoodDeployment(t *testing.T) {
	t.Parallel()
	document := NewDocument()
	first := Deployment{Core: "sing-box", Ref: "stable", Version: "1.2.3", ConfigHash: "old"}
	second := Deployment{Core: "sing-box", Ref: "stable", Version: "1.2.4", ConfigHash: "new"}
	third := Deployment{Core: "sing-box", Ref: "stable", Version: "1.2.4", ConfigHash: "newer"}
	document.Active = &first
	document.Stage(second)
	document.Stage(third)
	if !SameDeployment(document.Previous, &first) {
		t.Fatalf("previous = %#v, want %#v", document.Previous, first)
	}
	if !SameDeployment(document.Active, &third) {
		t.Fatalf("active = %#v, want %#v", document.Active, third)
	}
}

func TestWriteAtomicReplacesFile(t *testing.T) {
	t.Parallel()
	path := filepath.Join(t.TempDir(), "state.json")
	if err := os.WriteFile(path, []byte("before"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := WriteAtomic(path, []byte("after"), 0o600); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "after" {
		t.Fatalf("content = %q", data)
	}
}

func TestInstanceLeaseIsExclusive(t *testing.T) {
	t.Parallel()
	store := New(layout.At(t.TempDir()))
	if err := store.Initialize(); err != nil {
		t.Fatal(err)
	}
	first, err := store.AcquireInstance()
	if err != nil {
		t.Fatal(err)
	}
	if _, err := store.AcquireInstance(); err == nil {
		t.Fatal("second instance lease succeeded")
	}
	running, err := store.InstanceRunning()
	if err != nil {
		t.Fatal(err)
	}
	if !running {
		t.Fatal("held instance lock was reported as free")
	}
	first.Release()
	second, err := store.AcquireInstance()
	if err != nil {
		t.Fatal(err)
	}
	second.Release()
}
