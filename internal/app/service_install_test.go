package app

import (
	"bytes"
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
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

func TestSubscriptionInstallationPreservesExistingCatalog(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	if _, err := source.CreateSubscriptionProfile("portable"); err != nil {
		t.Fatal(err)
	}
	target := layout.SystemAt(t.TempDir())
	targetManager, err := New(target, bytes.NewBuffer(nil), bytes.NewBuffer(nil))
	if err != nil {
		t.Fatal(err)
	}
	systemProfile, err := targetManager.CreateSubscriptionProfile("system")
	if err != nil {
		t.Fatal(err)
	}
	existing := state.NewDocument()
	existing.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
	operation, err := source.stageSubscriptionInstallation(target, existing, true)
	if err != nil {
		t.Fatal(err)
	}
	defer operation.cleanup()
	if err := operation.activate(); err != nil {
		t.Fatal(err)
	}
	if err := operation.commit(); err != nil {
		t.Fatal(err)
	}
	catalog, err := subscriptions.NewStore(target).Read()
	if err != nil {
		t.Fatal(err)
	}
	if _, err := subscriptions.FindProfile(&catalog, systemProfile.ID); err != nil {
		t.Fatalf("existing system profile was not preserved: %v", err)
	}
	for _, profile := range catalog.Profiles {
		if profile.Name == "portable" {
			t.Fatal("portable catalog replaced the existing system catalog")
		}
	}
}

func TestSubscriptionOnlyInstallationPreservesExistingCatalog(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	if _, err := source.CreateSubscriptionProfile("portable"); err != nil {
		t.Fatal(err)
	}
	target := layout.SystemAt(t.TempDir())
	targetManager, err := New(target, bytes.NewBuffer(nil), bytes.NewBuffer(nil))
	if err != nil {
		t.Fatal(err)
	}
	catalog, _, _, _, err := targetManager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	systemProfile := catalog.Profiles[0]
	systemProfile.Name = "system"
	if err := targetManager.subscriptions.Update(func(candidate *subscriptions.Catalog) error {
		candidate.Profiles[0] = systemProfile
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	existing := state.NewDocument()
	meaningful, err := source.meaningfulSubscriptionData(target, existing)
	if err != nil {
		t.Fatal(err)
	}
	if !meaningful {
		t.Fatal("configured subscription catalog was treated as empty without a selected core")
	}
	operation, err := source.stageSubscriptionInstallation(target, existing, meaningful)
	if err != nil {
		t.Fatal(err)
	}
	defer operation.cleanup()
	if err := operation.activate(); err != nil {
		t.Fatal(err)
	}
	if err := operation.commit(); err != nil {
		t.Fatal(err)
	}
	preserved, err := subscriptions.NewStore(target).Read()
	if err != nil {
		t.Fatal(err)
	}
	if len(preserved.Profiles) != 1 || preserved.Profiles[0].Name != "system" {
		t.Fatalf("subscription-only system catalog was not preserved: %#v", preserved.Profiles)
	}
}

func TestSubscriptionInstallationMigratesLegacySystemURL(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	existing := state.NewDocument()
	existing.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
	existing.Subscription.URL = "https://system.example/subscription"
	operation, err := source.stageSubscriptionInstallation(target, existing, true)
	if err != nil {
		t.Fatal(err)
	}
	defer operation.cleanup()
	if err := operation.activate(); err != nil {
		t.Fatal(err)
	}
	if err := operation.commit(); err != nil {
		t.Fatal(err)
	}
	catalog, err := subscriptions.NewStore(target).Read()
	if err != nil {
		t.Fatal(err)
	}
	if len(catalog.Profiles) != 1 || len(catalog.Profiles[0].Sources) != 1 ||
		catalog.Profiles[0].Sources[0].URL != existing.Subscription.URL {
		t.Fatalf("legacy subscription was not migrated: %#v", catalog.Profiles)
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

func TestInstallMergeCopiesDeploymentIntoEmptySystemState(t *testing.T) {
	t.Parallel()
	source := state.NewDocument()
	sourceState := source.Core("sing-box").Source("")
	sourceState.Channels["stable"] = "1.2.3"
	sourceState.Installed["1.2.3"] = &state.Installation{}
	source.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
	source.Active = &state.Deployment{
		Core:       "sing-box",
		Ref:        "stable",
		Version:    "1.2.3",
		ConfigHash: testHashA,
	}
	source.Configs["sing-box"] = testHashA
	source.ActiveProfileID = "portable-profile"
	source.AutoRestart = false

	merged := mergeInstallDocument(source, state.NewDocument(), false)
	if merged.Selected == nil || *merged.Selected != *source.Selected {
		t.Fatalf("selected = %#v", merged.Selected)
	}
	if merged.Active == nil || *merged.Active != *source.Active {
		t.Fatalf("active = %#v", merged.Active)
	}
	if merged.ActiveProfileID != source.ActiveProfileID || merged.AutoRestart != source.AutoRestart {
		t.Fatalf("subscription settings = %q, %t", merged.ActiveProfileID, merged.AutoRestart)
	}
}

func TestInstallMergeCopiesDeploymentButPreservesStoppedIntent(t *testing.T) {
	t.Parallel()
	source := state.NewDocument()
	sourceState := source.Core("sing-box").Source("")
	sourceState.Channels["stable"] = "1.2.3"
	sourceState.Installed["1.2.3"] = &state.Installation{}
	source.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
	source.Active = &state.Deployment{
		Core:       "sing-box",
		Ref:        "stable",
		Version:    "1.2.3",
		ConfigHash: testHashA,
	}
	source.Configs["sing-box"] = testHashA

	existing := state.NewDocument()
	existing.DesiredState = state.DesiredStopped
	merged := mergeInstallDocument(source, existing, false)
	if merged.Active == nil || *merged.Active != *source.Active {
		t.Fatalf("active = %#v", merged.Active)
	}
	if merged.DesiredState != state.DesiredStopped {
		t.Fatalf("desired state = %q", merged.DesiredState)
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
