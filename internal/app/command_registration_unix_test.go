//go:build !windows

package app

import (
	"errors"
	"os"
	"path/filepath"
	"testing"

	"github.com/tinymins/sempre/internal/layout"
)

func TestCommandRegistrationLifecycle(t *testing.T) {
	t.Parallel()
	paths := layout.SystemAt(t.TempDir())
	if err := os.MkdirAll(filepath.Dir(paths.ServiceExecutable), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(paths.ServiceExecutable, []byte("sempre"), 0o755); err != nil {
		t.Fatal(err)
	}

	rollback, err := registerCommand(paths)
	if err != nil {
		t.Fatal(err)
	}
	if err := checkCommandRegistration(paths); err != nil {
		t.Fatal(err)
	}
	secondRollback, err := registerCommand(paths)
	if err != nil {
		t.Fatal(err)
	}
	if err := secondRollback(); err != nil {
		t.Fatal(err)
	}
	if err := checkCommandRegistration(paths); err != nil {
		t.Fatalf("idempotent rollback removed the existing registration: %v", err)
	}
	if err := rollback(); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Lstat(paths.CommandExecutable); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("command registration still exists: %v", err)
	}
}

func TestCommandRegistrationRefusesConflictingPath(t *testing.T) {
	t.Parallel()
	paths := layout.SystemAt(t.TempDir())
	if err := os.MkdirAll(filepath.Dir(paths.CommandExecutable), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(paths.CommandExecutable, []byte("other"), 0o755); err != nil {
		t.Fatal(err)
	}

	if _, err := registerCommand(paths); err == nil {
		t.Fatal("conflicting command path was overwritten")
	}
	data, err := os.ReadFile(paths.CommandExecutable)
	if err != nil || string(data) != "other" {
		t.Fatalf("conflicting command changed: %q, %v", data, err)
	}
	if err := unregisterCommand(paths); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(paths.CommandExecutable); err != nil {
		t.Fatalf("uninstall removed a command it did not own: %v", err)
	}
}

func TestUnregisterLeavesForeignSymlink(t *testing.T) {
	t.Parallel()
	paths := layout.SystemAt(t.TempDir())
	if err := os.MkdirAll(filepath.Dir(paths.CommandExecutable), 0o755); err != nil {
		t.Fatal(err)
	}
	foreign := filepath.Join(t.TempDir(), "foreign")
	if err := os.Symlink(foreign, paths.CommandExecutable); err != nil {
		t.Fatal(err)
	}
	if err := unregisterCommand(paths); err != nil {
		t.Fatal(err)
	}
	target, err := os.Readlink(paths.CommandExecutable)
	if err != nil || target != foreign {
		t.Fatalf("foreign symlink changed: %q, %v", target, err)
	}
}
