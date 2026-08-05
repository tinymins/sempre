package app

import (
	"context"
	"errors"
	"fmt"
	"os"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
)

func (manager *Manager) UninstallApplication(ctx context.Context, purge bool) error {
	target, err := layout.ForMode(layout.System)
	if err != nil {
		return err
	}
	release, err := acquireOperationLocks(target)
	if err != nil {
		return err
	}
	if err := manager.service.Uninstall(ctx); err != nil {
		release()
		return err
	}
	cleanupErr := manager.transparent.Cleanup(ctx)
	commandErr := manager.commands.Unregister(target)
	if !purge {
		store := state.New(target)
		if err := store.Initialize(); err != nil {
			release()
			return errors.Join(err, commandErr, cleanupErr)
		}
		if err := store.Update(func(document *state.Document) error {
			document.Selected = nil
			document.Active = nil
			document.Previous = nil
			document.Pending = false
			document.LastError = ""
			document.Cores = map[string]*state.CoreState{}
			document.Runtime = state.Runtime{}
			return nil
		}); err != nil {
			release()
			return errors.Join(err, commandErr, cleanupErr)
		}
	}
	release()

	result := errors.Join(commandErr, cleanupErr)
	for _, path := range []string{target.Cores, target.UI, target.Logs, target.Runtime} {
		if err := os.RemoveAll(path); err != nil {
			result = errors.Join(result, fmt.Errorf("remove %s: %w", path, err))
		}
	}
	if purge {
		if err := os.RemoveAll(target.Home); err != nil {
			result = errors.Join(result, fmt.Errorf("remove %s: %w", target.Home, err))
		}
	}
	if err := removeInstallationRoot(target.Root); err != nil {
		result = errors.Join(result, err)
	}
	return result
}
