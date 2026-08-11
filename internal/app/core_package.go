package app

import (
	"context"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"strings"

	"github.com/tinymins/sempre/internal/archive"
	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/download"
	"github.com/tinymins/sempre/internal/state"
)

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
	if err := archive.Extract(archivePath, extracted, archive.ExtractOptions{
		Format:         item.Format,
		SingleFileName: adapter.ExecutableName(core.CurrentTarget()),
	}); err != nil {
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
