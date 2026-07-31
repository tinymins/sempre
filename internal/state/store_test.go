package state

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/sempre-lab/sempre/internal/layout"
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
		document.Core("sing-box").Channels["stable"] = "1.2.3"
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
	first.Release()
	second, err := store.AcquireInstance()
	if err != nil {
		t.Fatal(err)
	}
	second.Release()
}
