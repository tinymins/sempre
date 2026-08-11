package app

import (
	"bytes"
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/state"
)

func TestSystemInstallSucceedsWithoutBundledUI(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	controller := &recordingService{state: service.NotInstalled}
	source.service = controller

	if err := source.deployToSystem(context.Background(), target, DeployAll, true, true, false); err != nil {
		t.Fatal(err)
	}
	if controller.state != service.Running {
		t.Fatalf("service state = %s", controller.state)
	}
	if strings.Join(controller.calls, ",") != "status,install,start" {
		t.Fatalf("service calls = %v", controller.calls)
	}
	if err := source.commands.Check(target); err != nil {
		t.Fatalf("command registration: %v", err)
	}
}

func TestBundleSnapshotInstallAllowsSelectedCoreWithoutActiveDeployment(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	controller := &recordingService{state: service.NotInstalled}
	source.service = controller

	if err := source.deployToSystem(context.Background(), target, DeployAll, true, true, true); err != nil {
		t.Fatal(err)
	}
	if controller.state != service.Running {
		t.Fatalf("service state = %s", controller.state)
	}
	document, err := state.New(target).Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Selected == nil || document.Selected.Core != "sing-box" {
		t.Fatalf("selected = %#v", document.Selected)
	}
	if document.Active != nil {
		t.Fatalf("active = %#v", document.Active)
	}
}

func TestCommandRegistrationFailureRollsBackSystemInstall(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	if err := os.MkdirAll(filepath.Dir(target.CommandExecutable), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target.CommandExecutable, []byte("other"), 0o755); err != nil {
		t.Fatal(err)
	}
	controller := &recordingService{state: service.NotInstalled}
	source.service = controller

	if err := source.deployToSystem(context.Background(), target, DeployAll, true, true, false); err == nil {
		t.Fatal("install succeeded despite a conflicting command path")
	}
	if controller.state != service.NotInstalled {
		t.Fatalf("service state = %s", controller.state)
	}
	if _, err := os.Stat(target.ServiceExecutable); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("service executable was not rolled back: %v", err)
	}
	data, err := os.ReadFile(target.CommandExecutable)
	if err != nil || string(data) != "other" {
		t.Fatalf("conflicting command changed: %q, %v", data, err)
	}
}

func TestServiceStartFailureRollsBackCommandRegistration(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	controller := &recordingService{state: service.NotInstalled, failStarts: 1}
	source.service = controller

	if err := source.deployToSystem(context.Background(), target, DeployAll, true, true, false); err == nil {
		t.Fatal("install succeeded despite service start failure")
	}
	if _, err := os.Lstat(target.CommandExecutable); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("command registration was not rolled back: %v", err)
	}
}

func TestInvalidBundledUIDoesNotRollBackSystemInstall(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	controller := &recordingService{state: service.NotInstalled}
	source.service = controller
	var errorsOutput bytes.Buffer
	source.errors = &errorsOutput
	if err := os.MkdirAll(target.Resources, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(target.Resources, "sempre-ui.zip"), []byte("invalid"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(target.Resources, "SHA256SUMS"), []byte("invalid  sempre-ui.zip\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	if err := source.deployToSystem(context.Background(), target, DeployAll, true, true, false); err != nil {
		t.Fatal(err)
	}
	if controller.state != service.Running {
		t.Fatalf("service state = %s", controller.state)
	}
	if !strings.Contains(errorsOutput.String(), "WARNING: install bundled UI") {
		t.Fatalf("warning output = %q", errorsOutput.String())
	}
}

func TestCoreDeployRestoresRunningService(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	controller := &recordingService{state: service.Running}
	source.service = controller

	if err := source.deployToSystem(context.Background(), target, DeployCore, false, false, false); err != nil {
		t.Fatal(err)
	}
	if controller.state != service.Running {
		t.Fatalf("service state = %s", controller.state)
	}
	if strings.Join(controller.calls, ",") != "status,stop,start" {
		t.Fatalf("service calls = %v", controller.calls)
	}
	if _, err := os.Stat(target.CoreBinary("sing-box", "", "1.2.3")); err != nil {
		t.Fatal(err)
	}
}

func TestFailedDeployRestoresFilesAndRunningService(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	if err := os.MkdirAll(target.CoreVersionDir("sing-box", "", "1.2.3"), 0o700); err != nil {
		t.Fatal(err)
	}
	targetBinary := target.CoreBinary("sing-box", "", "1.2.3")
	if err := os.WriteFile(targetBinary, []byte("old"), 0o700); err != nil {
		t.Fatal(err)
	}
	controller := &recordingService{state: service.Running, failStarts: 1}
	source.service = controller

	if err := source.deployToSystem(context.Background(), target, DeployCore, false, false, false); err == nil {
		t.Fatal("deployment succeeded despite service start failure")
	}
	data, err := os.ReadFile(targetBinary)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "old" {
		t.Fatalf("target core was not rolled back: %q", data)
	}
	if controller.state != service.Running {
		t.Fatalf("service state = %s", controller.state)
	}
}

func writeTestUI(t *testing.T, directory, name string) {
	t.Helper()
	if err := os.MkdirAll(directory, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(directory, "index.html"), []byte("<!doctype html><title>"+name+"</title>"), 0o600); err != nil {
		t.Fatal(err)
	}
	manifest := `{"schema":1,"name":"` + name + `","version":"1.0.0","entry":"index.html","api":{"major":1}}`
	if err := os.WriteFile(filepath.Join(directory, "sempre-ui.json"), []byte(manifest), 0o600); err != nil {
		t.Fatal(err)
	}
	metadata := `{"manifest":` + manifest + `,"source_type":"local","source":"test.zip","sha256":"` + strings.Repeat("a", 64) + `","installed_at":"2026-01-01T00:00:00Z"}`
	if err := os.WriteFile(filepath.Join(directory, ".sempre-source.json"), []byte(metadata), 0o600); err != nil {
		t.Fatal(err)
	}
}
