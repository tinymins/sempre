package app

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func (manager *Manager) stageDeployment(
	ctx context.Context,
	target layout.Layout,
	component DeployComponent,
	document state.Document,
) ([]*swapOperation, error) {
	var operations []*swapOperation
	fail := func(err error) ([]*swapOperation, error) {
		cleanupStaged(operations)
		return nil, err
	}

	if component == DeployAll || component == DeployBin {
		if err := target.EnsureServiceExecutableDirectory(); err != nil {
			return fail(err)
		}
		executable, err := layout.CurrentExecutable()
		if err != nil {
			return fail(err)
		}
		if !sameFile(executable, target.ServiceExecutable) {
			operation, err := stageExecutable(executable, target.ServiceExecutable)
			if err != nil {
				return fail(err)
			}
			operations = append(operations, operation)
		}
		resources, err := manager.stageMergedDirectory(
			filepath.Join(filepath.Dir(executable), "resources"),
			target.Resources,
			0o600,
		)
		if err != nil {
			return fail(err)
		}
		operations = append(operations, resources)
		tools, err := manager.stageMergedDirectory(manager.paths.Tools, target.Tools, 0o755)
		if err != nil {
			return fail(err)
		}
		operations = append(operations, tools)
	}
	if component == DeployAll || component == DeployCore {
		operation, err := manager.stageCores(ctx, target, document, component == DeployCore)
		if err != nil {
			return fail(err)
		}
		operations = append(operations, operation)
	}
	if component == DeployAll || component == DeployData {
		if component == DeployData {
			if err := manager.validateTargetCores(ctx, target, document); err != nil {
				return fail(err)
			}
		}
		configs, err := manager.stageConfigs(target, document)
		if err != nil {
			return fail(err)
		}
		operations = append(operations, configs)
		subscriptionData, err := stageDirectoryFromSources(
			target.Subscriptions,
			0o600,
			manager.paths.Subscriptions,
		)
		if err != nil {
			return fail(err)
		}
		operations = append(operations, subscriptionData)
		tunnels, err := manager.stageTunnelConfig(target.TunnelConfig, false)
		if err != nil {
			return fail(err)
		}
		operations = append(operations, tunnels)
		stateFile, err := stageStateFile(target.State, deploymentDocument(document))
		if err != nil {
			return fail(err)
		}
		operations = append(operations, stateFile)
		web, err := manager.stageWebConfig(target.WebConfig, false)
		if err != nil {
			return fail(err)
		}
		operations = append(operations, web)
		ui, err := manager.stageCurrentUI(target.UICurrent)
		if err != nil {
			return fail(err)
		}
		operations = append(operations, ui)
	}
	return operations, nil
}

func (manager *Manager) stageInstallation(
	ctx context.Context,
	target layout.Layout,
	source, existing state.Document,
) ([]*swapOperation, error) {
	var operations []*swapOperation
	fail := func(err error) ([]*swapOperation, error) {
		cleanupStaged(operations)
		return nil, err
	}
	if err := target.EnsureServiceExecutableDirectory(); err != nil {
		return nil, err
	}
	executable, err := layout.CurrentExecutable()
	if err != nil {
		return nil, err
	}
	if !sameFile(executable, target.ServiceExecutable) {
		operation, err := stageExecutable(executable, target.ServiceExecutable)
		if err != nil {
			return fail(err)
		}
		operations = append(operations, operation)
	}
	resources, err := manager.stageMergedDirectory(
		filepath.Join(filepath.Dir(executable), "resources"),
		target.Resources,
		0o600,
	)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, resources)
	tools, err := manager.stageMergedDirectory(manager.paths.Tools, target.Tools, 0o755)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, tools)
	cores, err := manager.stageCores(ctx, target, source, true)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, cores)
	configs, err := manager.stageMergedDirectory(manager.paths.Configs, target.Configs, 0o600)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, configs)
	preserveSubscriptions, err := manager.meaningfulSubscriptionData(target, existing)
	if err != nil {
		return fail(err)
	}
	existingSubscriptionWins := meaningfulDeploymentState(existing) || preserveSubscriptions
	subscriptionData, err := manager.stageSubscriptionInstallation(
		target,
		existing,
		existingSubscriptionWins,
	)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, subscriptionData)
	tunnels, err := manager.stageTunnelConfig(target.TunnelConfig, true)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, tunnels)
	merged := mergeInstallDocument(source, existing, existingSubscriptionWins)
	stateFile, err := stageStateFile(target.State, merged)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, stateFile)
	web, err := manager.stageWebConfig(target.WebConfig, false)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, web)
	ui, err := manager.stageCurrentUI(target.UICurrent)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, ui)
	return operations, nil
}

func (manager *Manager) stageMergedDirectory(source, target string, mode os.FileMode) (*swapOperation, error) {
	sources := []string{target}
	if !sameFile(source, target) {
		sources = append(sources, source)
	}
	return stageDirectoryFromSources(target, mode, sources...)
}

func stageDirectoryFromSources(target string, mode os.FileMode, sources ...string) (*swapOperation, error) {
	staging, err := stageDirectory(target)
	if err != nil {
		return nil, err
	}
	operation := &swapOperation{staged: staging, target: target}
	for _, source := range sources {
		if err := copyDirectoryIfExists(source, staging, mode); err != nil {
			operation.cleanup()
			return nil, err
		}
	}
	return operation, nil
}

func (manager *Manager) stageSubscriptionInstallation(
	target layout.Layout,
	existing state.Document,
	existingWins bool,
) (*swapOperation, error) {
	sources := []string{target.Subscriptions, manager.paths.Subscriptions}
	if existingWins {
		sources[0], sources[1] = sources[1], sources[0]
	}
	operation, err := stageDirectoryFromSources(target.Subscriptions, 0o600, sources...)
	if err != nil {
		return nil, err
	}
	if !existingWins {
		return operation, nil
	}
	if _, err := os.Stat(target.SubscriptionStore); err == nil {
		return operation, nil
	} else if !errors.Is(err, os.ErrNotExist) {
		operation.cleanup()
		return nil, err
	}

	// An older installation has no catalog yet. Preserve its legacy URL (or its
	// empty subscription state) instead of adopting the portable catalog.
	if err := os.Remove(filepath.Join(operation.staged, filepath.Base(target.SubscriptionStore))); err != nil &&
		!errors.Is(err, os.ErrNotExist) {
		operation.cleanup()
		return nil, err
	}
	stagedPaths := target
	stagedPaths.Subscriptions = operation.staged
	stagedPaths.SubscriptionStore = filepath.Join(operation.staged, filepath.Base(target.SubscriptionStore))
	stagedPaths.SubscriptionBlobs = filepath.Join(operation.staged, filepath.Base(target.SubscriptionBlobs))
	stagedPaths.SubscriptionCache = filepath.Join(operation.staged, filepath.Base(target.SubscriptionCache))
	if err := subscriptions.NewStore(stagedPaths).Initialize(existing.Subscription.URL); err != nil {
		operation.cleanup()
		return nil, err
	}
	return operation, nil
}

func (manager *Manager) stageWebConfig(target string, clearPassword bool) (*swapOperation, error) {
	config, err := manager.web.Read()
	if err != nil {
		return nil, err
	}
	if clearPassword {
		config.Password = ""
	}
	if err := config.Validate(); err != nil {
		return nil, err
	}
	data, err := json.MarshalIndent(config, "", "  ")
	if err != nil {
		return nil, err
	}
	data = append(data, '\n')
	if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
		return nil, err
	}
	staging, err := unusedSibling(target, ".sempre-web-*")
	if err != nil {
		return nil, err
	}
	if err := state.WriteAtomic(staging, data, 0o600); err != nil {
		return nil, err
	}
	return &swapOperation{staged: staging, target: target}, nil
}

func (manager *Manager) stageCurrentUI(target string) (*swapOperation, error) {
	staging, err := stageDirectory(target)
	if err != nil {
		return nil, err
	}
	operation := &swapOperation{staged: staging, target: target}
	if _, err := os.Stat(manager.paths.UICurrent); errors.Is(err, os.ErrNotExist) {
		return operation, nil
	} else if err != nil {
		operation.cleanup()
		return nil, err
	}
	if _, err := manager.ui.Current(); err != nil {
		operation.cleanup()
		return nil, fmt.Errorf("validate current UI: %w", err)
	}
	if err := copyDirectory(manager.paths.UICurrent, staging, 0o600); err != nil {
		operation.cleanup()
		return nil, err
	}
	return operation, nil
}
