package main

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
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
	}
	return writeChecksums(dist, targets)
}

func checkFormatting(root string) error {
	var files []string
	err := filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() && (entry.Name() == ".git" || entry.Name() == "dist") {
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

func writeChecksums(dist string, targets []target) error {
	names := make([]string, 0, len(targets))
	for _, item := range targets {
		names = append(names, item.name)
	}
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
