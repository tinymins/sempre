package app

import (
	"context"
	"fmt"
	"time"

	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

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
