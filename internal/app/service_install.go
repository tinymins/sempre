package app

import (
	"context"
	"errors"
	"fmt"
	"os"
	"strings"

	"github.com/sempre-lab/sempre/internal/layout"
	"github.com/sempre-lab/sempre/internal/service"
	"github.com/sempre-lab/sempre/internal/state"
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
	if manager.paths.Mode == layout.Portable {
		return manager.deployToSystem(ctx, systemPaths, DeployAll, allowReplace, true)
	}
	if _, _, err := manager.deploymentSpec(ctx, ""); err != nil {
		return fmt.Errorf("system deployment is not ready: %w", err)
	}
	return manager.replaceSystemExecutable(ctx, systemPaths)
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
	return manager.deployToSystem(ctx, systemPaths, component, allowReplace, false)
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
	if component == DeployAll || component == DeployData {
		if _, _, err := manager.deploymentSpec(ctx, ""); err != nil {
			return fmt.Errorf("portable deployment is not ready: %w", err)
		}
		targetDocument, err := readSystemDeploymentState(target)
		if err != nil {
			return fmt.Errorf("read system state: %w", err)
		}
		if meaningfulState(targetDocument) && !sameDeploymentData(sourceDocument, targetDocument) && !allowReplace {
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

	operations, err := manager.stageDeployment(ctx, target, component, sourceDocument)
	if err != nil {
		return err
	}
	defer cleanupStaged(operations)

	wasRunning := current == service.Running || current == service.StartPending
	if current != service.NotInstalled && current != service.Stopped {
		if err := manager.service.Stop(ctx); err != nil {
			return err
		}
	}
	if err := activateSwaps(operations); err != nil {
		_ = restoreServiceState(ctx, manager.service, current)
		return err
	}
	if component == DeployAll {
		if err := target.Ensure(); err != nil {
			return rollbackDeployment(ctx, manager.service, operations, current, false, target, err)
		}
	}

	repairRegistration := install || component == DeployAll || component == DeployBin
	if repairRegistration {
		if err := manager.service.Install(ctx, target.ServiceExecutable, target.Home); err != nil {
			return rollbackDeployment(ctx, manager.service, operations, current, repairRegistration, target, err)
		}
	}
	if install || wasRunning {
		if err := manager.service.Start(ctx); err != nil {
			return rollbackDeployment(ctx, manager.service, operations, current, repairRegistration, target, err)
		}
	} else if repairRegistration {
		afterInstall, statusErr := manager.service.Status(ctx)
		if statusErr != nil {
			return rollbackDeployment(ctx, manager.service, operations, current, repairRegistration, target, statusErr)
		}
		if afterInstall != service.Stopped && afterInstall != service.NotInstalled {
			if err := manager.service.Stop(ctx); err != nil {
				return rollbackDeployment(ctx, manager.service, operations, current, repairRegistration, target, err)
			}
		}
	}
	commitSwaps(operations)
	return nil
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
			return err
		}
	}
	if err := activateSwaps(operations); err != nil {
		_ = restoreServiceState(ctx, manager.service, current)
		return err
	}
	if err := manager.service.Install(ctx, target.ServiceExecutable, target.Home); err != nil {
		return rollbackDeployment(ctx, manager.service, operations, current, true, target, err)
	}
	if err := manager.service.Start(ctx); err != nil {
		return rollbackDeployment(ctx, manager.service, operations, current, true, target, err)
	}
	commitSwaps(operations)
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
	rollbackErr := rollbackSwaps(operations)
	if repairRegistration {
		if previous == service.NotInstalled {
			rollbackErr = errors.Join(rollbackErr, controller.Uninstall(ctx))
		} else {
			rollbackErr = errors.Join(rollbackErr, controller.Install(ctx, target.ServiceExecutable, target.Home))
		}
	}
	rollbackErr = errors.Join(rollbackErr, restoreServiceState(ctx, controller, previous))
	if rollbackErr != nil {
		return fmt.Errorf("%w (rollback failed: %v)", cause, rollbackErr)
	}
	return cause
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
