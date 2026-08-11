package app

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/state"
	"github.com/tinymins/sempre/internal/webconfig"
)

type recordingService struct {
	state           service.State
	calls           []string
	failStarts      int
	startContextErr error
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

func (controller *recordingService) Start(ctx context.Context) error {
	controller.calls = append(controller.calls, "start")
	controller.startContextErr = ctx.Err()
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
	extra := target.CoreVersionDir("sing-box", "", "9.9.9")
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
	if err := commitSwaps(operations); err != nil {
		t.Fatal(err)
	}

	for _, path := range []string{
		target.CoreBinary("sing-box", "", "1.2.3"),
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
	extra := target.CoreVersionDir("sing-box", "", "9.9.9")
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
	if err := commitSwaps(operations); err != nil {
		t.Fatal(err)
	}

	if _, err := os.Stat(target.CoreBinary("sing-box", "", "1.2.3")); err != nil {
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
	if err := os.MkdirAll(target.CoreVersionDir("sing-box", "", "1.2.3"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target.CoreBinary("sing-box", "", "1.2.3"), []byte("fake"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(target.Runtime, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := source.store.Update(func(document *state.Document) error {
		document.Configs["sing-box"] = testHashA
		document.Active = &state.Deployment{
			Core:       "sing-box",
			Ref:        "stable",
			Version:    "1.2.3",
			ConfigHash: testHashA,
		}
		document.Runtime = state.Runtime{State: "running", PID: 123}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if err := state.WriteAtomic(source.paths.Config("sing-box", testHashA), []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := state.WriteAtomic(source.paths.Config("sing-box", testHashB), []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := state.WriteAtomic(filepath.Join(source.paths.Subscriptions, "source-marker"), []byte("source"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(target.Subscriptions, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := state.WriteAtomic(filepath.Join(target.Subscriptions, "stale-marker"), []byte("stale"), 0o600); err != nil {
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
	if err := commitSwaps(operations); err != nil {
		t.Fatal(err)
	}

	deployed, err := state.New(target).Read()
	if err != nil {
		t.Fatal(err)
	}
	if deployed.Runtime != (state.Runtime{}) {
		t.Fatalf("runtime = %#v", deployed.Runtime)
	}
	if _, err := os.Stat(target.Config("sing-box", testHashA)); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(target.Config("sing-box", testHashB)); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("unused config was copied: %v", err)
	}
	if _, err := os.Stat(filepath.Join(target.Subscriptions, "source-marker")); err != nil {
		t.Fatalf("subscription data was not copied: %v", err)
	}
	if _, err := os.Stat(filepath.Join(target.Subscriptions, "stale-marker")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("stale subscription data was retained: %v", err)
	}
}

func TestDataDeploymentCopiesWebAndCurrentUI(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	if err := os.MkdirAll(target.CoreVersionDir("sing-box", "", "1.2.3"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target.CoreBinary("sing-box", "", "1.2.3"), []byte("fake"), 0o700); err != nil {
		t.Fatal(err)
	}
	if _, err := source.web.SetPassword("administrator"); err != nil {
		t.Fatal(err)
	}
	if _, err := source.web.Update(func(config *webconfig.Config) error {
		config.Listen = "127.0.0.1:44111"
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	writeTestUI(t, source.paths.UICurrent, "Custom Console")

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
	if err := commitSwaps(operations); err != nil {
		t.Fatal(err)
	}

	web, err := webconfig.New(target.WebConfig).Read()
	if err != nil {
		t.Fatal(err)
	}
	if web.Listen != "127.0.0.1:44111" || web.Password == "" {
		t.Fatalf("web config was not copied: %#v", web)
	}
	if _, err := os.Stat(filepath.Join(target.UICurrent, "index.html")); err != nil {
		t.Fatal(err)
	}
}
