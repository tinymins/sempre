package app

import (
	"context"
	"fmt"
	"time"

	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func (manager *Manager) RefreshSubscriptionProfile(ctx context.Context, id string) (Change, subscriptions.RenderResult, error) {
	var change Change
	var rendered subscriptions.RenderResult
	err := manager.withOperation(func() error {
		var err error
		change, rendered, err = manager.refreshSubscriptionProfile(ctx, id, false)
		return err
	})
	return change, rendered, err
}

func (manager *Manager) UseSubscriptionProfile(ctx context.Context, id string) (Change, subscriptions.RenderResult, error) {
	document, err := manager.store.Read()
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	if document.ActiveProfileID == id {
		return manager.RefreshSubscriptionProfile(ctx, id)
	}
	var change Change
	var rendered subscriptions.RenderResult
	err = manager.withOperation(func() error {
		var err error
		change, rendered, err = manager.refreshSubscriptionProfile(ctx, id, true)
		return err
	})
	return change, rendered, err
}

func (manager *Manager) RefreshSubscriptionProfileAsActive(ctx context.Context, id string, catalog subscriptions.Catalog) (Change, subscriptions.RenderResult, error) {
	_ = catalog
	return manager.refreshSubscriptionProfile(ctx, id, true)
}

func (manager *Manager) refreshSubscriptionProfile(ctx context.Context, id string, activate bool) (Change, subscriptions.RenderResult, error) {
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	profile, err := subscriptions.FindProfile(&catalog, id)
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	document, err := manager.store.Read()
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	rendered, updated, target, err := manager.renderSubscriptionForRuntime(ctx, catalog, *profile, document, true)
	if err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
	now := time.Now().UTC()
	configHash := configContentHash(rendered.Content)
	active := activate || document.ActiveProfileID == id
	change := Change{Changed: true, Message: "subscription profile refreshed, compiled, and validated"}
	if active {
		change, err = manager.activateConfig(ctx, []byte(rendered.Content), configBuild(*profile, target), func(document *state.Document, changed bool) {
			if activate {
				document.ActiveProfileID = id
			}
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
		change.Message = "subscription profile refreshed, validated, and staged"
	} else {
		if err := manager.ValidateConfigContent(ctx, []byte(rendered.Content)); err != nil {
			return Change{}, subscriptions.RenderResult{}, err
		}
		if _, err := manager.subscriptions.SaveBlob([]byte(rendered.Content)); err != nil {
			return Change{}, subscriptions.RenderResult{}, fmt.Errorf("persist compiled subscription: %w", err)
		}
	}
	if err := manager.recordSubscriptionCompilation(*profile, updated, rendered, configHash, now); err != nil {
		return Change{}, subscriptions.RenderResult{}, err
	}
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
		if profile.Mode == subscriptions.ProfileRemote {
			result, _, err = manager.remote.Render(ctx, *profile, target)
		} else {
			result, _, err = manager.compiler.Render(ctx, *profile, catalog, target, force)
		}
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
