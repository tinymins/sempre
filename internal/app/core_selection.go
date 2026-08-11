package app

import (
	"context"
	"errors"
	"fmt"

	"github.com/tinymins/sempre/internal/state"
)

func (manager *Manager) UseCore(ctx context.Context, value string) (Change, error) {
	before, err := manager.store.Read()
	if err != nil {
		return Change{}, err
	}
	var change Change
	err = manager.withOperation(func() error {
		var err error
		change, err = manager.useCore(ctx, value)
		return err
	})
	if err == nil {
		change, err = manager.compileSelectedProfileIfNeeded(ctx, change)
	}
	if err != nil {
		after, readErr := manager.store.Read()
		if readErr == nil && after.Selected != nil && !sameSelection(before.Selected, after.Selected) {
			rollbackErr := manager.withOperation(func() error {
				return manager.store.Update(func(document *state.Document) error {
					if sameSelection(document.Selected, after.Selected) {
						document.Selected = cloneSelection(before.Selected)
					}
					return nil
				})
			})
			if rollbackErr != nil {
				err = errors.Join(err, fmt.Errorf("restore previous core selection: %w", rollbackErr))
			}
		}
	}
	return change, err
}

func sameSelection(left, right *state.Selection) bool {
	return (left == nil && right == nil) || (left != nil && right != nil && *left == *right)
}

func cloneSelection(selection *state.Selection) *state.Selection {
	if selection == nil {
		return nil
	}
	result := *selection
	return &result
}

func (manager *Manager) compileSelectedProfileIfNeeded(ctx context.Context, change Change) (Change, error) {
	document, err := manager.store.Read()
	if err != nil {
		return Change{}, err
	}
	if document.Selected == nil {
		return change, nil
	}
	catalog, profile, _, err := manager.activeProfile()
	if err != nil {
		return Change{}, err
	}
	if !subscriptionProfileHasInputs(*profile) {
		return change, nil
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
		return change, nil
	}
	compiled, err := manager.compileActiveProfileForSelectedCore(ctx, catalog, *profile, document)
	if err != nil {
		return Change{}, err
	}
	change.Changed = change.Changed || compiled.Changed
	change.NeedsRestart = change.NeedsRestart || compiled.NeedsRestart
	change.CurrentDetail = compiled.CurrentDetail
	change.Message = compiled.Message
	return change, nil
}

func (manager *Manager) useCore(ctx context.Context, value string) (Change, error) {
	reference, adapter, err := manager.resolveReference(value)
	if err != nil {
		return Change{}, err
	}
	document, err := manager.store.Read()
	if err != nil {
		return Change{}, err
	}
	version, err := resolveInstalledVersion(document, reference)
	if err != nil {
		return Change{}, err
	}
	storedConfigHash := document.Configs[reference.Core]
	configHash, err := manager.currentProfileConfigHash(document, adapter, version)
	if err != nil {
		return Change{}, err
	}
	if configHash != "" {
		binary := coreBinaryPath(manager.paths, adapter, reference.Repository, version)
		config := manager.paths.Config(reference.Core, configHash)
		if err := manager.validateConfiguration(ctx, adapter, binary, config, manager.output, manager.errors); err != nil {
			return Change{}, fmt.Errorf("candidate %s rejected the active configuration: %w", reference, err)
		}
	}
	change := Change{}
	err = manager.store.Update(func(document *state.Document) error {
		currentVersion, err := resolveInstalledVersion(*document, reference)
		if err != nil {
			return err
		}
		if currentVersion != version || document.Configs[reference.Core] != storedConfigHash {
			return fmt.Errorf("core state changed while selecting %s; retry the command", reference)
		}
		installation := document.Cores[reference.Core].LookupSource(reference.Repository).Installed[version]
		if !reference.IsChannel() && !installation.Explicit {
			installation.Explicit = true
			change.Changed = true
		}
		selection := state.Selection{Core: reference.Core, Repository: reference.Repository, Ref: reference.Value}
		selectionChanged := document.Selected == nil || *document.Selected != selection
		if selectionChanged {
			document.Selected = &selection
			change.Changed = true
		}
		if configHash == "" {
			change.CurrentDetail = reference.String() + " (waiting for configuration)"
			return nil
		}
		deployment := state.Deployment{
			Core:       reference.Core,
			Repository: reference.Repository,
			Ref:        reference.Value,
			Version:    version,
			ConfigHash: configHash,
		}
		if state.SameDeployment(document.Active, &deployment) {
			return nil
		}
		if document.Active != nil {
			change.PreviousDetail = deploymentLabel(*document.Active)
		}
		document.Stage(deployment)
		change.Changed = true
		change.NeedsRestart = true
		change.CurrentDetail = deploymentLabel(deployment)
		return nil
	})
	if err != nil {
		return Change{}, err
	}
	if change.Changed {
		change.Message = "selected core changed"
	} else {
		change.Message = "core selection is already current"
	}
	return change, nil
}
