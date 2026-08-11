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
	"time"
)

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
