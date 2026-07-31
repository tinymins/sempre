//go:build !windows

package elevate

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

func Ensure(arguments []string, required bool) (bool, int, error) {
	if !required || os.Geteuid() == 0 {
		return false, 0, nil
	}
	for _, argument := range arguments {
		if argument == "--elevated" {
			return false, 0, fmt.Errorf("sudo did not grant administrator access")
		}
	}
	executable, err := os.Executable()
	if err != nil {
		return false, 0, err
	}
	executable, err = filepath.EvalSymlinks(executable)
	if err != nil {
		return false, 0, err
	}
	elevatedArguments := append([]string{"--", executable, "--elevated"}, arguments...)
	command := exec.Command("sudo", elevatedArguments...)
	command.Stdin = os.Stdin
	command.Stdout = os.Stdout
	command.Stderr = os.Stderr
	err = command.Run()
	if err == nil {
		return true, 0, nil
	}
	var exitError *exec.ExitError
	if errors.As(err, &exitError) {
		return true, exitError.ExitCode(), nil
	}
	return false, 0, fmt.Errorf("request administrator access with sudo: %w", err)
}
