package app

import (
	"archive/zip"
	"context"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
)

type BundleExportResult struct {
	Directory string
	Archive   string
}

func (manager *Manager) InstallBundle(ctx context.Context, allowReplace bool) error {
	executable, err := layout.CurrentExecutable()
	if err != nil {
		return err
	}
	portable := layout.PortableAt(executable)
	if portable.State == manager.paths.State {
		return manager.installBundleService(ctx, allowReplace)
	}
	if _, err := os.Stat(portable.State); errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("bundle snapshot is missing: %s", portable.State)
	} else if err != nil {
		return err
	}
	source, err := New(portable, manager.output, manager.errors)
	if err != nil {
		return err
	}
	return source.installBundleService(ctx, allowReplace)
}

func (manager *Manager) ExportBundle(ctx context.Context, outputDir string) (BundleExportResult, error) {
	outputDir = strings.TrimSpace(outputDir)
	if outputDir == "" {
		return BundleExportResult{}, fmt.Errorf("bundle output directory is required")
	}
	outputDir, err := filepath.Abs(outputDir)
	if err != nil {
		return BundleExportResult{}, err
	}
	if err := os.MkdirAll(outputDir, 0o755); err != nil {
		return BundleExportResult{}, err
	}
	name := fmt.Sprintf("sempre-bundle-%s-%s", runtime.GOOS, runtime.GOARCH)
	packageDir := filepath.Join(outputDir, name)
	archivePath := packageDir + ".zip"
	if err := os.RemoveAll(packageDir); err != nil {
		return BundleExportResult{}, err
	}
	if err := os.Remove(archivePath); err != nil && !errors.Is(err, os.ErrNotExist) {
		return BundleExportResult{}, err
	}
	if err := os.MkdirAll(packageDir, 0o755); err != nil {
		return BundleExportResult{}, err
	}
	if err := manager.exportBundleDirectory(ctx, packageDir); err != nil {
		_ = os.RemoveAll(packageDir)
		return BundleExportResult{}, err
	}
	if err := zipDirectory(archivePath, packageDir, filepath.Base(packageDir)); err != nil {
		_ = os.Remove(archivePath)
		return BundleExportResult{}, err
	}
	return BundleExportResult{Directory: packageDir, Archive: archivePath}, nil
}

func (manager *Manager) exportBundleDirectory(ctx context.Context, packageDir string) error {
	paths := layout.At(packageDir)
	if err := paths.Ensure(); err != nil {
		return err
	}
	executable, err := layout.CurrentExecutable()
	if err != nil {
		return err
	}
	document, err := manager.store.Read()
	if err != nil {
		return err
	}
	operations := []*swapOperation{}
	fail := func(err error) error {
		cleanupStaged(operations)
		return err
	}
	executableOperation, err := stageExecutable(executable, paths.ServiceExecutable)
	if err != nil {
		return err
	}
	operations = append(operations, executableOperation)
	resources, err := stageDirectoryFromSources(paths.Resources, 0o600, manager.paths.Resources)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, resources)
	cores, err := manager.stageCores(ctx, paths, document, false)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, cores)
	configs, err := manager.stageConfigs(paths, document)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, configs)
	subscriptions, err := stageDirectoryFromSources(paths.Subscriptions, 0o600, manager.paths.Subscriptions)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, subscriptions)
	stateFile, err := stageStateFile(paths.State, deploymentDocument(document))
	if err != nil {
		return fail(err)
	}
	operations = append(operations, stateFile)
	web, err := manager.stageWebConfig(paths.WebConfig, true)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, web)
	ui, err := manager.stageCurrentUI(paths.UICurrent)
	if err != nil {
		return fail(err)
	}
	operations = append(operations, ui)
	if err := activateSwaps(operations); err != nil {
		return fail(err)
	}
	if err := commitSwaps(operations); err != nil {
		return err
	}
	if err := state.WriteAtomic(layout.PortableMarkerPath(paths.ServiceExecutable), []byte{}, 0o600); err != nil {
		return err
	}
	return writeBundleInstallers(packageDir, paths.ServiceExecutable, runtime.GOOS)
}

func writeBundleInstallers(packageDir, executable, goos string) error {
	executableName := filepath.Base(executable)
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

func zipDirectory(destination, source, prefix string) error {
	archive, err := os.Create(destination)
	if err != nil {
		return err
	}
	writer := zip.NewWriter(archive)
	closeWithError := func(cause error) error {
		return errors.Join(cause, writer.Close(), archive.Close())
	}
	err = filepath.WalkDir(source, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if path == source {
			return nil
		}
		relative, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return fmt.Errorf("refuse symlink while archiving %s", path)
		}
		name := filepath.ToSlash(filepath.Join(prefix, relative))
		if entry.IsDir() {
			header := &zip.FileHeader{Name: name + "/", Method: zip.Store}
			header.SetMode(0o700 | os.ModeDir)
			_, err := writer.CreateHeader(header)
			return err
		}
		header := &zip.FileHeader{Name: name, Method: zip.Deflate}
		header.SetMode(info.Mode())
		target, err := writer.CreateHeader(header)
		if err != nil {
			return err
		}
		sourceFile, err := os.Open(path)
		if err != nil {
			return err
		}
		defer sourceFile.Close()
		_, err = io.Copy(target, sourceFile)
		return err
	})
	if err != nil {
		return closeWithError(err)
	}
	return closeWithError(nil)
}
