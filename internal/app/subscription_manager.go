package app

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"runtime"
	"sort"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

var errSubscriptionConfigurationContextChanged = errors.New("subscription configuration target changed; reload the profile before saving")

func (manager *Manager) SubscriptionCatalog() (subscriptions.Catalog, string, state.Subscription, bool, error) {
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return subscriptions.Catalog{}, "", state.Subscription{}, false, err
	}
	document, err := manager.store.Read()
	if err != nil {
		return subscriptions.Catalog{}, "", state.Subscription{}, false, err
	}
	return catalog, document.ActiveProfileID, document.Subscription, document.AutoRestart, nil
}

func (manager *Manager) SubscriptionConfigurationContext() (subscriptions.ConfigurationContext, error) {
	document, err := manager.store.Read()
	if err != nil {
		return subscriptions.ConfigurationContext{}, err
	}
	context := subscriptions.ConfigurationContext{
		Key:          "common",
		Platform:     runtime.GOOS,
		Capabilities: manager.registry.StableCapabilities(core.CurrentTarget()),
	}
	if document.Active != nil {
		context.Running = &subscriptions.RunningCore{Core: document.Active.Core, Version: document.Active.Version}
	}
	if document.Selected == nil {
		return context, nil
	}
	deployment, adapter, err := manager.configurationTarget(document)
	if err != nil {
		return subscriptions.ConfigurationContext{}, err
	}
	compilerTarget, err := adapter.CompilerTarget(deployment.Version, core.CurrentTarget())
	if err != nil {
		return subscriptions.ConfigurationContext{}, err
	}
	target, err := subscriptions.ParseTarget(compilerTarget.Format)
	if err != nil {
		return subscriptions.ConfigurationContext{}, err
	}
	target.Core = adapter.ID()
	configurationTarget := subscriptions.NewConfigurationTarget(adapter.ID(), deployment.Version, target)
	context.Target = &configurationTarget
	context.Key = configurationTarget.Key
	context.Capabilities = manager.registry.Capabilities(adapter, deployment.Version, core.CurrentTarget())
	return context, nil
}

func (manager *Manager) CreateSubscriptionProfile(name string) (subscriptions.Profile, error) {
	profile := subscriptions.NewProfile(name)
	err := manager.withOperation(func() error {
		return manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
			catalog.Profiles = append(catalog.Profiles, profile)
			return nil
		})
	})
	return profile, err
}

func (manager *Manager) RenameSubscriptionProfile(id, name string) (subscriptions.Profile, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return subscriptions.Profile{}, fmt.Errorf("profile name is required")
	}
	var renamed subscriptions.Profile
	err := manager.withOperation(func() error {
		return manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
			profile, err := subscriptions.FindProfile(catalog, id)
			if err != nil {
				return err
			}
			profile.Name = name
			renamed = *profile
			return nil
		})
	})
	return renamed, err
}

func (manager *Manager) SaveSubscriptionProfile(ctx context.Context, id string, candidate subscriptions.Profile) (Change, subscriptions.RenderResult, error) {
	return manager.saveSubscriptionProfile(ctx, id, candidate, "")
}

func (manager *Manager) SaveSubscriptionProfileForContext(ctx context.Context, id string, candidate subscriptions.Profile, expectedContext string) (Change, subscriptions.RenderResult, error) {
	return manager.saveSubscriptionProfile(ctx, id, candidate, expectedContext)
}

func (manager *Manager) saveSubscriptionProfile(ctx context.Context, id string, candidate subscriptions.Profile, expectedContext string) (Change, subscriptions.RenderResult, error) {
	var change Change
	var rendered subscriptions.RenderResult
	err := manager.withOperation(func() error {
		catalog, err := manager.subscriptions.Read()
		if err != nil {
			return err
		}
		current, err := subscriptions.FindProfile(&catalog, id)
		if err != nil {
			return err
		}
		previousConfigHash := current.LastConfigHash
		candidate.ID = id
		candidate.Revision = current.Revision
		if err := subscriptions.ApplyEditorConfig(&candidate); err != nil {
			return err
		}
		if err := normalizeSavedTransparentProxy(&candidate); err != nil {
			return err
		}
		candidate.LastCheck = current.LastCheck
		candidate.LastChange = current.LastChange
		candidate.LastResult = current.LastResult
		candidate.LastConfigHash = current.LastConfigHash
		candidate.LastRuntimeValidated = current.LastRuntimeValidated
		candidate.LastCompilerTarget = current.LastCompilerTarget
		candidate.LastCompilerWarnings = current.LastCompilerWarnings
		*current = candidate
		if err := subscriptions.ValidateCatalog(catalog); err != nil {
			return err
		}
		document, err := manager.store.Read()
		if err != nil {
			return err
		}
		if expectedContext != "" {
			actualContext, contextErr := manager.subscriptionConfigurationContextKey(document)
			if contextErr != nil {
				return contextErr
			}
			if actualContext != expectedContext {
				return errSubscriptionConfigurationContextChanged
			}
		}
		if !subscriptionProfileHasInputs(candidate) {
			candidate.LastResult = "profile saved without enabled nodes; active configuration retained"
			change = Change{Changed: true, Message: candidate.LastResult}
			return manager.subscriptions.Update(func(stored *subscriptions.Catalog) error {
				profile, err := subscriptions.FindProfile(stored, id)
				if err != nil {
					return err
				}
				*profile = candidate
				return nil
			})
		}
		candidate.Revision++
		*current = candidate
		if document.Selected == nil {
			candidate.LastResult = "profile saved; select a core to compile a runtime configuration"
			change = Change{Changed: true, Message: candidate.LastResult}
			return manager.subscriptions.Update(func(stored *subscriptions.Catalog) error {
				profile, err := subscriptions.FindProfile(stored, id)
				if err != nil {
					return err
				}
				*profile = candidate
				return nil
			})
		}
		target, runtimeValidation, targetWarnings, err := manager.subscriptionTarget(document)
		if err != nil {
			return err
		}
		rendered, candidate, err = manager.compiler.Render(ctx, candidate, catalog, target, true)
		if err != nil {
			return err
		}
		rendered.Warnings = append(targetWarnings, rendered.Warnings...)
		if runtimeValidation {
			if err := manager.ValidateConfigContent(ctx, []byte(rendered.Content)); err != nil {
				return fmt.Errorf("compiled subscription was rejected by the selected core: %w", err)
			}
			rendered.RuntimeValidated = true
		}
		configHash, err := manager.subscriptions.SaveBlob([]byte(rendered.Content))
		if err != nil {
			return fmt.Errorf("persist compiled subscription: %w", err)
		}
		now := time.Now().UTC()
		candidate.LastCheck = now
		candidate.LastResult = "configuration compiled"
		candidate.LastConfigHash = configHash
		if previousConfigHash != configHash {
			candidate.LastChange = now
		}
		candidate.LastRuntimeValidated = runtimeValidation
		candidate.LastCompilerTarget = target.Format
		candidate.LastCompilerWarnings = append([]string{}, rendered.Warnings...)
		active := document.ActiveProfileID == id
		if active && runtimeValidation {
			change, err = manager.activateConfig(ctx, []byte(rendered.Content), configBuild(candidate, target), func(document *state.Document, changed bool) {
				document.Subscription.LastCheck = now
				if changed {
					document.Subscription.LastChange = now
					document.Subscription.LastResult = "configuration updated"
					candidate.LastChange = now
				} else {
					document.Subscription.LastResult = "no change"
				}
			})
			if err != nil {
				return err
			}
			change.Message = "subscription profile saved, validated, and staged"
		} else if active {
			change = Change{Changed: true, Message: "subscription profile saved and compiled; select a core to validate and stage it"}
		} else {
			change = Change{Changed: true, Message: "subscription profile saved and compiled"}
		}
		return manager.subscriptions.Update(func(stored *subscriptions.Catalog) error {
			profile, err := subscriptions.FindProfile(stored, id)
			if err != nil {
				return err
			}
			*profile = candidate
			return nil
		})
	})
	return change, rendered, err
}

func normalizeSavedTransparentProxy(profile *subscriptions.Profile) error {
	profile.TransparentProxy.TUN.InterfaceName = strings.TrimSpace(profile.TransparentProxy.TUN.InterfaceName)
	if profile.TransparentProxy.TUN.InterfaceName == "" {
		return fmt.Errorf("TUN interface name must contain 1 to 15 characters")
	}
	return nil
}

func (manager *Manager) RefreshSubscriptionProfile(ctx context.Context, id string) (Change, subscriptions.RenderResult, error) {
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	profile, err := subscriptions.FindProfile(&catalog, id)
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	return manager.SaveSubscriptionProfile(ctx, id, *profile)
}

func (manager *Manager) UseSubscriptionProfile(ctx context.Context, id string) (Change, subscriptions.RenderResult, error) {
	document, err := manager.store.Read()
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	if document.ActiveProfileID == id {
		return manager.RefreshSubscriptionProfile(ctx, id)
	}
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	if _, err := subscriptions.FindProfile(&catalog, id); err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	var change Change
	var rendered subscriptions.RenderResult
	err = manager.withOperation(func() error {
		var refreshErr error
		change, rendered, refreshErr = manager.RefreshSubscriptionProfileAsActive(ctx, id, catalog)
		return refreshErr
	})
	return change, rendered, err
}

func (manager *Manager) RefreshSubscriptionProfileAsActive(ctx context.Context, id string, catalog subscriptions.Catalog) (Change, subscriptions.RenderResult, error) {
	profile, err := subscriptions.FindProfile(&catalog, id)
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	document, err := manager.store.Read()
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	target, runtimeValidation, warnings, err := manager.subscriptionTarget(document)
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	if !runtimeValidation {
		return Change{}, subscriptions.RenderResult{}, fmt.Errorf("select and install a core before activating a subscription profile")
	}
	rendered, updated, err := manager.compiler.Render(ctx, *profile, catalog, target, true)
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	rendered.Warnings = append(warnings, rendered.Warnings...)
	if err := manager.ValidateConfigContent(ctx, []byte(rendered.Content)); err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	rendered.RuntimeValidated = true
	updated.Revision = profile.Revision + 1
	previousConfigHash := updated.LastConfigHash
	configHash, err := manager.subscriptions.SaveBlob([]byte(rendered.Content))
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, fmt.Errorf("persist compiled subscription: %w", err)
	}
	now := time.Now().UTC()
	change, err := manager.activateConfig(ctx, []byte(rendered.Content), configBuild(updated, target), func(document *state.Document, changed bool) {
		document.ActiveProfileID = id
		document.Subscription.LastCheck = now
		if changed {
			document.Subscription.LastChange = now
			document.Subscription.LastResult = "configuration updated"
		} else {
			document.Subscription.LastResult = "no change"
		}
	})
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	updated.LastCheck = now
	updated.LastResult = "configuration compiled"
	updated.LastConfigHash = configHash
	updated.LastRuntimeValidated = true
	updated.LastCompilerTarget = target.Format
	updated.LastCompilerWarnings = append([]string{}, rendered.Warnings...)
	if previousConfigHash != configHash {
		updated.LastChange = now
	}
	if err := manager.subscriptions.Update(func(stored *subscriptions.Catalog) error {
		item, err := subscriptions.FindProfile(stored, id)
		if err != nil {
			return err
		}
		*item = updated
		return nil
	}); err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	change.Message = "subscription profile activated and staged"
	return change, rendered, nil
}

func (manager *Manager) RemoveSubscriptionProfile(id string) (Change, error) {
	change := Change{}
	err := manager.withOperation(func() error {
		document, err := manager.store.Read()
		if err != nil {
			return err
		}
		if document.ActiveProfileID == id {
			return fmt.Errorf("the active subscription profile cannot be removed")
		}
		return manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
			if len(catalog.Profiles) == 1 {
				return fmt.Errorf("the last subscription profile cannot be removed")
			}
			for index, profile := range catalog.Profiles {
				if profile.ID == id {
					catalog.Profiles = append(catalog.Profiles[:index], catalog.Profiles[index+1:]...)
					change = Change{Changed: true, Message: "subscription profile removed"}
					return nil
				}
			}
			return fmt.Errorf("subscription profile %q was not found", id)
		})
	})
	return change, err
}

func (manager *Manager) RenderSubscriptionProfile(ctx context.Context, id, format string, force bool) (subscriptions.RenderResult, error) {
	var result subscriptions.RenderResult
	err := manager.withOperation(func() error {
		catalog, err := manager.subscriptions.Read()
		if err != nil {
			return err
		}
		profile, err := subscriptions.FindProfile(&catalog, id)
		if err != nil {
			return err
		}
		target, err := subscriptions.ParseTarget(format)
		if err != nil {
			return err
		}
		result, _, err = manager.compiler.Render(ctx, *profile, catalog, target, force)
		return err
	})
	return result, err
}

func (manager *Manager) TestSubscriptionSource(ctx context.Context, source subscriptions.Source) (subscriptions.SourceResult, error) {
	return manager.testSubscriptionSource(ctx, source, true)
}

func (manager *Manager) TestSubscriptionSourceWithCache(ctx context.Context, source subscriptions.Source) (subscriptions.SourceResult, error) {
	return manager.testSubscriptionSource(ctx, source, false)
}

func (manager *Manager) testSubscriptionSource(ctx context.Context, source subscriptions.Source, force bool) (subscriptions.SourceResult, error) {
	if source.ID == "" {
		source.ID = subscriptions.NewID()
	}
	catalog := subscriptions.NewCatalog("")
	profile := catalog.Profiles[0]
	profile.Sources = []subscriptions.Source{source}
	var result subscriptions.RenderResult
	err := manager.withOperation(func() error {
		var renderErr error
		result, _, renderErr = manager.compiler.Render(ctx, profile, catalog, subscriptions.Target{Format: "clash-meta"}, force)
		return renderErr
	})
	if err != nil {
		return subscriptions.SourceResult{}, err
	}
	return result.SourceResults[0], nil
}

func (manager *Manager) TraceSubscriptionNode(ctx context.Context, id, name, format string) (subscriptions.FieldDiff, error) {
	result, err := manager.RenderSubscriptionProfile(ctx, id, format, true)
	if err != nil {
		return subscriptions.FieldDiff{}, err
	}
	for _, diff := range result.FieldDiffs {
		if diff.Node == name {
			return diff, nil
		}
	}
	return subscriptions.FieldDiff{}, fmt.Errorf("node %q was not found in conversion diagnostics", name)
}

func (manager *Manager) PreviewSubscriptionNodes(ctx context.Context, id string, force bool) ([]subscriptions.PreviewNode, error) {
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return nil, err
	}
	profile, err := subscriptions.FindProfile(&catalog, id)
	if err != nil {
		return nil, err
	}
	return manager.compiler.PreviewNodes(ctx, *profile, catalog, force)
}

func (manager *Manager) TraceSubscriptionNodeSteps(ctx context.Context, id, name, format string) (map[string]any, error) {
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return nil, err
	}
	profile, err := subscriptions.FindProfile(&catalog, id)
	if err != nil {
		return nil, err
	}
	return manager.compiler.TraceNode(ctx, *profile, catalog, name, format)
}

func (manager *Manager) ClearSubscriptionCache() (Change, error) {
	err := manager.withOperation(manager.subscriptions.ClearCache)
	if err != nil {
		return Change{}, err
	}
	return Change{Changed: true, Message: "subscription fetch cache cleared"}, nil
}

func (manager *Manager) CustomNodes() ([]subscriptions.CustomNode, error) {
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return nil, err
	}
	sort.Slice(catalog.CustomNodes, func(i, j int) bool { return catalog.CustomNodes[i].Name < catalog.CustomNodes[j].Name })
	return catalog.CustomNodes, nil
}

func (manager *Manager) SaveCustomNode(candidate subscriptions.CustomNode) (subscriptions.CustomNode, error) {
	create := candidate.ID == ""
	if create {
		candidate.ID = subscriptions.NewID()
	}
	if candidate.Proxy == nil {
		return subscriptions.CustomNode{}, fmt.Errorf("custom node proxy is required")
	}
	candidate.Name = strings.TrimSpace(candidate.Name)
	if candidate.Name == "" {
		candidate.Name = strings.TrimSpace(stringValue(candidate.Proxy["name"]))
	}
	if candidate.Name == "" {
		return subscriptions.CustomNode{}, fmt.Errorf("custom node name is required")
	}
	candidate.Proxy["name"] = candidate.Name
	proxy, err := subscriptions.ProxyFromMap(candidate.Proxy)
	if err != nil {
		return subscriptions.CustomNode{}, err
	}
	if proxy.Port == 0 {
		return subscriptions.CustomNode{}, fmt.Errorf("custom node port must be greater than zero")
	}
	now := time.Now().UTC()
	candidate.UpdatedAt = now
	err = manager.withOperation(func() error {
		return manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
			for index, item := range catalog.CustomNodes {
				if item.ID == candidate.ID {
					candidate.CreatedAt = item.CreatedAt
					catalog.CustomNodes[index] = candidate
					incrementProfilesReferencingNode(catalog, candidate.ID)
					return nil
				}
			}
			if !create {
				return fmt.Errorf("custom node %q was not found", candidate.ID)
			}
			candidate.CreatedAt = now
			catalog.CustomNodes = append(catalog.CustomNodes, candidate)
			return nil
		})
	})
	return candidate, err
}

func (manager *Manager) RemoveCustomNode(id string) (Change, error) {
	change := Change{}
	err := manager.withOperation(func() error {
		return manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
			for _, profile := range catalog.Profiles {
				for _, nodeID := range profile.CustomNodeIDs {
					if nodeID == id {
						return fmt.Errorf("custom node is referenced by subscription profile %q", profile.Name)
					}
				}
			}
			for index, node := range catalog.CustomNodes {
				if node.ID == id {
					catalog.CustomNodes = append(catalog.CustomNodes[:index], catalog.CustomNodes[index+1:]...)
					change = Change{Changed: true, Message: "custom node removed"}
					return nil
				}
			}
			return fmt.Errorf("custom node %q was not found", id)
		})
	})
	return change, err
}

func (manager *Manager) SetSubscriptionAutoRestart(enabled bool) (Change, error) {
	change := Change{}
	err := manager.withOperation(func() error {
		return manager.store.Update(func(document *state.Document) error {
			if document.AutoRestart == enabled {
				return nil
			}
			document.AutoRestart = enabled
			change = Change{Changed: true, Message: fmt.Sprintf("subscription automatic restart set to %t", enabled)}
			return nil
		})
	})
	return change, err
}

func (manager *Manager) subscriptionTarget(document state.Document) (subscriptions.Target, bool, []string, error) {
	if document.Selected == nil {
		return subscriptions.Target{}, false, nil, fmt.Errorf("select and install a core before compiling a subscription profile")
	}
	deployment, adapter, err := manager.configurationTarget(document)
	if err != nil {
		return subscriptions.Target{}, false, nil, err
	}
	compilerTarget, err := adapter.CompilerTarget(deployment.Version, core.CurrentTarget())
	if err != nil {
		return subscriptions.Target{}, false, nil, err
	}
	target, err := subscriptions.ParseTarget(compilerTarget.Format)
	target.Core = adapter.ID()
	return target, true, compilerTarget.Warnings, err
}

func (manager *Manager) subscriptionConfigurationContextKey(document state.Document) (string, error) {
	if document.Selected == nil {
		return "common", nil
	}
	deployment, adapter, err := manager.configurationTarget(document)
	if err != nil {
		return "", err
	}
	compilerTarget, err := adapter.CompilerTarget(deployment.Version, core.CurrentTarget())
	if err != nil {
		return "", err
	}
	target, err := subscriptions.ParseTarget(compilerTarget.Format)
	if err != nil {
		return "", err
	}
	target.Core = adapter.ID()
	return subscriptions.NewConfigurationTarget(adapter.ID(), deployment.Version, target).Key, nil
}

func configBuild(profile subscriptions.Profile, target subscriptions.Target) state.ConfigBuild {
	return state.ConfigBuild{
		ProfileID:       profile.ID,
		ProfileRevision: profile.Revision,
		TargetKey:       compilerTargetKey(target.Format, target.Version, target.Platform),
		RuntimeKey:      profileRuntimeKey(profile),
	}
}

func profileRuntimeKey(profile subscriptions.Profile) string {
	data, _ := json.Marshal(struct {
		Transparent subscriptions.TransparentProxyConfig `json:"transparent_proxy"`
		LocalProxy  subscriptions.LocalProxyConfig       `json:"local_proxy"`
		Management  subscriptions.ManagementAPIConfig    `json:"management_api"`
	}{Transparent: profile.TransparentProxy, LocalProxy: profile.LocalProxy, Management: profile.ManagementAPI})
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:])
}

func compilerTargetKey(format, version, platform string) string {
	return format + "|" + version + "|" + platform
}

func expectedConfigBuild(profile subscriptions.Profile, adapter core.Adapter, version string) (state.ConfigBuild, error) {
	target, err := adapter.CompilerTarget(version, core.CurrentTarget())
	if err != nil {
		return state.ConfigBuild{}, err
	}
	return state.ConfigBuild{
		ProfileID:       profile.ID,
		ProfileRevision: profile.Revision,
		TargetKey:       compilerTargetKey(target.Format, target.Version, target.Platform),
		RuntimeKey:      profileRuntimeKey(profile),
	}, nil
}

func (manager *Manager) currentProfileConfigHash(document state.Document, adapter core.Adapter, version string) (string, error) {
	hash := document.Configs[adapter.ID()]
	if hash == "" {
		return "", nil
	}
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return "", err
	}
	profile, err := subscriptions.FindProfile(&catalog, document.ActiveProfileID)
	if err != nil {
		return "", err
	}
	if !subscriptionProfileHasInputs(*profile) {
		return hash, nil
	}
	expected, err := expectedConfigBuild(*profile, adapter, version)
	if err != nil {
		return "", err
	}
	if document.ConfigBuilds[adapter.ID()] != expected {
		return "", nil
	}
	return hash, nil
}

func incrementProfilesReferencingNode(catalog *subscriptions.Catalog, nodeID string) {
	for index := range catalog.Profiles {
		for _, referenced := range catalog.Profiles[index].CustomNodeIDs {
			if referenced == nodeID {
				catalog.Profiles[index].Revision++
				break
			}
		}
	}
}

func (manager *Manager) compileActiveProfileForSelectedCore(
	ctx context.Context,
	catalog subscriptions.Catalog,
	profile subscriptions.Profile,
	document state.Document,
) (Change, error) {
	target, runtimeValidation, warnings, err := manager.subscriptionTarget(document)
	if err != nil {
		return Change{}, err
	}
	if !runtimeValidation {
		return Change{}, fmt.Errorf("select and install a core before compiling the active subscription profile")
	}
	rendered, updated, err := manager.compiler.Render(ctx, profile, catalog, target, false)
	if err != nil {
		return Change{}, err
	}
	rendered.Warnings = append(warnings, rendered.Warnings...)
	configHash, err := manager.subscriptions.SaveBlob([]byte(rendered.Content))
	if err != nil {
		return Change{}, fmt.Errorf("persist compiled subscription: %w", err)
	}
	now := time.Now().UTC()
	updated.Revision = profile.Revision
	updated.LastCheck = profile.LastCheck
	updated.LastResult = "configuration compiled"
	updated.LastConfigHash = configHash
	updated.LastRuntimeValidated = true
	updated.LastCompilerTarget = target.Format
	updated.LastCompilerWarnings = append([]string{}, rendered.Warnings...)
	if profile.LastConfigHash != configHash {
		updated.LastChange = now
	}
	if err := manager.subscriptions.Update(func(stored *subscriptions.Catalog) error {
		item, findErr := subscriptions.FindProfile(stored, profile.ID)
		if findErr != nil {
			return findErr
		}
		if item.Revision != profile.Revision {
			return fmt.Errorf("subscription profile changed while compiling; retry the command")
		}
		*item = updated
		return nil
	}); err != nil {
		return Change{}, err
	}
	change, err := manager.activateConfig(ctx, []byte(rendered.Content), configBuild(profile, target), nil)
	if err != nil {
		return Change{}, err
	}
	change.Message = "active profile compiled for " + document.Selected.Core
	return change, nil
}

func (manager *Manager) activeProfile() (subscriptions.Catalog, *subscriptions.Profile, state.Document, error) {
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return subscriptions.Catalog{}, nil, state.Document{}, err
	}
	document, err := manager.store.Read()
	if err != nil {
		return subscriptions.Catalog{}, nil, state.Document{}, err
	}
	profile, err := subscriptions.FindProfile(&catalog, document.ActiveProfileID)
	return catalog, profile, document, err
}

func stringValue(value any) string { result, _ := value.(string); return result }

func subscriptionProfileHasInputs(profile subscriptions.Profile) bool {
	if strings.TrimSpace(profile.Editor.Servers) != "" && strings.TrimSpace(profile.Editor.Servers) != "[]" {
		return true
	}
	if len(profile.CustomNodeIDs) > 0 {
		return true
	}
	for _, source := range profile.Sources {
		if source.Enabled {
			return true
		}
	}
	return false
}

func subscriptionProfileHasScheduledSources(profile subscriptions.Profile) bool {
	for _, source := range profile.Sources {
		if source.Enabled && source.Type == subscriptions.SourceURL {
			return true
		}
	}
	return false
}

func (manager *Manager) importSubscriptionSource(ctx context.Context, path string) (Change, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return Change{}, fmt.Errorf("read subscription source: %w", err)
	}
	if int64(len(data)) > subscriptions.MaxSourceSize {
		return Change{}, fmt.Errorf("subscription source exceeds %d bytes", subscriptions.MaxSourceSize)
	}
	catalog, profile, _, err := manager.activeProfile()
	_ = catalog
	if err != nil {
		return Change{}, err
	}
	candidate := *profile
	candidate.Sources = append(candidate.Sources, subscriptions.Source{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: string(data), Remark: filepathBase(path)})
	change, _, err := manager.SaveSubscriptionProfile(ctx, candidate.ID, candidate)
	return change, err
}

func filepathBase(path string) string {
	parts := strings.FieldsFunc(path, func(r rune) bool { return r == '/' || r == '\\' })
	if len(parts) == 0 {
		return "imported source"
	}
	return parts[len(parts)-1]
}
