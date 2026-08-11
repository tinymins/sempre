package app

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

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
