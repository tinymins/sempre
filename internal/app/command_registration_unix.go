//go:build !windows

package app

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/tinymins/sempre/internal/layout"
)

func registerCommand(paths layout.Layout) (func() error, error) {
	if err := os.MkdirAll(filepath.Dir(paths.CommandExecutable), 0o755); err != nil {
		return nil, fmt.Errorf("create command directory: %w", err)
	}
	owned, err := commandRegistrationOwned(paths)
	if err == nil && owned {
		return func() error { return nil }, nil
	}
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return nil, err
	}
	if err == nil {
		return nil, fmt.Errorf("command path %s already exists and is not owned by Sempre", paths.CommandExecutable)
	}
	if err := os.Symlink(paths.ServiceExecutable, paths.CommandExecutable); err != nil {
		return nil, fmt.Errorf("register command %s: %w", paths.CommandExecutable, err)
	}
	return func() error { return removeOwnedCommand(paths) }, nil
}

func unregisterCommand(paths layout.Layout) error {
	owned, err := commandRegistrationOwned(paths)
	if errors.Is(err, os.ErrNotExist) || (err == nil && !owned) {
		return nil
	}
	if err != nil {
		return err
	}
	return removeOwnedCommand(paths)
}

func checkCommandRegistration(paths layout.Layout) error {
	owned, err := commandRegistrationOwned(paths)
	if errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("not registered at %s", paths.CommandExecutable)
	}
	if err != nil {
		return err
	}
	if !owned {
		return fmt.Errorf("%s is not owned by Sempre", paths.CommandExecutable)
	}
	return nil
}

func commandRegistrationOwned(paths layout.Layout) (bool, error) {
	info, err := os.Lstat(paths.CommandExecutable)
	if err != nil {
		return false, err
	}
	if info.Mode()&os.ModeSymlink == 0 {
		return false, nil
	}
	target, err := os.Readlink(paths.CommandExecutable)
	if err != nil {
		return false, fmt.Errorf("inspect command link %s: %w", paths.CommandExecutable, err)
	}
	if !filepath.IsAbs(target) {
		target = filepath.Join(filepath.Dir(paths.CommandExecutable), target)
	}
	return filepath.Clean(target) == filepath.Clean(paths.ServiceExecutable), nil
}

func removeOwnedCommand(paths layout.Layout) error {
	owned, err := commandRegistrationOwned(paths)
	if errors.Is(err, os.ErrNotExist) || (err == nil && !owned) {
		return nil
	}
	if err != nil {
		return err
	}
	if err := os.Remove(paths.CommandExecutable); err != nil && !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("remove command registration %s: %w", paths.CommandExecutable, err)
	}
	return nil
}
