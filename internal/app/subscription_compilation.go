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
	rendered, updated, target, err := manager.renderSubscriptionForRuntime(ctx, catalog, profile, document, false)
	if err != nil {
		return Change{}, err
	}
	change, err := manager.activateConfig(ctx, []byte(rendered.Content), configBuild(profile, target), nil)
	if err != nil {
		return Change{}, err
	}
	if err := manager.recordSubscriptionCompilation(profile, updated, rendered, configContentHash(rendered.Content), time.Now().UTC()); err != nil {
		return Change{}, err
	}
	change.Message = "active profile compiled for " + document.Selected.Core
	return change, nil
}

func (manager *Manager) renderSubscriptionForRuntime(
	ctx context.Context,
	catalog subscriptions.Catalog,
	profile subscriptions.Profile,
	document state.Document,
	refresh bool,
) (subscriptions.RenderResult, subscriptions.Profile, subscriptions.Target, error) {
	preparedCatalog, prepared, err := prepareSubscriptionProfile(catalog, profile)
	if err != nil {
		return subscriptions.RenderResult{}, subscriptions.Profile{}, subscriptions.Target{}, err
	}
	target, runtimeValidation, warnings, err := manager.subscriptionTarget(document)
	if err != nil {
		return subscriptions.RenderResult{}, subscriptions.Profile{}, subscriptions.Target{}, err
	}
	if !runtimeValidation {
		return subscriptions.RenderResult{}, subscriptions.Profile{}, subscriptions.Target{}, fmt.Errorf("select and install a core before compiling the active subscription profile")
	}
	var rendered subscriptions.RenderResult
	var updated subscriptions.Profile
	if refresh {
		rendered, updated, err = manager.compiler.Render(ctx, prepared, preparedCatalog, target, true)
	} else {
		rendered, updated, err = manager.compiler.RenderCached(ctx, prepared, preparedCatalog, target)
	}
	if err != nil {
		return subscriptions.RenderResult{}, subscriptions.Profile{}, subscriptions.Target{}, err
	}
	rendered.Warnings = append(warnings, rendered.Warnings...)
	rendered.RuntimeValidated = true
	return rendered, updated, target, nil
}

func prepareSubscriptionProfile(catalog subscriptions.Catalog, profile subscriptions.Profile) (subscriptions.Catalog, subscriptions.Profile, error) {
	prepared := profile
	if err := subscriptions.ApplyEditorConfig(&prepared); err != nil {
		return subscriptions.Catalog{}, subscriptions.Profile{}, err
	}
	if err := normalizeSavedTransparentProxy(&prepared); err != nil {
		return subscriptions.Catalog{}, subscriptions.Profile{}, err
	}
	preparedCatalog := catalog
	preparedCatalog.Profiles = append([]subscriptions.Profile{}, catalog.Profiles...)
	item, err := subscriptions.FindProfile(&preparedCatalog, profile.ID)
	if err != nil {
		return subscriptions.Catalog{}, subscriptions.Profile{}, err
	}
	*item = prepared
	if err := subscriptions.ValidateCatalog(preparedCatalog); err != nil {
		return subscriptions.Catalog{}, subscriptions.Profile{}, err
	}
	return preparedCatalog, prepared, nil
}

func configContentHash(content string) string {
	sum := sha256.Sum256([]byte(content))
	return hex.EncodeToString(sum[:])
}

func (manager *Manager) recordSubscriptionCompilation(
	profile subscriptions.Profile,
	updated subscriptions.Profile,
	rendered subscriptions.RenderResult,
	configHash string,
	now time.Time,
) error {
	return manager.subscriptions.Update(func(stored *subscriptions.Catalog) error {
		item, err := subscriptions.FindProfile(stored, profile.ID)
		if err != nil {
			return err
		}
		if item.Revision != profile.Revision {
			return fmt.Errorf("subscription profile changed while compiling; retry the command")
		}
		item.Sources = preserveSubscriptionSnapshots(updated.Sources, item.Sources)
		item.LastCheck = now
		item.LastResult = "configuration compiled and runtime validated"
		if item.LastConfigHash != configHash {
			item.LastChange = now
		}
		item.LastConfigHash = configHash
		item.LastRuntimeValidated = true
		item.LastCompilerTarget = rendered.Format
		item.LastCompilerWarnings = append([]string{}, rendered.Warnings...)
		return nil
	})
}

func (manager *Manager) prepareActiveProfileForRuntime(ctx context.Context) (Change, error) {
	catalog, profile, document, err := manager.activeProfile()
	if err != nil {
		return Change{}, err
	}
	if document.Selected == nil {
		return Change{}, nil
	}
	deployment, adapter, err := manager.configurationTarget(document)
	if err != nil {
		return Change{}, err
	}
	expected, err := expectedConfigBuild(*profile, adapter, deployment.Version)
	if err != nil {
		return Change{}, err
	}
	if document.Configs[deployment.Core] != "" && document.ConfigBuilds[deployment.Core] == expected {
		return Change{}, nil
	}
	if subscriptionProfileHasInputs(*profile) {
		return manager.compileActiveProfileForSelectedCore(ctx, catalog, *profile, document)
	}
	if initialConfigurationWithoutProfileBuild(*profile, document.ConfigBuilds[deployment.Core]) {
		return Change{}, nil
	}
	if deployment.ConfigHash == "" {
		return Change{}, nil
	}
	if _, _, err := prepareSubscriptionProfile(catalog, *profile); err != nil {
		return Change{}, err
	}
	data, err := os.ReadFile(manager.paths.Config(deployment.Core, deployment.ConfigHash))
	if err != nil {
		return Change{}, fmt.Errorf("read active configuration: %w", err)
	}
	target, _, _, err := manager.subscriptionTarget(document)
	if err != nil {
		return Change{}, err
	}
	return manager.activateConfig(ctx, data, configBuild(*profile, target), nil)
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

func initialConfigurationWithoutProfileBuild(profile subscriptions.Profile, build state.ConfigBuild) bool {
	return !subscriptionProfileHasInputs(profile) && profile.Revision == 1 && build.ProfileID == ""
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
	if err != nil {
		return Change{}, err
	}
	var prepared Change
	err = manager.withOperation(func() error {
		var prepareErr error
		prepared, prepareErr = manager.prepareActiveProfileForRuntime(ctx)
		return prepareErr
	})
	if err == nil {
		change.Changed = change.Changed || prepared.Changed
		change.NeedsRestart = change.NeedsRestart || prepared.NeedsRestart
		change.Message = prepared.Message
	}
	return change, err
}

func filepathBase(path string) string {
	parts := strings.FieldsFunc(path, func(r rune) bool { return r == '/' || r == '\\' })
	if len(parts) == 0 {
		return "imported source"
	}
	return parts[len(parts)-1]
}
