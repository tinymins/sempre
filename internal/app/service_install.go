package app

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/sempre-lab/sempre/internal/layout"
	"github.com/sempre-lab/sempre/internal/service"
	"github.com/sempre-lab/sempre/internal/state"
)

func (manager *Manager) installSystemService(ctx context.Context) error {
	systemPaths, err := layout.ForMode(layout.System)
	if err != nil {
		return err
	}
	systemManager := manager
	if manager.paths.Mode == layout.Portable {
		if _, err := os.Stat(systemPaths.State); errors.Is(err, os.ErrNotExist) {
			if _, _, err := manager.deploymentSpec(ctx, ""); err != nil {
				return fmt.Errorf("portable deployment is not ready: %w", err)
			}
			if err := bootstrapSystemData(manager, systemPaths); err != nil {
				return err
			}
		} else if err != nil {
			return fmt.Errorf("inspect system state: %w", err)
		}
		systemManager, err = New(systemPaths, manager.output, manager.errors)
		if err != nil {
			return err
		}
	}
	if _, _, err := systemManager.deploymentSpec(ctx, ""); err != nil {
		return fmt.Errorf("system deployment is not ready: %w", err)
	}
	current, err := manager.service.Status(ctx)
	if err != nil {
		return err
	}
	if current != service.NotInstalled && current != service.Stopped {
		if err := manager.service.Stop(ctx); err != nil {
			return err
		}
	}
	source, err := layout.CurrentExecutable()
	if err != nil {
		return err
	}
	if err := installExecutable(source, systemPaths); err != nil {
		return err
	}
	if err := manager.service.Install(ctx, systemPaths.ServiceExecutable, systemPaths.Home); err != nil {
		return err
	}
	return manager.service.Start(ctx)
}

func (manager *Manager) systemManager() (*Manager, error) {
	if manager.paths.Mode == layout.System {
		return manager, nil
	}
	paths, err := layout.ForMode(layout.System)
	if err != nil {
		return nil, err
	}
	if _, err := os.Stat(paths.State); errors.Is(err, os.ErrNotExist) {
		return nil, fmt.Errorf("system deployment is not initialized; run 'sempre service install' first")
	} else if err != nil {
		return nil, err
	}
	return New(paths, manager.output, manager.errors)
}

func bootstrapSystemData(source *Manager, target layout.Layout) error {
	if _, err := os.Stat(target.State); err == nil {
		return nil
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if entries, err := os.ReadDir(target.Home); err == nil && len(entries) != 0 {
		return fmt.Errorf("system data directory %s exists without state.json", target.Home)
	} else if err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(target.Home), 0o755); err != nil {
		return err
	}
	staging, err := os.MkdirTemp(filepath.Dir(target.Home), ".sempre-bootstrap-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(staging)
	if err := os.Chmod(staging, 0o700); err != nil {
		return err
	}
	document, err := source.store.Read()
	if err != nil {
		return err
	}
	document.Runtime = state.Runtime{}
	document.Normalize()
	data, err := json.MarshalIndent(document, "", "  ")
	if err != nil {
		return err
	}
	data = append(data, '\n')
	if err := state.WriteAtomic(filepath.Join(staging, "state.json"), data, 0o600); err != nil {
		return err
	}
	if err := copyDirectory(source.paths.Cores, filepath.Join(staging, "cores"), 0o700); err != nil {
		return err
	}
	if err := copyDirectory(source.paths.Configs, filepath.Join(staging, "configs"), 0o600); err != nil {
		return err
	}
	if err := os.Remove(target.Home); err != nil && !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("prepare system data directory: %w", err)
	}
	if err := os.Rename(staging, target.Home); err != nil {
		return fmt.Errorf("activate system data: %w", err)
	}
	if err := target.Ensure(); err != nil {
		return err
	}
	return nil
}

func copyDirectory(source, target string, fileMode os.FileMode) error {
	if err := os.MkdirAll(target, 0o700); err != nil {
		return err
	}
	return filepath.WalkDir(source, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		relative, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		destination := filepath.Join(target, relative)
		if entry.Type()&os.ModeSymlink != 0 {
			return fmt.Errorf("refuse symlink while copying %s", path)
		}
		if entry.IsDir() {
			return os.MkdirAll(destination, 0o700)
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		return state.WriteAtomic(destination, data, fileMode)
	})
}

func installExecutable(source string, target layout.Layout) error {
	if err := target.EnsureServiceExecutableDirectory(); err != nil {
		return err
	}
	if sameFile(source, target.ServiceExecutable) {
		return nil
	}
	data, err := os.ReadFile(source)
	if err != nil {
		return fmt.Errorf("read Sempre executable: %w", err)
	}
	if err := state.WriteAtomic(target.ServiceExecutable, data, 0o755); err != nil {
		return fmt.Errorf("install Sempre executable: %w", err)
	}
	return nil
}

func sameFile(left, right string) bool {
	leftInfo, leftErr := os.Stat(left)
	rightInfo, rightErr := os.Stat(right)
	if leftErr == nil && rightErr == nil {
		return os.SameFile(leftInfo, rightInfo)
	}
	if runtime.GOOS == "windows" {
		return strings.EqualFold(filepath.Clean(left), filepath.Clean(right))
	}
	return filepath.Clean(left) == filepath.Clean(right)
}

func (manager *Manager) checkServiceExecutable() error {
	if manager.paths.Mode != layout.System {
		return nil
	}
	info, err := os.Stat(manager.paths.ServiceExecutable)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("not installed")
		}
		return err
	}
	if info.IsDir() {
		return fmt.Errorf("is a directory")
	}
	if err := checkProtectedPath(manager.paths.ServiceExecutable); err != nil {
		return err
	}
	return nil
}
