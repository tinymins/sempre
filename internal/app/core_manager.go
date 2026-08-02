package app

import (
	"context"
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
	return change, err
}

func (manager *Manager) installCore(ctx context.Context, value string) (Change, error) {
	reference, err := core.ParseRef(value)
	if err != nil {
		return Change{}, err
	}
	adapter, err := manager.registry.Get(reference.Core)
	if err != nil {
		return Change{}, err
	}
	resolved, err := adapter.Resolve(ctx, reference.Value, core.CurrentTarget())
	if err != nil {
		return Change{}, err
	}
	binary, installed, err := manager.installPackage(ctx, adapter, resolved)
	if err != nil {
		return Change{}, err
	}
	actual, err := adapter.Version(ctx, binary)
	if err != nil {
		if installed {
			_ = os.RemoveAll(manager.paths.CoreVersionDir(reference.Core, resolved.Version))
		}
		return Change{}, err
	}
	if actual != resolved.Version {
		if installed {
			_ = os.RemoveAll(manager.paths.CoreVersionDir(reference.Core, resolved.Version))
		}
		return Change{}, fmt.Errorf("%s reports version %s, expected %s", reference.Core, actual, resolved.Version)
	}
	document, err := manager.store.Read()
	if err != nil {
		return Change{}, err
	}
	if selectionMatches(document.Selected, reference) {
		if configHash := document.Configs[reference.Core]; configHash != "" {
			config := manager.paths.Config(reference.Core, configHash)
			if err := manager.validateConfiguration(ctx, adapter, binary, config, manager.output, manager.errors); err != nil {
				if installed {
					_ = os.RemoveAll(manager.paths.CoreVersionDir(reference.Core, resolved.Version))
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
		installation := coreState.Installed[resolved.Version]
		if installation == nil {
			installation = &state.Installation{}
			coreState.Installed[resolved.Version] = installation
			change.Changed = true
		}
		installation.Digest = resolved.Digest
		installation.Source = resolved.URL
		if installation.InstalledAt.IsZero() {
			installation.InstalledAt = time.Now().UTC()
		}
		if reference.IsChannel() {
			previousVersion = coreState.Channels[reference.Value]
			if previousVersion != resolved.Version {
				coreState.Channels[reference.Value] = resolved.Version
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
		if manager.collectWeakVersion(document, reference.Core, previousVersion) {
			cleanupVersion = previousVersion
		}
		return nil
	})
	if err != nil {
		if installed {
			_ = os.RemoveAll(manager.paths.CoreVersionDir(reference.Core, resolved.Version))
		}
		return Change{}, err
	}
	if cleanupVersion != "" {
		_ = os.RemoveAll(manager.paths.CoreVersionDir(reference.Core, cleanupVersion))
	}
	action := "is already installed"
	if change.Changed {
		action = "installed"
	}
	change.Message = fmt.Sprintf("%s@%s %s", reference.Core, resolved.Version, action)
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
		reference, err := core.ParseRef(value)
		if err != nil {
			return nil, err
		}
		if !reference.IsChannel() {
			return nil, fmt.Errorf("exact core versions are immutable; update a channel such as %s@stable", reference.Core)
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
		for channel := range coreState.Channels {
			references = append(references, name+"@"+channel)
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
	var change Change
	err := manager.withOperation(func() error {
		var err error
		change, err = manager.useCore(ctx, value)
		return err
	})
	return change, err
}

func (manager *Manager) useCore(ctx context.Context, value string) (Change, error) {
	reference, err := core.ParseRef(value)
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
		adapter, err := manager.registry.Get(reference.Core)
		if err != nil {
			return Change{}, err
		}
		binary := manager.paths.CoreBinary(reference.Core, version)
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
		installation := document.Cores[reference.Core].Installed[version]
		if !reference.IsChannel() && !installation.Explicit {
			installation.Explicit = true
			change.Changed = true
		}
		selection := state.Selection{Core: reference.Core, Ref: reference.Value}
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
	reference, err := core.ParseRef(value)
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
	if selectionReferencesVersion(document, reference.Core, version) {
		return Change{}, fmt.Errorf("cannot remove %s@%s: it is selected", reference.Core, version)
	}
	if document.Active != nil && document.Active.Core == reference.Core && document.Active.Version == version {
		return Change{}, fmt.Errorf("cannot remove %s@%s: it is active", reference.Core, version)
	}
	if document.Previous != nil && document.Previous.Core == reference.Core && document.Previous.Version == version {
		return Change{}, fmt.Errorf("cannot remove %s@%s: it is retained for rollback", reference.Core, version)
	}

	versionDir := manager.paths.CoreVersionDir(reference.Core, version)
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
			selectionReferencesVersion(*document, reference.Core, version) ||
			(document.Active != nil && document.Active.Core == reference.Core && document.Active.Version == version) ||
			(document.Previous != nil && document.Previous.Core == reference.Core && document.Previous.Version == version) {
			return fmt.Errorf("core state changed while removing %s@%s; retry the command", reference.Core, version)
		}
		coreState := document.Cores[reference.Core]
		for channel, target := range coreState.Channels {
			if target == version {
				delete(coreState.Channels, channel)
			}
		}
		delete(coreState.Installed, version)
		if len(coreState.Channels) == 0 && len(coreState.Installed) == 0 {
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
			return Change{}, fmt.Errorf("%s@%s removed, but temporary files could not be cleaned up: %w", reference.Core, version, err)
		}
	}
	return Change{
		Changed: true,
		Message: fmt.Sprintf("%s@%s removed", reference.Core, version),
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
		if coreState == nil || len(coreState.Installed) == 0 {
			fmt.Fprintln(&builder, "  not installed")
			continue
		}
		versions := make([]string, 0, len(coreState.Installed))
		for version := range coreState.Installed {
			versions = append(versions, version)
		}
		sort.Strings(versions)
		for _, version := range versions {
			installation := coreState.Installed[version]
			var labels []string
			if installation.Explicit {
				labels = append(labels, "explicit")
			}
			for channel, target := range coreState.Channels {
				if target == version {
					labels = append(labels, channel)
				}
			}
			if document.Active != nil && document.Active.Core == name && document.Active.Version == version {
				labels = append(labels, "active")
			}
			sort.Strings(labels)
			suffix := ""
			if len(labels) > 0 {
				suffix = " [" + strings.Join(labels, ", ") + "]"
			}
			fmt.Fprintf(&builder, "  %s%s\n", version, suffix)
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
		fmt.Fprintf(&builder, "Selected: %s@%s\n", document.Selected.Core, document.Selected.Ref)
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
	item core.Package,
) (string, bool, error) {
	finalDir := manager.paths.CoreVersionDir(adapter.ID(), item.Version)
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

func (manager *Manager) collectWeakVersion(document *state.Document, coreName, version string) bool {
	if version == "" {
		return false
	}
	coreState := document.Cores[coreName]
	if coreState == nil {
		return false
	}
	installation := coreState.Installed[version]
	if installation == nil || installation.Explicit {
		return false
	}
	for _, target := range coreState.Channels {
		if target == version {
			return false
		}
	}
	if selectionReferencesVersion(*document, coreName, version) {
		return false
	}
	if document.Active != nil && document.Active.Core == coreName && document.Active.Version == version {
		return false
	}
	if document.Previous != nil && document.Previous.Core == coreName && document.Previous.Version == version {
		return false
	}
	delete(coreState.Installed, version)
	return true
}

func deploymentLabel(deployment state.Deployment) string {
	return fmt.Sprintf("%s@%s -> %s", deployment.Core, deployment.Ref, deployment.Version)
}

func selectionMatches(selection *state.Selection, reference core.Ref) bool {
	return selection != nil && selection.Core == reference.Core && selection.Ref == reference.Value
}

func resolveInstalledVersion(document state.Document, reference core.Ref) (string, error) {
	coreState := document.Cores[reference.Core]
	if coreState == nil {
		return "", fmt.Errorf("%s is not installed", reference.Core)
	}
	version := reference.Value
	if reference.IsChannel() {
		version = coreState.Channels[reference.Value]
	}
	if version == "" || coreState.Installed[version] == nil {
		return "", fmt.Errorf("%s is not installed; run 'sempre core install %s' first", reference, reference)
	}
	return version, nil
}

func selectionReferencesVersion(document state.Document, coreName, version string) bool {
	if document.Selected == nil || document.Selected.Core != coreName {
		return false
	}
	selected := core.Ref{Core: document.Selected.Core, Value: document.Selected.Ref}
	selectedVersion, err := resolveInstalledVersion(document, selected)
	return err == nil && selectedVersion == version
}
