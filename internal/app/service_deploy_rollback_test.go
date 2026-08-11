package app

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
)

func TestSwapRollbackRestoresTarget(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	target := filepath.Join(root, "state.json")
	staged := filepath.Join(root, "staged.json")
	if err := os.WriteFile(target, []byte("old"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(staged, []byte("new"), 0o600); err != nil {
		t.Fatal(err)
	}
	operation := &swapOperation{staged: staged, target: target}
	if err := operation.activate(); err != nil {
		t.Fatal(err)
	}
	if err := operation.rollback(); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "old" {
		t.Fatalf("target = %q", data)
	}
}

func TestSwapPreservesBackupWhenActivationAndRestoreFail(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	target := filepath.Join(root, "state.json")
	staged := filepath.Join(root, "staged.json")
	if err := os.WriteFile(target, []byte("old"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(staged, []byte("new"), 0o600); err != nil {
		t.Fatal(err)
	}
	operation := &swapOperation{
		staged: staged,
		target: target,
		rename: func(source, destination string) error {
			if source == target {
				return os.Rename(source, destination)
			}
			return errors.New("injected rename failure")
		},
	}
	if err := operation.activate(); err == nil || !operation.needsRestore {
		t.Fatalf("activation error = %v, operation = %#v", err, operation)
	}
	operation.cleanup()
	if _, err := os.Stat(operation.backup); err != nil {
		t.Fatalf("recovery backup was removed: %v", err)
	}
	operation.rename = os.Rename
	if err := operation.rollback(); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(target)
	if err != nil || string(data) != "old" {
		t.Fatalf("restored target = %q, %v", data, err)
	}
}

func TestRecoverExecutableBackupUsesNewestRegularFile(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	target := filepath.Join(root, "sempre.exe")
	older := filepath.Join(root, ".sempre-backup-older")
	newer := filepath.Join(root, ".sempre-backup-newer")
	if err := os.WriteFile(older, []byte("older"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(newer, []byte("newer"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(filepath.Join(root, ".sempre-backup-directory"), 0o700); err != nil {
		t.Fatal(err)
	}
	now := time.Now()
	if err := os.Chtimes(older, now.Add(-time.Minute), now.Add(-time.Minute)); err != nil {
		t.Fatal(err)
	}
	if err := os.Chtimes(newer, now, now); err != nil {
		t.Fatal(err)
	}
	if err := recoverExecutableBackup(target); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(target)
	if err != nil || string(data) != "newer" {
		t.Fatalf("recovered executable = %q, %v", data, err)
	}
	if _, err := os.Stat(older); err != nil {
		t.Fatalf("older backup was removed: %v", err)
	}
}

func TestRecoverExecutableBackupDoesNotReplaceExistingTarget(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	target := filepath.Join(root, "sempre.exe")
	backup := filepath.Join(root, ".sempre-backup-existing")
	if err := os.WriteFile(target, []byte("current"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(backup, []byte("backup"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := recoverExecutableBackup(target); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(target)
	if err != nil || string(data) != "current" {
		t.Fatalf("existing executable = %q, %v", data, err)
	}
	if _, err := os.Stat(backup); err != nil {
		t.Fatalf("backup was removed: %v", err)
	}
}

func TestRecoveredExecutableBecomesRollbackBaseline(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	target := filepath.Join(root, "sempre.exe")
	backup := filepath.Join(root, ".sempre-backup-interrupted")
	staged := filepath.Join(root, ".sempre-bin-new")
	if err := os.WriteFile(backup, []byte("old"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(staged, []byte("new"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := recoverExecutableBackup(target); err != nil {
		t.Fatal(err)
	}
	operation := &swapOperation{staged: staged, target: target}
	if err := operation.activate(); err != nil {
		t.Fatal(err)
	}
	if err := operation.rollback(); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(target)
	if err != nil || string(data) != "old" {
		t.Fatalf("rolled back executable = %q, %v", data, err)
	}
}

func TestRollbackUsesCleanupContextAfterCancellation(t *testing.T) {
	t.Parallel()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	controller := &recordingService{state: service.Stopped}
	cause := errors.New("deployment failed")
	err := rollbackDeployment(ctx, controller, nil, service.Running, false, layout.SystemAt(t.TempDir()), cause)
	if !errors.Is(err, cause) {
		t.Fatalf("rollback error = %v", err)
	}
	if controller.startContextErr != nil {
		t.Fatalf("cleanup context was canceled: %v", controller.startContextErr)
	}
}
