package app

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func mergeInstallDocument(source, existing state.Document, preserveSubscriptions bool) state.Document {
	source.Normalize()
	existing.Normalize()
	hadExistingState := meaningfulState(existing)
	hadExistingDeployment := meaningfulDeploymentState(existing)
	result := existing
	for coreID, sourceCore := range source.Cores {
		targetCore := result.Core(coreID)
		for _, entry := range sourceCore.SourceEntries() {
			targetSource := targetCore.Source(entry.Repository)
			for version, installation := range entry.State.Installed {
				copy := *installation
				targetSource.Installed[version] = &copy
			}
			for channel, version := range entry.State.Channels {
				if targetSource.Channels[channel] == "" {
					targetSource.Channels[channel] = version
				}
			}
		}
	}
	for coreID, hash := range source.Configs {
		if result.Configs[coreID] == "" {
			result.Configs[coreID] = hash
			if build, ok := source.ConfigBuilds[coreID]; ok {
				result.ConfigBuilds[coreID] = build
			}
		}
	}
	if !hadExistingDeployment {
		result.Selected = source.Selected
		result.Active = source.Active
		result.Previous = source.Previous
		result.Pending = source.Pending
		result.LastError = source.LastError
		if !hadExistingState {
			result.DesiredState = source.DesiredState
		}
	}
	if !preserveSubscriptions {
		result.Subscription = source.Subscription
		result.ActiveProfileID = source.ActiveProfileID
		result.AutoRestart = source.AutoRestart
	}
	result.Runtime = state.Runtime{}
	result.Normalize()
	return result
}

func (manager *Manager) meaningfulSubscriptionData(target layout.Layout, document state.Document) (bool, error) {
	if document.Subscription.URL != "" || document.Subscription.Interval != "24h" || !document.AutoRestart {
		return true, nil
	}
	if _, err := os.Stat(target.SubscriptionStore); errors.Is(err, os.ErrNotExist) {
		return false, nil
	} else if err != nil {
		return false, err
	}
	catalog, err := subscriptions.NewStore(target).Read()
	if err != nil {
		return false, err
	}
	if len(catalog.Profiles) != 1 || len(catalog.CustomNodes) != 0 {
		return true, nil
	}
	profile := catalog.Profiles[0]
	return profile.Name != "" ||
		len(profile.Sources) != 0 || len(profile.CustomNodeIDs) != 0 ||
		len(profile.Groups) != 0 || len(profile.Rules) != 0 || len(profile.RuleProviders) != 0 || len(profile.Filters) != 0 ||
		len(profile.DNS) != 0 || len(profile.PrivateAccess) != 0 || len(profile.CoreOverrides) != 0 ||
		!profile.UseSystemGroups || !profile.UseSystemRules || !profile.UseSystemFilters || !profile.UseSystemDNS || !profile.UseSystemCustomConfig, nil
}

func (manager *Manager) stageCores(
	ctx context.Context,
	target layout.Layout,
	document state.Document,
	merge bool,
) (*swapOperation, error) {
	staging, err := stageDirectory(target.Cores)
	if err != nil {
		return nil, err
	}
	operation := &swapOperation{staged: staging, target: target.Cores}
	if merge {
		if err := copyDirectoryIfExists(target.Cores, staging, 0o700); err != nil {
			operation.cleanup()
			return nil, fmt.Errorf("stage existing system cores: %w", err)
		}
	}
	for _, coreID := range sortedCoreIDs(document) {
		adapter, err := manager.registry.Get(coreID)
		if err != nil {
			operation.cleanup()
			return nil, err
		}
		for _, installed := range sortedInstallations(document.Cores[coreID]) {
			version := installed.Version
			if err := validateCoreVersion(coreID, version); err != nil {
				operation.cleanup()
				return nil, err
			}
			actual, err := adapter.Version(ctx, coreBinaryPath(manager.paths, adapter, installed.Repository, version))
			if err != nil {
				operation.cleanup()
				return nil, fmt.Errorf("validate portable %s: %w", exactRef(core.Ref{Core: coreID, Repository: installed.Repository}, version), err)
			}
			if actual != version {
				operation.cleanup()
				return nil, fmt.Errorf("portable %s reports version %s, expected %s", coreID, actual, version)
			}
			destination := stagedCoreVersionDir(staging, coreID, installed.Repository, version)
			if err := os.RemoveAll(destination); err != nil {
				operation.cleanup()
				return nil, err
			}
			if err := copyDirectory(manager.paths.CoreVersionDir(coreID, installed.Repository, version), destination, 0o700); err != nil {
				operation.cleanup()
				return nil, fmt.Errorf("stage %s@%s: %w", coreID, version, err)
			}
		}
	}
	return operation, nil
}

func (manager *Manager) validateTargetCores(
	ctx context.Context,
	target layout.Layout,
	document state.Document,
) error {
	for _, coreID := range sortedCoreIDs(document) {
		adapter, err := manager.registry.Get(coreID)
		if err != nil {
			return err
		}
		for _, installed := range sortedInstallations(document.Cores[coreID]) {
			version := installed.Version
			if err := validateCoreVersion(coreID, version); err != nil {
				return err
			}
			actual, err := adapter.Version(ctx, coreBinaryPath(target, adapter, installed.Repository, version))
			if err != nil {
				return fmt.Errorf("system core %s@%s is required by data deployment: %w", coreID, version, err)
			}
			if actual != version {
				return fmt.Errorf("system core %s reports version %s, expected %s", coreID, actual, version)
			}
		}
	}
	return nil
}

func (manager *Manager) stageConfigs(
	target layout.Layout,
	document state.Document,
) (*swapOperation, error) {
	staging, err := stageDirectory(target.Configs)
	if err != nil {
		return nil, err
	}
	operation := &swapOperation{staged: staging, target: target.Configs}
	for coreID, hashes := range referencedConfigs(document) {
		for hash := range hashes {
			data, err := os.ReadFile(manager.paths.Config(coreID, hash))
			if err != nil {
				operation.cleanup()
				return nil, fmt.Errorf("read referenced configuration %s/%s: %w", coreID, hash, err)
			}
			if err := state.WriteAtomic(filepath.Join(staging, coreID, hash+".json"), data, 0o600); err != nil {
				operation.cleanup()
				return nil, err
			}
		}
	}
	return operation, nil
}
