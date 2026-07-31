//go:build !windows

package service

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

func requireRoot() error {
	if os.Geteuid() != 0 {
		return fmt.Errorf("administrator access is required; rerun this command with sudo")
	}
	return nil
}

func runCommand(ctx context.Context, name string, arguments ...string) error {
	command := exec.CommandContext(ctx, name, arguments...)
	output, err := command.CombinedOutput()
	if err != nil {
		return fmt.Errorf("%s %s: %w: %s", name, strings.Join(arguments, " "), err, strings.TrimSpace(string(output)))
	}
	return nil
}
