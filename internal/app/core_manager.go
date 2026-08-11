package app

import (
	"context"
	"fmt"
	"os"
	"sort"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
)

func (manager *Manager) InstallCore(ctx context.Context, value string) (Change, error) {
	var change Change
	err := manager.withOperation(func() error {
		var err error
		change, err = manager.installCore(ctx, value)
		return err
	})
	if err == nil {
		change, err = manager.compileSelectedProfileIfNeeded(ctx, change)
	}
	return change, err
}

func (manager *Manager) installCore(ctx context.Context, value string) (Change, error) {
	reference, adapter, err := manager.resolveReference(value)
	if err != nil {
		return Change{}, err
	}
	resolved, err := adapter.Resolve(ctx, reference.Repository, reference.Value, core.CurrentTarget())
	if err != nil {
		return Change{}, err
	}
	document, err := manager.store.Read()
	if err != nil {
		return Change{}, err
	}
	if coreState := document.Cores[reference.Core]; coreState != nil {
		if source := coreState.LookupSource(reference.Repository); source != nil {
			if installation := source.Installed[resolved.Version]; installation != nil && installation.Source != "" && installation.Source != resolved.URL {
				return Change{}, fmt.Errorf("%s is already installed from %s; remove it before installing a different artifact", exactRef(reference, resolved.Version), installation.Source)
			}
		}
	}
	binary, installed, err := manager.installPackage(ctx, adapter, reference.Repository, resolved)
	if err != nil {
		return Change{}, err
	}
	actual, err := adapter.Version(ctx, binary)
	if err != nil {
		if installed {
			_ = os.RemoveAll(manager.paths.CoreVersionDir(reference.Core, reference.Repository, resolved.Version))
		}
		return Change{}, err
	}
	if actual != resolved.Version {
		if installed {
			_ = os.RemoveAll(manager.paths.CoreVersionDir(reference.Core, reference.Repository, resolved.Version))
		}
		return Change{}, fmt.Errorf("%s reports version %s, expected %s", reference.Core, actual, resolved.Version)
	}
	document, err = manager.store.Read()
	if err != nil {
		return Change{}, err
	}
	selectedConfigHash := ""
	if selectionMatches(document.Selected, reference) {
		selectedConfigHash, err = manager.currentProfileConfigHash(document, adapter, resolved.Version)
		if err != nil {
			return Change{}, err
		}
		if selectedConfigHash != "" {
			config := manager.paths.Config(reference.Core, selectedConfigHash)
			if err := manager.validateConfiguration(ctx, adapter, binary, config, manager.output, manager.errors); err != nil {
				if installed {
					_ = os.RemoveAll(manager.paths.CoreVersionDir(reference.Core, reference.Repository, resolved.Version))
				}
				return Change{}, fmt.Errorf("candidate %s@%s rejected the active configuration: %w", reference.Core, resolved.Version, err)
			}
		}
	}

	change := Change{Changed: installed}
	var previousVersion string
	var cleanupVersion string
	err = manager.store.Update(func(document *state.Document) error {
		coreState := document.Core(reference.Core)
		source := coreState.Source(reference.Repository)
		installation := source.Installed[resolved.Version]
		if installation == nil {
			installation = &state.Installation{}
			source.Installed[resolved.Version] = installation
			change.Changed = true
		}
		installation.Digest = resolved.Digest
		installation.Source = resolved.URL
		if installation.InstalledAt.IsZero() {
			installation.InstalledAt = time.Now().UTC()
		}
		if reference.IsChannel() {
			previousVersion = source.Channels[reference.Value]
			if previousVersion != resolved.Version {
				source.Channels[reference.Value] = resolved.Version
				change.Changed = true
			}
		} else if !installation.Explicit {
			installation.Explicit = true
			change.Changed = true
		}

		if selectionMatches(document.Selected, reference) {
			if selectedConfigHash != "" {
				deployment := state.Deployment{
					Core:       reference.Core,
					Repository: reference.Repository,
					Ref:        reference.Value,
					Version:    resolved.Version,
					ConfigHash: selectedConfigHash,
				}
				if !state.SameDeployment(document.Active, &deployment) {
					document.Stage(deployment)
					change.NeedsRestart = true
				}
			}
		}
		if manager.collectWeakVersion(document, reference.Core, reference.Repository, previousVersion) {
			cleanupVersion = previousVersion
		}
		return nil
	})
	if err != nil {
		if installed {
			_ = os.RemoveAll(manager.paths.CoreVersionDir(reference.Core, reference.Repository, resolved.Version))
		}
		return Change{}, err
	}
	if cleanupVersion != "" {
		_ = os.RemoveAll(manager.paths.CoreVersionDir(reference.Core, reference.Repository, cleanupVersion))
	}
	action := "is already installed"
	if change.Changed {
		action = "installed"
	}
	change.Message = fmt.Sprintf("%s %s", exactRef(reference, resolved.Version), action)
	if reference.IsChannel() {
		change.CurrentDetail = reference.Value + " -> " + resolved.Version
		if previousVersion != "" && previousVersion != resolved.Version {
			change.PreviousDetail = reference.Value + " -> " + previousVersion
		}
	}
	return change, nil
}

func (manager *Manager) UpdateCores(ctx context.Context, value string) ([]Change, error) {
	var changes []Change
	err := manager.withOperation(func() error {
		var err error
		changes, err = manager.updateCores(ctx, value)
		return err
	})
	if err == nil {
		compiled, compileErr := manager.compileSelectedProfileIfNeeded(ctx, Change{})
		if compileErr != nil {
			return nil, compileErr
		}
		if compiled.Changed {
			changes = append(changes, compiled)
		}
	}
	return changes, err
}

func (manager *Manager) updateCores(ctx context.Context, value string) ([]Change, error) {
	if value != "" {
		reference, _, err := manager.resolveReference(value)
		if err != nil {
			return nil, err
		}
		if !reference.IsChannel() {
			return nil, fmt.Errorf("exact core versions are immutable; update a channel such as %s", core.Ref{Core: reference.Core, Repository: reference.Repository, Value: core.Stable})
		}
		change, err := manager.installCore(ctx, reference.String())
		if err != nil {
			return nil, err
		}
		return []Change{change}, nil
	}

	document, err := manager.store.Read()
	if err != nil {
		return nil, err
	}
	var references []string
	for name, coreState := range document.Cores {
		for _, source := range coreState.SourceEntries() {
			for channel := range source.State.Channels {
				references = append(references, core.Ref{Core: name, Repository: source.Repository, Value: channel}.String())
			}
		}
	}
	sort.Strings(references)
	if len(references) == 0 {
		return nil, fmt.Errorf("no core channels are installed")
	}
	changes := make([]Change, 0, len(references))
	for _, reference := range references {
		change, err := manager.installCore(ctx, reference)
		if err != nil {
			return nil, err
		}
		changes = append(changes, change)
	}
	return changes, nil
}
