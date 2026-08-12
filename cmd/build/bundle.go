package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/archive"
	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/download"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"github.com/tinymins/sempre/internal/tunnel"
	"github.com/tinymins/sempre/internal/webconfig"
)

func writeBundle(ctx context.Context, dist string, item target, installedAt time.Time) error {
	workDir := filepath.Join(dist, ".bundle-work", item.directoryName())
	if err := os.RemoveAll(workDir); err != nil {
		return err
	}
	if err := os.MkdirAll(workDir, 0o755); err != nil {
		return err
	}
	if err := copyFile(filepath.Join(dist, item.name), filepath.Join(workDir, item.executableName()), 0o755); err != nil {
		return err
	}
	if err := copyDirectory(filepath.Join(dist, "resources"), filepath.Join(workDir, "resources"), 0o600); err != nil {
		return err
	}
	if err := writeReleaseSnapshot(ctx, workDir, item, installedAt); err != nil {
		_ = os.RemoveAll(workDir)
		return err
	}
	if err := writeBundleInstallers(workDir, item.executableName(), item.os); err != nil {
		_ = os.RemoveAll(workDir)
		return err
	}
	if err := zipDirectoryWithPrefix(filepath.Join(dist, item.bundleName()), workDir, item.directoryName()); err != nil {
		_ = os.RemoveAll(workDir)
		_ = os.Remove(filepath.Join(dist, item.bundleName()))
		return err
	}
	return cleanupBundleWork(workDir)
}

func cleanupBundleWork(workDir string) error {
	if err := os.RemoveAll(workDir); err != nil {
		return err
	}
	if err := os.Remove(filepath.Dir(workDir)); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	return nil
}

func writeReleaseSnapshot(ctx context.Context, packageDir string, item target, installedAt time.Time) error {
	paths := layout.PortableAt(filepath.Join(packageDir, item.executableName()))
	if err := paths.Ensure(); err != nil {
		return err
	}
	if err := state.WriteAtomic(layout.PortableMarkerPath(paths.ServiceExecutable), []byte{}, 0o600); err != nil {
		return err
	}
	if err := writeReleaseWebConfig(paths.WebConfig); err != nil {
		return err
	}
	if err := subscriptions.NewStore(paths).Initialize(""); err != nil {
		return err
	}
	if err := tunnel.NewStore(paths).Initialize(); err != nil {
		return err
	}
	if _, _, err := tunnel.InstallPackage(ctx, paths, item.os, item.arch); err != nil {
		return fmt.Errorf("install wstunnel %s for %s/%s: %w", tunnel.Version, item.os, item.arch, err)
	}
	installations := []releaseCoreInstallation{}
	for _, request := range releaseCoreRequests() {
		resolved, err := request.Adapter.Resolve(ctx, "", request.Reference, releaseCoreTarget(item))
		if err != nil {
			return fmt.Errorf("resolve %s@%s for %s/%s: %w", request.Adapter.ID(), request.Reference, item.os, item.arch, err)
		}
		if err := installReleaseCore(ctx, paths, item, request.Adapter, resolved); err != nil {
			return fmt.Errorf("install %s %s for %s/%s: %w", request.Adapter.ID(), resolved.Version, item.os, item.arch, err)
		}
		installations = append(installations, releaseCoreInstallation{Core: request.Adapter.ID(), Channel: request.Channel, Package: resolved})
	}
	document, err := buildReleaseState(installedAt, installations)
	if err != nil {
		return err
	}
	data, err := json.MarshalIndent(document, "", "  ")
	if err != nil {
		return err
	}
	return state.WriteAtomic(paths.State, append(data, '\n'), 0o600)
}

type releaseCoreInstallation struct {
	Core    string
	Channel string
	Package core.Package
}

func buildReleaseState(installedAt time.Time, installations []releaseCoreInstallation) (state.Document, error) {
	document := state.NewDocument()
	document.Selected = &state.Selection{Core: "sing-box", Ref: core.Stable}
	for _, installation := range installations {
		source := document.Core(installation.Core).Source("")
		if installation.Channel != "" {
			source.Channels[installation.Channel] = installation.Package.Version
		}
		source.Installed[installation.Package.Version] = &state.Installation{
			Digest:      installation.Package.Digest,
			Source:      installation.Package.URL,
			InstalledAt: installedAt,
		}
	}
	document.Normalize()
	if err := document.Validate(); err != nil {
		return state.Document{}, err
	}
	return document, nil
}

func releaseCoreTarget(item target) core.Target {
	return core.Target{OS: item.os, Arch: item.arch}
}

func installReleaseCore(ctx context.Context, paths layout.Layout, item target, adapter core.Adapter, resolved core.Package) error {
	temporary, err := os.MkdirTemp(paths.Runtime, "release-core-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(temporary)
	archivePath := filepath.Join(temporary, resolved.Name)
	if err := download.Verified(ctx, download.Artifact{
		Name:   resolved.Name,
		URL:    resolved.URL,
		Digest: resolved.Digest,
		Size:   resolved.Size,
	}, archivePath); err != nil {
		return err
	}
	target := releaseCoreTarget(item)
	extracted := filepath.Join(temporary, "extract")
	if err := archive.Extract(archivePath, extracted, archive.ExtractOptions{
		Format:         resolved.Format,
		SingleFileName: adapter.ExecutableName(target),
	}); err != nil {
		return err
	}
	executableName := adapter.ExecutableName(target)
	sourceBinary, err := findReleaseBinary(extracted, executableName, item)
	if err != nil {
		return err
	}
	destination := releaseCoreVersionDir(paths, adapter.ID(), resolved.Version)
	if err := os.RemoveAll(destination); err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(destination), 0o700); err != nil {
		return err
	}
	if err := copyDirectory(filepath.Dir(sourceBinary), destination, 0o700); err != nil {
		return err
	}
	copiedBinary := filepath.Join(destination, filepath.Base(sourceBinary))
	finalBinary := filepath.Join(destination, executableName)
	if copiedBinary != finalBinary {
		if err := os.Rename(copiedBinary, finalBinary); err != nil {
			return err
		}
	}
	return nil
}

func releaseCoreVersionDir(paths layout.Layout, coreID, version string) string {
	return filepath.Join(paths.Cores, coreID, version)
}

func findReleaseBinary(root, executableName string, item target) (string, error) {
	if path, err := archive.Find(root, executableName); err == nil {
		return path, nil
	}
	candidates := []string{}
	err := filepath.WalkDir(root, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() {
			return nil
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		if !info.Mode().IsRegular() {
			return nil
		}
		if item.os == "windows" && !strings.EqualFold(filepath.Ext(entry.Name()), ".exe") {
			return nil
		}
		candidates = append(candidates, path)
		return nil
	})
	if err != nil {
		return "", err
	}
	if len(candidates) != 1 {
		return "", fmt.Errorf("archive does not contain %s", executableName)
	}
	return candidates[0], nil
}

func writeReleaseWebConfig(path string) error {
	config := webconfig.Config{Schema: webconfig.SchemaVersion, Listen: webconfig.DefaultListen}
	if err := config.Validate(); err != nil {
		return err
	}
	data, err := json.MarshalIndent(config, "", "  ")
	if err != nil {
		return err
	}
	return state.WriteAtomic(path, append(data, '\n'), 0o600)
}
