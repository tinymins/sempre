package main

import (
	"archive/zip"
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/archive"
	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/core/mihomo"
	"github.com/tinymins/sempre/internal/core/singbox"
	"github.com/tinymins/sempre/internal/core/v2ray"
	"github.com/tinymins/sempre/internal/core/xray"
	"github.com/tinymins/sempre/internal/download"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"github.com/tinymins/sempre/internal/webconfig"
)

type target struct {
	os   string
	arch string
	name string
}

func (item target) bundleName() string {
	return fmt.Sprintf("sempre-bundle-%s-%s.zip", item.os, item.arch)
}

func (item target) directoryName() string {
	return fmt.Sprintf("sempre-%s-%s", item.os, item.arch)
}

func (item target) executableName() string {
	if item.os == "windows" {
		return "sempre.exe"
	}
	return "sempre"
}

func main() {
	if err := build(); err != nil {
		fmt.Fprintln(os.Stderr, "build failed:", err)
		os.Exit(1)
	}
}

func build() error {
	_, source, _, ok := runtime.Caller(0)
	if !ok {
		return fmt.Errorf("locate build command")
	}
	root := filepath.Clean(filepath.Join(filepath.Dir(source), "..", ".."))
	goBinary := filepath.Join(runtime.GOROOT(), "bin", "go")
	if runtime.GOOS == "windows" {
		goBinary += ".exe"
	}
	if err := checkFormatting(root); err != nil {
		return err
	}
	if err := run(root, nil, goBinary, "test", "./..."); err != nil {
		return err
	}
	if err := run(root, nil, goBinary, "vet", "./..."); err != nil {
		return err
	}
	bunBinary, err := exec.LookPath(executableName("bun"))
	if err != nil {
		return fmt.Errorf("locate Bun: %w", err)
	}
	if err := run(root, nil, bunBinary, "--cwd=ui", "run", "lint"); err != nil {
		return err
	}
	if err := run(root, nil, bunBinary, "--cwd=ui", "run", "test"); err != nil {
		return err
	}

	resources := []struct {
		arch string
		path string
	}{
		{"amd64", filepath.Join(root, "cmd", "sempre", "rsrc_windows_amd64.syso")},
		{"arm64", filepath.Join(root, "cmd", "sempre", "rsrc_windows_arm64.syso")},
	}
	for _, resource := range resources {
		defer os.Remove(resource.path)
		if err := run(
			root,
			nil,
			goBinary,
			"run",
			"github.com/tc-hib/go-winres@v0.3.3",
			"make",
			"--in",
			"winres/winres.json",
			"--out",
			"cmd/sempre/rsrc",
			"--arch",
			resource.arch,
		); err != nil {
			return err
		}
	}

	dist := filepath.Join(root, "dist")
	if err := os.RemoveAll(dist); err != nil {
		return err
	}
	if err := os.MkdirAll(dist, 0o755); err != nil {
		return err
	}
	version := gitOutput(root, "describe", "--tags", "--always", "--dirty")
	if version == "unknown" {
		version = "dev"
	}
	commit := gitOutput(root, "rev-parse", "--short=12", "HEAD")
	date := gitOutput(root, "show", "-s", "--format=%cI", "HEAD")
	if err := buildUI(root, dist, bunBinary, version); err != nil {
		return err
	}
	if err := writeDistributionResources(dist); err != nil {
		return err
	}
	ldflags := strings.Join([]string{
		"-s", "-w",
		"-X", "github.com/tinymins/sempre/internal/buildinfo.Version=" + version,
		"-X", "github.com/tinymins/sempre/internal/buildinfo.Commit=" + commit,
		"-X", "github.com/tinymins/sempre/internal/buildinfo.Date=" + date,
	}, " ")
	targets := []target{
		{"windows", "amd64", "sempre-windows-amd64.exe"},
		{"windows", "arm64", "sempre-windows-arm64.exe"},
		{"linux", "amd64", "sempre-linux-amd64"},
		{"linux", "arm64", "sempre-linux-arm64"},
		{"darwin", "amd64", "sempre-darwin-amd64"},
		{"darwin", "arm64", "sempre-darwin-arm64"},
	}
	installedAt := parseBuildDate(date)
	for _, item := range targets {
		output := filepath.Join(dist, item.name)
		environment := map[string]string{
			"CGO_ENABLED": "0",
			"GOOS":        item.os,
			"GOARCH":      item.arch,
		}
		fmt.Printf("Building %s/%s -> %s\n", item.os, item.arch, output)
		if err := run(root, environment, goBinary, "build", "-trimpath", "-ldflags", ldflags, "-o", output, "./cmd/sempre"); err != nil {
			return err
		}
		if err := writeBundle(context.Background(), dist, item, installedAt); err != nil {
			return err
		}
	}
	names := []string{"sempre-ui.zip"}
	for _, item := range targets {
		names = append(names, item.name, item.bundleName())
	}
	return writeChecksums(dist, names)
}

func buildUI(root, dist, bunBinary, version string) error {
	if err := run(root, nil, bunBinary, "--cwd=ui", "run", "build"); err != nil {
		return err
	}
	uiDist := filepath.Join(root, "ui", "dist")
	manifestPath := filepath.Join(uiDist, "sempre-ui.json")
	data, err := os.ReadFile(manifestPath)
	if err != nil {
		return fmt.Errorf("read UI manifest: %w", err)
	}
	var manifest map[string]any
	if err := json.Unmarshal(data, &manifest); err != nil {
		return fmt.Errorf("decode UI manifest: %w", err)
	}
	manifest["version"] = version
	data, err = json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		return err
	}
	if err := os.WriteFile(manifestPath, append(data, '\n'), 0o644); err != nil {
		return fmt.Errorf("write UI manifest: %w", err)
	}
	if err := zipDirectory(filepath.Join(dist, "sempre-ui.zip"), uiDist); err != nil {
		return fmt.Errorf("archive UI: %w", err)
	}
	return nil
}

func writeDistributionResources(dist string) error {
	directory := filepath.Join(dist, "resources")
	if err := os.MkdirAll(directory, 0o755); err != nil {
		return err
	}
	data, err := os.ReadFile(filepath.Join(dist, "sempre-ui.zip"))
	if err != nil {
		return err
	}
	archive := filepath.Join(directory, "sempre-ui.zip")
	if err := os.WriteFile(archive, data, 0o644); err != nil {
		return err
	}
	digest, err := hashFile(archive)
	if err != nil {
		return err
	}
	checksums := []byte(fmt.Sprintf("%s  sempre-ui.zip\n", digest))
	return os.WriteFile(filepath.Join(directory, "SHA256SUMS"), checksums, 0o644)
}

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
	installations := []releaseCoreInstallation{}
	for _, adapter := range releaseCoreAdapters() {
		resolved, err := adapter.Resolve(ctx, "", core.Stable, releaseCoreTarget(item))
		if err != nil {
			return fmt.Errorf("resolve %s for %s/%s: %w", adapter.ID(), item.os, item.arch, err)
		}
		if err := installReleaseCore(ctx, paths, item, adapter, resolved); err != nil {
			return fmt.Errorf("install %s %s for %s/%s: %w", adapter.ID(), resolved.Version, item.os, item.arch, err)
		}
		installations = append(installations, releaseCoreInstallation{Core: adapter.ID(), Package: resolved})
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
	Package core.Package
}

func buildReleaseState(installedAt time.Time, installations []releaseCoreInstallation) (state.Document, error) {
	document := state.NewDocument()
	document.Selected = &state.Selection{Core: "sing-box", Ref: core.Stable}
	for _, installation := range installations {
		source := document.Core(installation.Core).Source("")
		source.Channels[core.Stable] = installation.Package.Version
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

func releaseCoreAdapters() []core.Adapter {
	return []core.Adapter{singbox.New(), mihomo.New(), xray.New(), v2ray.New()}
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

func writeBundleInstallers(packageDir, executableName, goos string) error {
	unix := fmt.Sprintf("#!/bin/sh\nset -eu\ncd -- \"$(dirname -- \"$0\")\"\n./%s bundle install --yes\n", executableName)
	switch goos {
	case "windows":
		windows := fmt.Sprintf("@echo off\r\ncd /d \"%%~dp0\"\r\n\"%%~dp0%s\" bundle install --yes\r\nset EXITCODE=%%ERRORLEVEL%%\r\npause\r\nexit /b %%EXITCODE%%\r\n", executableName)
		return state.WriteAtomic(filepath.Join(packageDir, "install.cmd"), []byte(windows), 0o755)
	case "darwin":
		if err := state.WriteAtomic(filepath.Join(packageDir, "install.command"), []byte(unix), 0o755); err != nil {
			return err
		}
		return state.WriteAtomic(filepath.Join(packageDir, "install.sh"), []byte(unix), 0o755)
	default:
		if err := state.WriteAtomic(filepath.Join(packageDir, "install.sh"), []byte(unix), 0o755); err != nil {
			return err
		}
		desktop := "[Desktop Entry]\nType=Application\nName=Install Sempre Bundle\nTerminal=true\nExec=sh -c 'cd \"$(dirname \"$1\")\" && sh install.sh' sh %k\n"
		return state.WriteAtomic(filepath.Join(packageDir, "install.desktop"), []byte(desktop), 0o755)
	}
}

func zipDirectory(destination, source string) error {
	archive, err := os.Create(destination)
	if err != nil {
		return err
	}
	writer := zip.NewWriter(archive)
	err = filepath.WalkDir(source, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() {
			return nil
		}
		relative, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		return addFileToZIP(writer, path, filepath.ToSlash(relative), 0o600)
	})
	return errors.Join(err, writer.Close(), archive.Close())
}

func zipDirectoryWithPrefix(destination, source, prefix string) error {
	archive, err := os.Create(destination)
	if err != nil {
		return err
	}
	writer := zip.NewWriter(archive)
	closeWithError := func(cause error) error {
		return errors.Join(cause, writer.Close(), archive.Close())
	}
	err = filepath.WalkDir(source, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if path == source {
			return nil
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return fmt.Errorf("refuse symlink while archiving %s", path)
		}
		relative, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		name := filepath.ToSlash(filepath.Join(prefix, relative))
		if entry.IsDir() {
			header := &zip.FileHeader{Name: name + "/", Method: zip.Store}
			header.SetMode(0o700 | os.ModeDir)
			_, err := writer.CreateHeader(header)
			return err
		}
		return addFileToZIP(writer, path, name, info.Mode())
	})
	if err != nil {
		return closeWithError(err)
	}
	return closeWithError(nil)
}

func addFileToZIP(writer *zip.Writer, source, name string, mode os.FileMode) error {
	file, err := os.Open(source)
	if err != nil {
		return err
	}
	defer file.Close()
	header := &zip.FileHeader{Name: name, Method: zip.Deflate}
	header.SetMode(mode)
	destination, err := writer.CreateHeader(header)
	if err != nil {
		return err
	}
	_, err = io.Copy(destination, file)
	return err
}

func copyDirectory(source, target string, mode os.FileMode) error {
	return filepath.WalkDir(source, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		relative, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		if relative == "." {
			return os.MkdirAll(target, 0o755)
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return fmt.Errorf("refuse symlink while copying %s", path)
		}
		destination := filepath.Join(target, relative)
		if entry.IsDir() {
			return os.MkdirAll(destination, 0o755)
		}
		return copyFile(path, destination, mode)
	})
}

func copyFile(source, target string, mode os.FileMode) error {
	sourceFile, err := os.Open(source)
	if err != nil {
		return err
	}
	defer sourceFile.Close()
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		return err
	}
	targetFile, err := os.OpenFile(target, os.O_CREATE|os.O_EXCL|os.O_WRONLY, mode)
	if err != nil {
		return err
	}
	_, copyErr := io.Copy(targetFile, sourceFile)
	closeErr := targetFile.Close()
	return errors.Join(copyErr, closeErr)
}

func parseBuildDate(value string) time.Time {
	parsed, err := time.Parse(time.RFC3339, value)
	if err != nil {
		return time.Now().UTC()
	}
	return parsed.UTC()
}

func checkFormatting(root string) error {
	var files []string
	err := filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() && (entry.Name() == ".git" || entry.Name() == "dist" || entry.Name() == "node_modules") {
			return filepath.SkipDir
		}
		if !entry.IsDir() && filepath.Ext(path) == ".go" {
			files = append(files, path)
		}
		return nil
	})
	if err != nil {
		return err
	}
	arguments := append([]string{"-l"}, files...)
	command := exec.Command(filepath.Join(runtime.GOROOT(), "bin", executableName("gofmt")), arguments...)
	command.Dir = root
	output, err := command.Output()
	if err != nil {
		return fmt.Errorf("check formatting: %w", err)
	}
	if strings.TrimSpace(string(output)) != "" {
		return fmt.Errorf("files require gofmt:\n%s", strings.TrimSpace(string(output)))
	}
	return nil
}

func executableName(name string) string {
	if runtime.GOOS == "windows" {
		return name + ".exe"
	}
	return name
}

func run(directory string, values map[string]string, name string, arguments ...string) error {
	command := exec.Command(name, arguments...)
	command.Dir = directory
	command.Env = withEnvironment(os.Environ(), values)
	command.Stdout = os.Stdout
	command.Stderr = os.Stderr
	if err := command.Run(); err != nil {
		return fmt.Errorf("%s %s: %w", name, strings.Join(arguments, " "), err)
	}
	return nil
}

func withEnvironment(current []string, values map[string]string) []string {
	result := make([]string, 0, len(current)+len(values))
	for _, entry := range current {
		key, _, _ := strings.Cut(entry, "=")
		if _, replaced := values[key]; !replaced {
			result = append(result, entry)
		}
	}
	for key, value := range values {
		result = append(result, key+"="+value)
	}
	return result
}

func gitOutput(root string, arguments ...string) string {
	command := exec.Command("git", arguments...)
	command.Dir = root
	output, err := command.Output()
	if err != nil {
		return "unknown"
	}
	return strings.TrimSpace(string(output))
}

func writeChecksums(dist string, names []string) error {
	sort.Strings(names)
	file, err := os.Create(filepath.Join(dist, "SHA256SUMS"))
	if err != nil {
		return err
	}
	writer := bufio.NewWriter(file)
	for _, name := range names {
		digest, err := hashFile(filepath.Join(dist, name))
		if err != nil {
			file.Close()
			return err
		}
		if _, err := fmt.Fprintf(writer, "%s  %s\n", digest, name); err != nil {
			file.Close()
			return err
		}
	}
	if err := writer.Flush(); err != nil {
		file.Close()
		return err
	}
	return file.Close()
}

func hashFile(path string) (string, error) {
	file, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer file.Close()
	hash := sha256.New()
	if _, err := io.Copy(hash, file); err != nil {
		return "", err
	}
	return hex.EncodeToString(hash.Sum(nil)), nil
}
