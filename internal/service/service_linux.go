//go:build linux

package service

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"

	"github.com/sempre-lab/sempre/internal/state"
)

const systemdUnit = "/etc/systemd/system/sempre.service"

type linuxController struct{}

func New() Controller {
	return linuxController{}
}

func (linuxController) Install(ctx context.Context, executable string) error {
	if err := requireRoot(); err != nil {
		return err
	}
	executable, err := filepath.Abs(executable)
	if err != nil {
		return err
	}
	unit := fmt.Sprintf(`[Unit]
Description=%s
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=%s
ExecStart=%s daemon
Restart=on-failure
RestartSec=5
TimeoutStopSec=20

[Install]
WantedBy=multi-user.target
`, Description, strconv.Quote(filepath.Dir(executable)), strconv.Quote(executable))
	if err := state.WriteAtomic(systemdUnit, []byte(unit), 0o644); err != nil {
		return err
	}
	if err := runCommand(ctx, "systemctl", "daemon-reload"); err != nil {
		return err
	}
	return runCommand(ctx, "systemctl", "enable", Name+".service")
}

func (controller linuxController) Uninstall(ctx context.Context) error {
	if err := requireRoot(); err != nil {
		return err
	}
	_ = controller.Stop(ctx)
	_ = runCommand(ctx, "systemctl", "disable", Name+".service")
	if err := os.Remove(systemdUnit); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	return runCommand(ctx, "systemctl", "daemon-reload")
}

func (linuxController) Start(ctx context.Context) error {
	if err := requireRoot(); err != nil {
		return err
	}
	return runCommand(ctx, "systemctl", "start", Name+".service")
}

func (linuxController) Stop(ctx context.Context) error {
	if err := requireRoot(); err != nil {
		return err
	}
	state, err := (linuxController{}).Status(ctx)
	if err != nil {
		return err
	}
	if state == NotInstalled || state == Stopped {
		return nil
	}
	return runCommand(ctx, "systemctl", "stop", Name+".service")
}

func (controller linuxController) Restart(ctx context.Context) error {
	if err := requireRoot(); err != nil {
		return err
	}
	return runCommand(ctx, "systemctl", "restart", Name+".service")
}

func (linuxController) Status(ctx context.Context) (State, error) {
	loadOutput, loadErr := exec.CommandContext(ctx, "systemctl", "show", "-p", "LoadState", "--value", Name+".service").CombinedOutput()
	load := strings.TrimSpace(string(loadOutput))
	if loadErr != nil || load == "not-found" || load == "" {
		return NotInstalled, nil
	}
	output, err := exec.CommandContext(ctx, "systemctl", "is-active", Name+".service").CombinedOutput()
	value := strings.TrimSpace(string(output))
	switch value {
	case "active":
		return Running, nil
	case "activating":
		return StartPending, nil
	case "deactivating":
		return StopPending, nil
	case "inactive", "failed":
		return Stopped, nil
	default:
		if err != nil {
			return Unknown, fmt.Errorf("query systemd service: %s", value)
		}
		return Unknown, nil
	}
}

func (linuxController) Run(ctx context.Context, daemon func(context.Context) error) error {
	runCtx, cancel := signal.NotifyContext(ctx, os.Interrupt, syscall.SIGTERM)
	defer cancel()
	return daemon(runCtx)
}
