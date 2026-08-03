//go:build !windows

package app

import (
	"fmt"
	"os"
)

func removeInstallationRoot(path string) error {
	if err := os.RemoveAll(path); err != nil {
		return fmt.Errorf("remove installation directory %s: %w", path, err)
	}
	return nil
}
