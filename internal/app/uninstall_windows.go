//go:build windows

package app

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

func removeInstallationRoot(path string) error {
	executable, err := os.Executable()
	if err != nil {
		return err
	}
	executable, _ = filepath.Abs(executable)
	root, _ := filepath.Abs(path)
	if !strings.EqualFold(filepath.Dir(executable), root) {
		if err := os.RemoveAll(root); err != nil {
			return fmt.Errorf("remove installation directory %s: %w", root, err)
		}
		return nil
	}
	if strings.Contains(root, "\"") || strings.ContainsAny(root, "\r\n") {
		return fmt.Errorf("installation path cannot be scheduled for removal")
	}
	script := fmt.Sprintf("ping 127.0.0.1 -n 3 >NUL & rmdir /S /Q \"%s\"", root)
	command := exec.Command("cmd.exe", "/D", "/S", "/C", script)
	if err := command.Start(); err != nil {
		return fmt.Errorf("schedule installation removal: %w", err)
	}
	return command.Process.Release()
}
