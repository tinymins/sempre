package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
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
