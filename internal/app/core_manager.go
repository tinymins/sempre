package app

import (
	"context"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/archive"
	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/download"
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
	if selectionMatches(document.Selected, reference) {
		if configHash := document.Configs[reference.Core]; configHash != "" {
			config := manager.paths.Config(reference.Core, configHash)
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
			if configHash := document.Configs[reference.Core]; configHash != "" {
				deployment := state.Deployment{
					Core:       reference.Core,
					Repository: reference.Repository,
					Ref:        reference.Value,
					Version:    resolved.Version,
					ConfigHash: configHash,
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
		if readErr == nil && after.Selected != nil && !sameSelection(before.Selected, after.Selected) && after.Configs[after.Selected.Core] == "" {
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
	if document.Selected == nil || document.Configs[document.Selected.Core] != "" {
		return change, nil
	}
	_, profile, _, err := manager.activeProfile()
	if err != nil {
		return Change{}, err
	}
	if !subscriptionProfileHasInputs(*profile) {
		return change, nil
	}
	compiled, err := manager.UpdateSubscription(ctx)
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
	configHash := document.Configs[reference.Core]
	if configHash != "" {
		binary := manager.paths.CoreBinary(reference.Core, reference.Repository, version)
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
		if currentVersion != version || document.Configs[reference.Core] != configHash {
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

func (manager *Manager) RemoveCore(value string) (Change, error) {
	var change Change
	err := manager.withOperation(func() error {
		var err error
		change, err = manager.removeCore(value)
		return err
	})
	return change, err
}

func (manager *Manager) removeCore(value string) (Change, error) {
	reference, _, err := manager.resolveReference(value)
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
	if selectionReferencesVersion(document, reference.Core, reference.Repository, version) {
		return Change{}, fmt.Errorf("cannot remove %s: it is selected", exactRef(reference, version))
	}
	if deploymentReferencesVersion(document.Active, reference.Core, reference.Repository, version) {
		return Change{}, fmt.Errorf("cannot remove %s: it is active", exactRef(reference, version))
	}
	if deploymentReferencesVersion(document.Previous, reference.Core, reference.Repository, version) {
		return Change{}, fmt.Errorf("cannot remove %s: it is retained for rollback", exactRef(reference, version))
	}

	versionDir := manager.paths.CoreVersionDir(reference.Core, reference.Repository, version)
	removedDir := ""
	if _, err := os.Stat(versionDir); err == nil {
		parent := filepath.Dir(versionDir)
		removedDir, err = os.MkdirTemp(parent, ".remove-"+version+"-*")
		if err != nil {
			return Change{}, err
		}
		if err := os.Remove(removedDir); err != nil {
			return Change{}, err
		}
		if err := os.Rename(versionDir, removedDir); err != nil {
			return Change{}, fmt.Errorf("prepare core removal: %w", err)
		}
	} else if !os.IsNotExist(err) {
		return Change{}, err
	}

	err = manager.store.Update(func(document *state.Document) error {
		currentVersion, err := resolveInstalledVersion(*document, reference)
		if err != nil {
			return err
		}
		if currentVersion != version ||
			selectionReferencesVersion(*document, reference.Core, reference.Repository, version) ||
			deploymentReferencesVersion(document.Active, reference.Core, reference.Repository, version) ||
			deploymentReferencesVersion(document.Previous, reference.Core, reference.Repository, version) {
			return fmt.Errorf("core state changed while removing %s; retry the command", exactRef(reference, version))
		}
		coreState := document.Cores[reference.Core]
		source := coreState.LookupSource(reference.Repository)
		for channel, target := range source.Channels {
			if target == version {
				delete(source.Channels, channel)
			}
		}
		delete(source.Installed, version)
		if reference.Repository != "" && len(source.Channels) == 0 && len(source.Installed) == 0 {
			delete(coreState.Custom, reference.Repository)
		}
		if coreState.Empty() {
			delete(document.Cores, reference.Core)
		}
		return nil
	})
	if err != nil {
		if removedDir != "" {
			_ = os.Rename(removedDir, versionDir)
		}
		return Change{}, err
	}
	if removedDir != "" {
		if err := os.RemoveAll(removedDir); err != nil {
			return Change{}, fmt.Errorf("%s removed, but temporary files could not be cleaned up: %w", exactRef(reference, version), err)
		}
	}
	return Change{
		Changed: true,
		Message: fmt.Sprintf("%s removed", exactRef(reference, version)),
	}, nil
}

func (manager *Manager) ListCores(filter string) (string, error) {
	document, err := manager.store.Read()
	if err != nil {
		return "", err
	}
	var builder strings.Builder
	ids := manager.CoreIDs()
	if filter != "" {
		if _, err := manager.registry.Get(filter); err != nil {
			return "", err
		}
		ids = []string{filter}
	}
	for _, name := range ids {
		fmt.Fprintln(&builder, name)
		coreState := document.Cores[name]
		if coreState == nil || coreState.Empty() {
			fmt.Fprintln(&builder, "  not installed")
			continue
		}
		adapter, _ := manager.registry.Get(name)
		entries := coreState.SourceEntries()
		sort.Slice(entries, func(i, j int) bool { return entries[i].Repository < entries[j].Repository })
		for _, entry := range entries {
			if len(entry.State.Installed) == 0 {
				continue
			}
			repository := entry.Repository
			kind := "custom"
			if repository == "" {
				repository = adapter.DefaultRepository()
				kind = "default"
			}
			fmt.Fprintf(&builder, "  %s [%s]\n", repository, kind)
			versions := make([]string, 0, len(entry.State.Installed))
			for version := range entry.State.Installed {
				versions = append(versions, version)
			}
			sort.Strings(versions)
			for _, version := range versions {
				installation := entry.State.Installed[version]
				var labels []string
				if installation.Explicit {
					labels = append(labels, "explicit")
				}
				for channel, target := range entry.State.Channels {
					if target == version {
						labels = append(labels, channel)
					}
				}
				if deploymentReferencesVersion(document.Active, name, entry.Repository, version) {
					labels = append(labels, "active")
				}
				if selectionReferencesVersion(document, name, entry.Repository, version) {
					labels = append(labels, "selected")
				}
				sort.Strings(labels)
				suffix := ""
				if len(labels) > 0 {
					suffix = " [" + strings.Join(labels, ", ") + "]"
				}
				fmt.Fprintf(&builder, "    %s%s\n", version, suffix)
			}
		}
	}
	return strings.TrimRight(builder.String(), "\n"), nil
}

func (manager *Manager) CurrentCore() (string, error) {
	document, err := manager.store.Read()
	if err != nil {
		return "", err
	}
	var builder strings.Builder
	if document.Selected == nil {
		fmt.Fprintln(&builder, "Selected: none")
	} else {
		fmt.Fprintf(&builder, "Selected: %s\n", selectionRef(*document.Selected))
	}
	if document.Active == nil {
		fmt.Fprintln(&builder, "Active: none")
		return strings.TrimRight(builder.String(), "\n"), nil
	}
	label := deploymentLabel(*document.Active)
	if document.Pending {
		label += " (pending validation)"
	}
	fmt.Fprintln(&builder, "Active:", label)
	return strings.TrimRight(builder.String(), "\n"), nil
}

func (manager *Manager) installPackage(
	ctx context.Context,
	adapter core.Adapter,
	repository string,
	item core.Package,
) (string, bool, error) {
	finalDir := manager.paths.CoreVersionDir(adapter.ID(), repository, item.Version)
	finalBinary := filepath.Join(finalDir, adapter.ExecutableName(core.CurrentTarget()))
	if _, err := os.Stat(finalBinary); err == nil {
		return finalBinary, false, nil
	} else if !os.IsNotExist(err) {
		return "", false, err
	}

	temporary, err := os.MkdirTemp(manager.paths.Runtime, "core-install-*")
	if err != nil {
		return "", false, err
	}
	defer os.RemoveAll(temporary)
	archivePath := filepath.Join(temporary, item.Name)
	if err := download.Verified(ctx, download.Artifact{
		Name:   item.Name,
		URL:    item.URL,
		Digest: item.Digest,
		Size:   item.Size,
	}, archivePath); err != nil {
		return "", false, err
	}
	extracted := filepath.Join(temporary, "extract")
	if err := archive.Extract(archivePath, extracted, item.Format); err != nil {
		return "", false, err
	}
	sourceBinary, err := archive.Find(extracted, adapter.ExecutableName(core.CurrentTarget()))
	if err != nil {
		return "", false, err
	}
	sourceDir := filepath.Dir(sourceBinary)
	if err := os.MkdirAll(filepath.Dir(finalDir), 0o700); err != nil {
		return "", false, err
	}
	staging, err := os.MkdirTemp(filepath.Dir(finalDir), "."+item.Version+"-*")
	if err != nil {
		return "", false, err
	}
	defer os.RemoveAll(staging)
	if err := copyTree(sourceDir, staging); err != nil {
		return "", false, err
	}
	stagingBinary := filepath.Join(staging, adapter.ExecutableName(core.CurrentTarget()))
	if err := os.Chmod(stagingBinary, 0o755); err != nil {
		return "", false, err
	}
	actual, err := adapter.Version(ctx, stagingBinary)
	if err != nil {
		return "", false, err
	}
	if actual != item.Version {
		return "", false, fmt.Errorf("downloaded %s reports version %s", adapter.ID(), actual)
	}
	if err := os.Rename(staging, finalDir); err != nil {
		if _, statErr := os.Stat(finalBinary); statErr == nil {
			return finalBinary, false, nil
		}
		return "", false, fmt.Errorf("activate core version: %w", err)
	}
	return finalBinary, true, nil
}

func copyTree(source, destination string) error {
	return filepath.WalkDir(source, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		relative, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		target := filepath.Join(destination, relative)
		if entry.IsDir() {
			return os.MkdirAll(target, 0o700)
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		if !info.Mode().IsRegular() {
			return nil
		}
		input, err := os.Open(path)
		if err != nil {
			return err
		}
		output, err := os.OpenFile(target, os.O_CREATE|os.O_EXCL|os.O_WRONLY, info.Mode().Perm())
		if err != nil {
			input.Close()
			return err
		}
		_, copyErr := io.Copy(output, input)
		inputCloseErr := input.Close()
		closeErr := output.Close()
		if copyErr != nil {
			return copyErr
		}
		if inputCloseErr != nil {
			return inputCloseErr
		}
		return closeErr
	})
}

func (manager *Manager) collectWeakVersion(document *state.Document, coreName, repository, version string) bool {
	if version == "" {
		return false
	}
	coreState := document.Cores[coreName]
	if coreState == nil {
		return false
	}
	source := coreState.LookupSource(repository)
	if source == nil {
		return false
	}
	installation := source.Installed[version]
	if installation == nil || installation.Explicit {
		return false
	}
	for _, target := range source.Channels {
		if target == version {
			return false
		}
	}
	if selectionReferencesVersion(*document, coreName, repository, version) {
		return false
	}
	if deploymentReferencesVersion(document.Active, coreName, repository, version) {
		return false
	}
	if deploymentReferencesVersion(document.Previous, coreName, repository, version) {
		return false
	}
	delete(source.Installed, version)
	if repository != "" && len(source.Channels) == 0 && len(source.Installed) == 0 {
		delete(coreState.Custom, repository)
	}
	return true
}

func deploymentLabel(deployment state.Deployment) string {
	return fmt.Sprintf("%s -> %s", core.Ref{Core: deployment.Core, Repository: deployment.Repository, Value: deployment.Ref}, deployment.Version)
}

func selectionMatches(selection *state.Selection, reference core.Ref) bool {
	return selection != nil && selection.Core == reference.Core && selection.Repository == reference.Repository && selection.Ref == reference.Value
}

func resolveInstalledVersion(document state.Document, reference core.Ref) (string, error) {
	coreState := document.Cores[reference.Core]
	if coreState == nil {
		return "", fmt.Errorf("%s is not installed", reference.Core)
	}
	source := coreState.LookupSource(reference.Repository)
	if source == nil {
		return "", fmt.Errorf("%s is not installed", reference)
	}
	version := reference.Value
	if reference.IsChannel() {
		version = source.Channels[reference.Value]
	}
	if version == "" || source.Installed[version] == nil {
		return "", fmt.Errorf("%s is not installed; run 'sempre core install %s' first", reference, reference)
	}
	return version, nil
}

func selectionReferencesVersion(document state.Document, coreName, repository, version string) bool {
	if document.Selected == nil || document.Selected.Core != coreName || document.Selected.Repository != repository {
		return false
	}
	selected := core.Ref{Core: document.Selected.Core, Repository: document.Selected.Repository, Value: document.Selected.Ref}
	selectedVersion, err := resolveInstalledVersion(document, selected)
	return err == nil && selectedVersion == version
}

func deploymentReferencesVersion(deployment *state.Deployment, coreName, repository, version string) bool {
	return deployment != nil && deployment.Core == coreName && deployment.Repository == repository && deployment.Version == version
}

func selectionRef(selection state.Selection) core.Ref {
	return core.Ref{Core: selection.Core, Repository: selection.Repository, Value: selection.Ref}
}

func exactRef(reference core.Ref, version string) core.Ref {
	reference.Value = version
	return reference
}

func (manager *Manager) resolveReference(value string) (core.Ref, core.Adapter, error) {
	reference, err := core.ParseRef(value)
	if err != nil {
		return core.Ref{}, nil, err
	}
	adapter, err := manager.registry.Get(reference.Core)
	if err != nil {
		return core.Ref{}, nil, err
	}
	if strings.EqualFold(reference.Repository, adapter.DefaultRepository()) {
		reference.Repository = ""
	}
	return reference, adapter, nil
}
