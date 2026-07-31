package app

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/sempre-lab/sempre/internal/layout"
	"github.com/sempre-lab/sempre/internal/state"
)

func TestBootstrapSystemDataCopiesDeploymentOnly(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	if err := source.store.Update(func(document *state.Document) error {
		document.Runtime = state.Runtime{State: "running", PID: 123}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if err := state.WriteAtomic(source.paths.Config("sing-box", "hash"), []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(source.paths.ManagerLog, []byte("do not copy"), 0o600); err != nil {
		t.Fatal(err)
	}

	target := layout.SystemAt(t.TempDir())
	if err := bootstrapSystemData(source, target); err != nil {
		t.Fatal(err)
	}
	store := state.New(target)
	document, err := store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Runtime != (state.Runtime{}) {
		t.Fatalf("runtime = %#v", document.Runtime)
	}
	if _, err := os.Stat(target.CoreBinary("sing-box", "1.2.3")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(target.Config("sing-box", "hash")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(target.ManagerLog); !os.IsNotExist(err) {
		t.Fatalf("log was copied: %v", err)
	}
}

func TestBootstrapSystemDataDoesNotReplaceExistingState(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	store := state.New(target)
	if err := store.Initialize(); err != nil {
		t.Fatal(err)
	}
	if err := store.Update(func(document *state.Document) error {
		document.LastError = "retain"
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if err := bootstrapSystemData(source, target); err != nil {
		t.Fatal(err)
	}
	document, err := store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.LastError != "retain" {
		t.Fatalf("state was replaced: %#v", document)
	}
}

func TestInstallExecutableUsesProtectedTarget(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	source := filepath.Join(root, "source")
	if err := os.WriteFile(source, []byte("sempre"), 0o700); err != nil {
		t.Fatal(err)
	}
	target := layout.SystemAt(filepath.Join(root, "system"))
	if err := installExecutable(source, target); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(target.ServiceExecutable)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "sempre" {
		t.Fatalf("executable = %q", data)
	}
}
