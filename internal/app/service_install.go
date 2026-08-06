package app

import (
	"context"
	"errors"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/state"
)

type DeployComponent string

const (
	DeployAll  DeployComponent = "all"
	DeployCore DeployComponent = "core"
	DeployBin  DeployComponent = "bin"
	DeployData DeployComponent = "data"
)

type ConfirmationRequired struct {
	Summary string
}

func (required *ConfirmationRequired) Error() string {
	return "replacing the existing system deployment requires confirmation"
}

func ParseDeployComponent(value string) (DeployComponent, error) {
	component := DeployComponent(strings.ToLower(strings.TrimSpace(value)))
	switch component {
	case DeployAll, DeployCore, DeployBin, DeployData:
		return component, nil
	default:
		return "", fmt.Errorf("deploy component must be one of: all, core, bin, data")
	}
}

func (manager *Manager) installSystemService(ctx context.Context, allowReplace bool) error {
	systemPaths, err := layout.ForMode(layout.System)
	if err != nil {
		return err
	}
	return manager.deployToSystem(ctx, systemPaths, DeployAll, allowReplace, true, false)
}

func (manager *Manager) installBundleService(ctx context.Context, allowReplace bool) error {
	systemPaths, err := layout.ForMode(layout.System)
	if err != nil {
		return err
	}
	return manager.deployToSystem(ctx, systemPaths, DeployAll, allowReplace, true, true)
}

func (manager *Manager) deploySystemService(
	ctx context.Context,
	component DeployComponent,
	allowReplace bool,
) error {
	if manager.paths.Mode != layout.Portable {
		return fmt.Errorf("service deploy is only available in portable mode")
	}
	systemPaths, err := layout.ForMode(layout.System)
	if err != nil {
		return err
	}
	return manager.deployToSystem(ctx, systemPaths, component, allowReplace, false, false)
}

func (manager *Manager) systemManager() (*Manager, error) {
	if manager.paths.Mode == layout.System {
		return manager, nil
	}
	paths, err := layout.ForMode(layout.System)
	if err != nil {
		return nil, err
	}
	if _, err := os.Stat(paths.State); errors.Is(err, os.ErrNotExist) {
		return nil, fmt.Errorf("system deployment is not initialized; run 'sempre service install' first")
	} else if err != nil {
		return nil, err
	}
	return New(paths, manager.output, manager.errors)
}

func (manager *Manager) deployToSystem(
	ctx context.Context,
	target layout.Layout,
	component DeployComponent,
	allowReplace bool,
	install bool,
	snapshot bool,
) error {
	var configLease *state.Lease
	if component == DeployAll || component == DeployData {
		var err error
		configLease, err = manager.store.AcquireConfig()
		if err != nil {
			return err
		}
		defer configLease.Release()
	}
	sourceDocument, err := manager.store.Read()
	if err != nil {
		return err
	}
	if (!install || snapshot) && sourceDocument.Active != nil && (component == DeployAll || component == DeployData) {
		if _, _, err := manager.deploymentSpec(ctx, ""); err != nil {
			return fmt.Errorf("portable deployment is not ready: %w", err)
		}
	}
	if (!install || snapshot) && (component == DeployAll || component == DeployData) {
		targetDocument, err := readSystemDeploymentState(target)
		if err != nil {
			return fmt.Errorf("read system state: %w", err)
		}
		targetSubscriptions, err := manager.meaningfulSubscriptionData(target, targetDocument)
		if err != nil {
			return err
		}
		sameData := sameDeploymentData(sourceDocument, targetDocument)
		if sameData {
			sameData, err = manager.sameSubscriptionCatalog(target)
			if err != nil {
				return err
			}
		}
		if (meaningfulState(targetDocument) || targetSubscriptions) && !sameData && !allowReplace {
			return &ConfirmationRequired{Summary: deploymentReplacementSummary(targetDocument)}
		}
	}

	current, err := manager.service.Status(ctx)
	if err != nil {
		return err
	}
	if !install && current == service.NotInstalled {
		return fmt.Errorf("system service is not installed; run 'sempre service install' first")
	}

	var operations []*swapOperation
	if install && !snapshot {
		targetDocument, readErr := readSystemDeploymentState(target)
		if readErr != nil {
			return readErr
		}
		operations, err = manager.stageInstallation(ctx, target, sourceDocument, targetDocument)
	} else {
		operations, err = manager.stageDeployment(ctx, target, component, sourceDocument)
	}
	if err != nil {
		return err
	}
	defer cleanupStaged(operations)

	wasRunning := current == service.Running || current == service.StartPending
	if current != service.NotInstalled && current != service.Stopped {
		if err := manager.service.Stop(ctx); err != nil {
			cleanupCtx, cancel := deploymentCleanupContext(ctx)
			defer cancel()
			return errors.Join(err, restoreServiceState(cleanupCtx, manager.service, current))
		}
	}
	if install || component == DeployAll || component == DeployBin {
		if err := recoverExecutableBackup(target.ServiceExecutable); err != nil {
			cleanupCtx, cancel := deploymentCleanupContext(ctx)
			defer cancel()
			return errors.Join(err, restoreServiceState(cleanupCtx, manager.service, current))
		}
	}
	if err := activateSwaps(operations); err != nil {
		cleanupCtx, cancel := deploymentCleanupContext(ctx)
		defer cancel()
		return errors.Join(err, restoreServiceState(cleanupCtx, manager.service, current))
	}
	if component == DeployAll || install {
		if err := target.Ensure(); err != nil {
			return rollbackDeployment(ctx, manager.service, operations, current, false, target, err)
		}
	}
	repairRegistration := install || component == DeployAll || component == DeployBin
	rollbackCommand := func() error { return nil }
	if repairRegistration {
		if err := manager.service.Install(ctx, target.ServiceExecutable, target.Home); err != nil {
			return rollbackDeployment(ctx, manager.service, operations, current, repairRegistration, target, err)
		}
		rollbackCommand, err = manager.commands.Register(target)
		if err != nil {
			return rollbackDeployment(ctx, manager.service, operations, current, repairRegistration, target, err)
		}
	}
	rollback := func(cause error) error {
		commandErr := rollbackCommand()
		deploymentErr := rollbackDeployment(ctx, manager.service, operations, current, repairRegistration, target, cause)
		if commandErr != nil {
			return fmt.Errorf("%w (command registration rollback failed: %v)", deploymentErr, commandErr)
		}
		return deploymentErr
	}
	if install || wasRunning {
		if err := manager.service.Start(ctx); err != nil {
			return rollback(err)
		}
	} else if repairRegistration {
		afterInstall, statusErr := manager.service.Status(ctx)
		if statusErr != nil {
			return rollback(statusErr)
		}
		if afterInstall != service.Stopped && afterInstall != service.NotInstalled {
			if err := manager.service.Stop(ctx); err != nil {
				return rollback(err)
			}
		}
	}
	if err := commitSwaps(operations); err != nil {
		return fmt.Errorf("deployment committed but backup cleanup failed: %w", err)
	}
	if install {
		manager.installBundledUIBestEffort(target)
	}
	return nil
}

func (manager *Manager) installBundledUIBestEffort(target layout.Layout) {
	targetManager, err := New(target, manager.output, manager.errors)
	if err != nil {
		fmt.Fprintln(manager.errors, "WARNING: initialize installed UI:", err)
		return
	}
	metadata, currentErr := targetManager.ui.Current()
	if currentErr == nil && metadata.SourceType != "official" {
		return
	}
	if _, found, err := targetManager.installBundledUI(); found && err != nil {
		fmt.Fprintln(manager.errors, "WARNING: install bundled UI:", err)
	}
}

func readSystemDeploymentState(paths layout.Layout) (state.Document, error) {
	if _, err := os.Stat(paths.State); errors.Is(err, os.ErrNotExist) {
		return state.NewDocument(), nil
	} else if err != nil {
		return state.Document{}, fmt.Errorf("inspect system state: %w", err)
	}
	return state.New(paths).Read()
}

func (manager *Manager) replaceSystemExecutable(ctx context.Context, target layout.Layout) error {
	if err := target.EnsureServiceExecutableDirectory(); err != nil {
		return err
	}
	source, err := layout.CurrentExecutable()
	if err != nil {
		return err
	}
	var operations []*swapOperation
	if !sameFile(source, target.ServiceExecutable) {
		operation, err := stageExecutable(source, target.ServiceExecutable)
		if err != nil {
			return err
		}
		operations = append(operations, operation)
	}
	defer cleanupStaged(operations)

	current, err := manager.service.Status(ctx)
	if err != nil {
		return err
	}
	if current != service.NotInstalled && current != service.Stopped {
		if err := manager.service.Stop(ctx); err != nil {
			cleanupCtx, cancel := deploymentCleanupContext(ctx)
			defer cancel()
			return errors.Join(err, restoreServiceState(cleanupCtx, manager.service, current))
		}
	}
	if err := recoverExecutableBackup(target.ServiceExecutable); err != nil {
		cleanupCtx, cancel := deploymentCleanupContext(ctx)
		defer cancel()
		return errors.Join(err, restoreServiceState(cleanupCtx, manager.service, current))
	}
	if err := activateSwaps(operations); err != nil {
		cleanupCtx, cancel := deploymentCleanupContext(ctx)
		defer cancel()
		return errors.Join(err, restoreServiceState(cleanupCtx, manager.service, current))
	}
	if err := manager.service.Install(ctx, target.ServiceExecutable, target.Home); err != nil {
		return rollbackDeployment(ctx, manager.service, operations, current, true, target, err)
	}
	if err := manager.service.Start(ctx); err != nil {
		return rollbackDeployment(ctx, manager.service, operations, current, true, target, err)
	}
	if err := commitSwaps(operations); err != nil {
		return fmt.Errorf("service update committed but backup cleanup failed: %w", err)
	}
	return nil
}

func rollbackDeployment(
	ctx context.Context,
	controller service.Controller,
	operations []*swapOperation,
	previous service.State,
	repairRegistration bool,
	target layout.Layout,
	cause error,
) error {
	cleanupCtx, cancel := deploymentCleanupContext(ctx)
	defer cancel()
	rollbackErr := rollbackSwaps(operations)
	if repairRegistration {
		if previous == service.NotInstalled {
			rollbackErr = errors.Join(rollbackErr, controller.Uninstall(cleanupCtx))
		} else {
			rollbackErr = errors.Join(rollbackErr, controller.Install(cleanupCtx, target.ServiceExecutable, target.Home))
		}
	}
	rollbackErr = errors.Join(rollbackErr, restoreServiceState(cleanupCtx, controller, previous))
	if rollbackErr != nil {
		return fmt.Errorf("%w (rollback failed: %v)", cause, rollbackErr)
	}
	return cause
}

func deploymentCleanupContext(parent context.Context) (context.Context, context.CancelFunc) {
	return context.WithTimeout(context.WithoutCancel(parent), 30*time.Second)
}

func restoreServiceState(ctx context.Context, controller service.Controller, previous service.State) error {
	switch previous {
	case service.Running, service.StartPending:
		return controller.Start(ctx)
	case service.Stopped, service.StopPending:
		return controller.Stop(ctx)
	default:
		return nil
	}
}

func (manager *Manager) checkServiceExecutable() error {
	if manager.paths.Mode != layout.System {
		return nil
	}
	info, err := os.Stat(manager.paths.ServiceExecutable)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("not installed")
		}
		return err
	}
	if info.IsDir() {
		return fmt.Errorf("is a directory")
	}
	if err := checkProtectedPath(manager.paths.ServiceExecutable); err != nil {
		return err
	}
	return nil
}
