package tunnel

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"sync"

	"github.com/tinymins/sempre/internal/archive"
	"github.com/tinymins/sempre/internal/download"
	"github.com/tinymins/sempre/internal/layout"
)

const Version = "10.5.5"

type Package struct {
	Name   string
	URL    string
	Digest string
	Size   int64
}

var packages = map[string]Package{
	"windows/amd64": releasePackage("windows_amd64", "d77ab72a96247000d9a6da1f0789d7306eb33b5466deafa1348b75bafb03cbce", 4077936),
	// Upstream does not publish a Windows ARM64 asset; Windows on ARM runs the x64 release through its compatibility layer.
	"windows/arm64": releasePackage("windows_amd64", "d77ab72a96247000d9a6da1f0789d7306eb33b5466deafa1348b75bafb03cbce", 4077936),
	"linux/amd64":   releasePackage("linux_amd64", "b20ffa02e945ec0c0d6b153ba69a290593f0957ed2892aee8f987f715ccd95d6", 4983919),
	"linux/arm64":   releasePackage("linux_arm64", "db85183da9732f26c110a08e3fffdfcfc4a44d544035d01eeefa708ed23874bb", 4601463),
	"darwin/amd64":  releasePackage("darwin_amd64", "83515a275775d4f3730315ae86762234f0fc0ec646826c9aaa0106adde0f25b0", 4573839),
	"darwin/arm64":  releasePackage("darwin_arm64", "c905eb5a54a31e0f4639d1676226a7790dcd9d2787364d3332613cdf0a67c36f", 4242096),
}

var installMu sync.Mutex

func releasePackage(target, digest string, size int64) Package {
	name := fmt.Sprintf("wstunnel_%s_%s.tar.gz", Version, target)
	return Package{
		Name:   name,
		URL:    fmt.Sprintf("https://github.com/erebe/wstunnel/releases/download/v%s/%s", Version, name),
		Digest: "sha256:" + digest,
		Size:   size,
	}
}

func PackageFor(goos, goarch string) (Package, error) {
	item, ok := packages[goos+"/"+goarch]
	if !ok {
		return Package{}, fmt.Errorf("wstunnel %s is unavailable for %s/%s", Version, goos, goarch)
	}
	return item, nil
}

func BinaryPath(paths layout.Layout) string {
	return paths.ToolBinary("wstunnel", Version)
}

func Installed(paths layout.Layout) bool {
	info, err := os.Stat(BinaryPath(paths))
	return err == nil && info.Mode().IsRegular()
}

func EnsureBinary(ctx context.Context, paths layout.Layout) (string, bool, error) {
	return InstallPackage(ctx, paths, runtime.GOOS, runtime.GOARCH)
}

func InstallPackage(ctx context.Context, paths layout.Layout, goos, goarch string) (string, bool, error) {
	installMu.Lock()
	defer installMu.Unlock()
	binary := filepath.Join(paths.ToolVersionDir("wstunnel", Version), executableName(goos))
	if info, err := os.Stat(binary); err == nil && info.Mode().IsRegular() {
		return binary, false, nil
	} else if err != nil && !errors.Is(err, os.ErrNotExist) {
		return "", false, err
	}
	item, err := PackageFor(goos, goarch)
	if err != nil {
		return "", false, err
	}
	temporary, err := os.MkdirTemp(paths.Runtime, "wstunnel-install-*")
	if err != nil {
		return "", false, err
	}
	defer os.RemoveAll(temporary)
	archivePath := filepath.Join(temporary, item.Name)
	if err := download.Verified(ctx, download.Artifact{Name: item.Name, URL: item.URL, Digest: item.Digest, Size: item.Size}, archivePath); err != nil {
		return "", false, err
	}
	extracted := filepath.Join(temporary, "extract")
	if err := archive.Extract(archivePath, extracted, archive.ExtractOptions{Format: "tar.gz", SingleFileName: executableName(goos)}); err != nil {
		return "", false, err
	}
	source, err := archive.Find(extracted, executableName(goos))
	if err != nil {
		return "", false, err
	}
	finalDir := paths.ToolVersionDir("wstunnel", Version)
	if err := os.MkdirAll(filepath.Dir(finalDir), 0o700); err != nil {
		return "", false, err
	}
	staging, err := os.MkdirTemp(filepath.Dir(finalDir), ".wstunnel-*")
	if err != nil {
		return "", false, err
	}
	defer os.RemoveAll(staging)
	stagedBinary := filepath.Join(staging, executableName(goos))
	if err := copyFile(source, stagedBinary); err != nil {
		return "", false, err
	}
	if err := os.RemoveAll(finalDir); err != nil {
		return "", false, fmt.Errorf("remove incomplete wstunnel installation: %w", err)
	}
	if err := os.Rename(staging, finalDir); err != nil {
		return "", false, fmt.Errorf("activate wstunnel: %w", err)
	}
	return binary, true, nil
}

func executableName(goos string) string {
	if goos == "windows" {
		return "wstunnel.exe"
	}
	return "wstunnel"
}

func copyFile(source, target string) error {
	data, err := os.ReadFile(source)
	if err != nil {
		return err
	}
	return os.WriteFile(target, data, 0o755)
}
