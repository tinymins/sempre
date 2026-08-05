package app

import (
	"path/filepath"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/layout"
)

func coreBinaryPath(paths layout.Layout, adapter core.Adapter, repository, version string) string {
	return filepath.Join(
		paths.CoreVersionDir(adapter.ID(), repository, version),
		adapter.ExecutableName(core.CurrentTarget()),
	)
}

func (manager *Manager) coreBinary(coreID, repository, version string) (string, error) {
	adapter, err := manager.registry.Get(coreID)
	if err != nil {
		return "", err
	}
	return coreBinaryPath(manager.paths, adapter, repository, version), nil
}
