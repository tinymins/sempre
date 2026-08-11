package app

import (
	"context"
	"errors"
	"fmt"
	"runtime"
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
