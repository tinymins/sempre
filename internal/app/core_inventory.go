package app

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/tinymins/sempre/internal/state"
)

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
