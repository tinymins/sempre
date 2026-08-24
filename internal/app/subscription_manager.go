package app

import (
	"context"
	"errors"
	"fmt"
	"runtime"
	"strings"

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

func (manager *Manager) saveSubscriptionProfile(_ context.Context, id string, candidate subscriptions.Profile, expectedContext string) (Change, subscriptions.RenderResult, error) {
	var change Change
	err := manager.withOperation(func() error {
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
		return manager.subscriptions.Update(func(stored *subscriptions.Catalog) error {
			current, err := subscriptions.FindProfile(stored, id)
			if err != nil {
				return err
			}
			candidate.ID = id
			candidate.Name = current.Name
			candidate.Revision = current.Revision + 1
			candidate.Sources = preserveSubscriptionSnapshots(current.Sources, candidate.Sources)
			candidate.LastCheck = current.LastCheck
			candidate.LastChange = current.LastChange
			candidate.LastResult = "profile saved; runtime configuration needs regeneration"
			candidate.LastConfigHash = current.LastConfigHash
			candidate.LastRuntimeValidated = false
			candidate.LastCompilerTarget = current.LastCompilerTarget
			candidate.LastCompilerWarnings = append([]string{}, current.LastCompilerWarnings...)
			*current = candidate
			return nil
		})
	})
	if err == nil {
		change = Change{
			Changed:      true,
			NeedsRestart: manager.activeProfileNeedsRuntimePreparation(id),
			Message:      "subscription profile saved locally; runtime configuration needs regeneration",
		}
	}
	return change, subscriptions.RenderResult{}, err
}

func preserveSubscriptionSnapshots(previous, candidate []subscriptions.Source) []subscriptions.Source {
	byID := make(map[string]subscriptions.Source, len(previous))
	for _, source := range previous {
		byID[source.ID] = source
	}
	for index := range candidate {
		before, found := byID[candidate[index].ID]
		if !found || !sameSubscriptionFetchIdentity(before, candidate[index]) {
			continue
		}
		candidate[index].SnapshotHash = before.SnapshotHash
		candidate[index].FetchedAt = before.FetchedAt
		candidate[index].LastStatus = before.LastStatus
		candidate[index].LastError = before.LastError
	}
	return candidate
}

func sameSubscriptionFetchIdentity(left, right subscriptions.Source) bool {
	userAgent := func(value string) string {
		if value == "" {
			return subscriptions.DefaultUserAgent
		}
		return value
	}
	fetchMode := func(value string) string {
		if value == "" {
			return subscriptions.FetchAuto
		}
		return value
	}
	return left.Type == right.Type && left.URL == right.URL &&
		userAgent(left.UserAgent) == userAgent(right.UserAgent) &&
		fetchMode(left.FetchMode) == fetchMode(right.FetchMode)
}

func (manager *Manager) activeProfileNeedsRuntimePreparation(id string) bool {
	document, err := manager.store.Read()
	return err == nil && document.Selected != nil && document.ActiveProfileID == id
}

func normalizeSavedTransparentProxy(profile *subscriptions.Profile) error {
	profile.TransparentProxy.TUN.InterfaceName = strings.TrimSpace(profile.TransparentProxy.TUN.InterfaceName)
	if profile.TransparentProxy.TUN.InterfaceName == "" {
		return fmt.Errorf("TUN interface name must contain 1 to 15 characters")
	}
	return nil
}
