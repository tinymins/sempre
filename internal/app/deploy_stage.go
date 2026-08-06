package app

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"sort"

	"github.com/tinymins/sempre/internal/core"
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

func deploymentDocument(document state.Document) state.Document {
	document.Normalize()
	document.Runtime = state.Runtime{}
	return document
}

func meaningfulState(document state.Document) bool {
	return meaningfulDeploymentState(document) ||
		document.DesiredState == state.DesiredStopped
}

func meaningfulDeploymentState(document state.Document) bool {
	return document.Selected != nil ||
		document.Active != nil ||
		document.Previous != nil ||
		len(document.Cores) != 0 ||
		len(document.Configs) != 0 ||
		document.Subscription.URL != ""
}

func sameDeploymentData(left, right state.Document) bool {
	left = deploymentDocument(left)
	right = deploymentDocument(right)
	left.UpdatedAt = right.UpdatedAt
	return reflect.DeepEqual(left, right)
}

func (manager *Manager) sameSubscriptionCatalog(target layout.Layout) (bool, error) {
	left, err := os.ReadFile(manager.paths.SubscriptionStore)
	if err != nil {
		return false, fmt.Errorf("read portable subscription catalog: %w", err)
	}
	right, err := os.ReadFile(target.SubscriptionStore)
	if errors.Is(err, os.ErrNotExist) {
		return false, nil
	}
	if err != nil {
		return false, fmt.Errorf("read system subscription catalog: %w", err)
	}
	return bytes.Equal(left, right), nil
}

func deploymentReplacementSummary(document state.Document) string {
	selected := "none"
	if document.Selected != nil {
		selected = selectionRef(*document.Selected).String()
	}
	active := "none"
	if document.Active != nil {
		active = deploymentLabel(*document.Active)
	}
	configured := "no"
	if document.ActiveProfileID != "" || document.Subscription.URL != "" {
		configured = "yes"
	}
	versions := 0
	for _, coreState := range document.Cores {
		for _, source := range coreState.SourceEntries() {
			versions += len(source.State.Installed)
		}
	}
	return fmt.Sprintf(
		"Existing system data will be replaced:\n  Selected: %s\n  Active: %s\n  Core versions: %d\n  Subscription configured: %s",
		selected,
		active,
		versions,
		configured,
	)
}

func referencedConfigs(document state.Document) map[string]map[string]bool {
	result := map[string]map[string]bool{}
	add := func(coreID, hash string) {
		if coreID == "" || hash == "" {
			return
		}
		if result[coreID] == nil {
			result[coreID] = map[string]bool{}
		}
		result[coreID][hash] = true
	}
	for coreID, hash := range document.Configs {
		add(coreID, hash)
	}
	if document.Active != nil {
		add(document.Active.Core, document.Active.ConfigHash)
	}
	if document.Previous != nil {
		add(document.Previous.Core, document.Previous.ConfigHash)
	}
	return result
}

func sortedCoreIDs(document state.Document) []string {
	ids := make([]string, 0, len(document.Cores))
	for coreID := range document.Cores {
		ids = append(ids, coreID)
	}
	sort.Strings(ids)
	return ids
}

type installedCore struct {
	Repository string
	Version    string
}

func sortedInstallations(coreState *state.CoreState) []installedCore {
	var installations []installedCore
	for _, source := range coreState.SourceEntries() {
		for version := range source.State.Installed {
			installations = append(installations, installedCore{Repository: source.Repository, Version: version})
		}
	}
	sort.Slice(installations, func(i, j int) bool {
		if installations[i].Repository == installations[j].Repository {
			return installations[i].Version < installations[j].Version
		}
		return installations[i].Repository < installations[j].Repository
	})
	return installations
}

func stagedCoreVersionDir(staging, coreID, repository, version string) string {
	if repository == "" {
		return filepath.Join(staging, coreID, version)
	}
	return filepath.Join(staging, coreID, "sources", filepath.FromSlash(repository), version)
}

func validateCoreVersion(coreID, version string) error {
	ref, err := core.ParseRef(coreID + "@" + version)
	if err != nil || ref.IsChannel() {
		return fmt.Errorf("invalid managed core version %s@%s", coreID, version)
	}
	return nil
}
