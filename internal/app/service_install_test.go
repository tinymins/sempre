package app

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/sempre-lab/sempre/internal/layout"
	"github.com/sempre-lab/sempre/internal/service"
	"github.com/sempre-lab/sempre/internal/state"
)

type recordingService struct {
	state      service.State
	calls      []string
	failStarts int
}

func (controller *recordingService) Install(context.Context, string, string) error {
	controller.calls = append(controller.calls, "install")
	if controller.state == service.NotInstalled {
		controller.state = service.Stopped
	}
	return nil
}

func (controller *recordingService) Uninstall(context.Context) error {
	controller.calls = append(controller.calls, "uninstall")
	controller.state = service.NotInstalled
	return nil
}

func (controller *recordingService) Start(context.Context) error {
	controller.calls = append(controller.calls, "start")
	if controller.failStarts > 0 {
		controller.failStarts--
		return errors.New("start failed")
	}
	controller.state = service.Running
	return nil
}

func (controller *recordingService) Stop(context.Context) error {
	controller.calls = append(controller.calls, "stop")
	controller.state = service.Stopped
	return nil
}

func (controller *recordingService) Restart(ctx context.Context) error {
	if err := controller.Stop(ctx); err != nil {
		return err
	}
	return controller.Start(ctx)
}

func (controller *recordingService) Status(context.Context) (service.State, error) {
	controller.calls = append(controller.calls, "status")
	return controller.state, nil
}

func (controller *recordingService) Run(ctx context.Context, daemon func(context.Context) error) error {
	return daemon(ctx)
}

func TestCoreDeploymentMergesManagedVersions(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	extra := target.CoreVersionDir("sing-box", "9.9.9")
	if err := os.MkdirAll(extra, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(extra, "sing-box"), []byte("extra"), 0o700); err != nil {
		t.Fatal(err)
	}

	document, err := source.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	operations, err := source.stageDeployment(context.Background(), target, DeployCore, document)
	if err != nil {
		t.Fatal(err)
	}
	defer cleanupStaged(operations)
	if err := activateSwaps(operations); err != nil {
		t.Fatal(err)
	}
	commitSwaps(operations)

	for _, path := range []string{
		target.CoreBinary("sing-box", "1.2.3"),
		filepath.Join(extra, "sing-box"),
	} {
		if _, err := os.Stat(path); err != nil {
			t.Fatalf("%s: %v", path, err)
		}
	}
}

func TestAllDeploymentReplacesExtraCoreAndKeepsLogs(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	extra := target.CoreVersionDir("sing-box", "9.9.9")
	if err := os.MkdirAll(extra, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(target.Logs, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target.ManagerLog, []byte("retain"), 0o600); err != nil {
		t.Fatal(err)
	}

	document, err := source.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	operations, err := source.stageDeployment(context.Background(), target, DeployAll, document)
	if err != nil {
		t.Fatal(err)
	}
	defer cleanupStaged(operations)
	if err := activateSwaps(operations); err != nil {
		t.Fatal(err)
	}
	commitSwaps(operations)

	if _, err := os.Stat(target.CoreBinary("sing-box", "1.2.3")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(extra); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("extra core was retained: %v", err)
	}
	data, err := os.ReadFile(target.ManagerLog)
	if err != nil || string(data) != "retain" {
		t.Fatalf("log changed: %q, %v", data, err)
	}
}

func TestDataDeploymentRequiresSystemCores(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	document, err := source.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if _, err := source.stageDeployment(context.Background(), target, DeployData, document); err == nil ||
		!strings.Contains(err.Error(), "is required by data deployment") {
		t.Fatalf("missing core error = %v", err)
	}
}

func TestDataDeploymentCopiesStateAndReferencedConfigsOnly(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	if err := os.MkdirAll(target.CoreVersionDir("sing-box", "1.2.3"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target.CoreBinary("sing-box", "1.2.3"), []byte("fake"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(target.Runtime, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := source.store.Update(func(document *state.Document) error {
		document.Configs["sing-box"] = "current"
		document.Active = &state.Deployment{
			Core:       "sing-box",
			Ref:        "stable",
			Version:    "1.2.3",
			ConfigHash: "current",
		}
		document.Runtime = state.Runtime{State: "running", PID: 123}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if err := state.WriteAtomic(source.paths.Config("sing-box", "current"), []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := state.WriteAtomic(source.paths.Config("sing-box", "unused"), []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}

	document, err := source.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	operations, err := source.stageDeployment(context.Background(), target, DeployData, document)
	if err != nil {
		t.Fatal(err)
	}
	defer cleanupStaged(operations)
	if err := activateSwaps(operations); err != nil {
		t.Fatal(err)
	}
	commitSwaps(operations)

	deployed, err := state.New(target).Read()
	if err != nil {
		t.Fatal(err)
	}
	if deployed.Runtime != (state.Runtime{}) {
		t.Fatalf("runtime = %#v", deployed.Runtime)
	}
	if _, err := os.Stat(target.Config("sing-box", "current")); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(target.Config("sing-box", "unused")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("unused config was copied: %v", err)
	}
}

func TestEmptySystemStateDoesNotRequireConfirmation(t *testing.T) {
	t.Parallel()
	empty := state.NewDocument()
	if meaningfulState(empty) {
		t.Fatalf("empty state is meaningful: %#v", empty)
	}
	empty.LastError = "old diagnostic"
	if meaningfulState(empty) {
		t.Fatalf("diagnostics made empty state meaningful: %#v", empty)
	}
}

func TestMeaningfulSystemStateRequiresConfirmation(t *testing.T) {
	t.Parallel()
	document := state.NewDocument()
	document.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
	if !meaningfulState(document) {
		t.Fatal("selected system state was treated as empty")
	}
	summary := deploymentReplacementSummary(document)
	if !strings.Contains(summary, "sing-box@stable") {
		t.Fatalf("summary = %q", summary)
	}
}

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

func TestCoreDeployRestoresRunningService(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	controller := &recordingService{state: service.Running}
	source.service = controller

	if err := source.deployToSystem(context.Background(), target, DeployCore, false, false); err != nil {
		t.Fatal(err)
	}
	if controller.state != service.Running {
		t.Fatalf("service state = %s", controller.state)
	}
	if strings.Join(controller.calls, ",") != "status,stop,start" {
		t.Fatalf("service calls = %v", controller.calls)
	}
	if _, err := os.Stat(target.CoreBinary("sing-box", "1.2.3")); err != nil {
		t.Fatal(err)
	}
}

func TestFailedDeployRestoresFilesAndRunningService(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	if err := os.MkdirAll(target.CoreVersionDir("sing-box", "1.2.3"), 0o700); err != nil {
		t.Fatal(err)
	}
	targetBinary := target.CoreBinary("sing-box", "1.2.3")
	if err := os.WriteFile(targetBinary, []byte("old"), 0o700); err != nil {
		t.Fatal(err)
	}
	controller := &recordingService{state: service.Running, failStarts: 1}
	source.service = controller

	if err := source.deployToSystem(context.Background(), target, DeployCore, false, false); err == nil {
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
