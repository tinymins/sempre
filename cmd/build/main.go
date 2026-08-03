package main

import (
	"archive/zip"
	"bufio"
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
)

type target struct {
	os   string
	arch string
	name string
}

func (item target) bundleName() string {
	return fmt.Sprintf("sempre-bundle-%s-%s.zip", item.os, item.arch)
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
		if err := writeBundle(dist, item); err != nil {
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

func writeBundle(dist string, item target) error {
	archive, err := os.Create(filepath.Join(dist, item.bundleName()))
	if err != nil {
		return err
	}
	writer := zip.NewWriter(archive)
	closeWithError := func(cause error) error {
		return errors.Join(cause, writer.Close(), archive.Close())
	}
	prefix := fmt.Sprintf("sempre-%s-%s", item.os, item.arch)
	executable := "sempre"
	if item.os == "windows" {
		executable += ".exe"
	}
	if err := addFileToZIP(writer, filepath.Join(dist, item.name), filepath.ToSlash(filepath.Join(prefix, executable)), 0o755); err != nil {
		return closeWithError(err)
	}
	uiArchive := filepath.Join(dist, "resources", "sempre-ui.zip")
	if err := addFileToZIP(writer, uiArchive, filepath.ToSlash(filepath.Join(prefix, "resources", "sempre-ui.zip")), 0o600); err != nil {
		return closeWithError(err)
	}
	checksums := filepath.Join(dist, "resources", "SHA256SUMS")
	if err := addFileToZIP(writer, checksums, filepath.ToSlash(filepath.Join(prefix, "resources", "SHA256SUMS")), 0o600); err != nil {
		return closeWithError(err)
	}
	return closeWithError(nil)
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
