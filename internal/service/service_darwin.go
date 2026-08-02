//go:build darwin

package service

import (
	"context"
	"encoding/xml"
	"errors"
	"fmt"
	"html"
	"io"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"github.com/tinymins/sempre/internal/state"
)

const (
	launchdLabel             = "io.github.tinymins.sempre"
	launchdPlist             = "/Library/LaunchDaemons/io.github.tinymins.sempre.plist"
	launchdBootstrapTimeout  = 10 * time.Second
	launchdBootstrapInterval = 100 * time.Millisecond
)

type darwinController struct{}

func New() Controller {
	return darwinController{}
}

func (darwinController) Install(ctx context.Context, executable, workingDirectory string) error {
	if err := requireRoot(); err != nil {
		return err
	}
	executable, err := filepath.Abs(executable)
	if err != nil {
		return err
	}
	plist, err := renderLaunchdPlist(executable, workingDirectory)
	if err != nil {
		return err
	}
	_, _ = exec.CommandContext(ctx, "launchctl", "bootout", "system/"+launchdLabel).CombinedOutput()
	if err := state.WriteAtomic(launchdPlist, plist, 0o644); err != nil {
		return err
	}
	if err := bootstrapLaunchd(ctx); err != nil {
		return err
	}
	return runCommand(ctx, "launchctl", "enable", "system/"+launchdLabel)
}

func bootstrapLaunchd(ctx context.Context) error {
	return retryLaunchdBootstrap(ctx, func(bootstrapCtx context.Context) error {
		return runCommand(bootstrapCtx, "launchctl", "bootstrap", "system", launchdPlist)
	})
}

func retryLaunchdBootstrap(ctx context.Context, bootstrap func(context.Context) error) error {
	retryCtx, cancel := context.WithTimeout(ctx, launchdBootstrapTimeout)
	defer cancel()

	for {
		err := bootstrap(retryCtx)
		if err == nil {
			return nil
		}
		// bootout may return before launchd has finished removing the old job.
		// launchctl reports that transition as error 5; other errors are actionable.
		if !strings.Contains(err.Error(), "Bootstrap failed: 5: Input/output error") {
			return err
		}

		timer := time.NewTimer(launchdBootstrapInterval)
		select {
		case <-retryCtx.Done():
			timer.Stop()
			return fmt.Errorf("wait for launchd to unload previous service: %w: %v", retryCtx.Err(), err)
		case <-timer.C:
		}
	}
}

func renderLaunchdPlist(executable, workingDirectory string) ([]byte, error) {
	escape := html.EscapeString
	plist := fmt.Sprintf(`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>%s</string>
  <key>ProgramArguments</key>
  <array><string>%s</string><string>--system</string><string>daemon</string></array>
  <key>WorkingDirectory</key><string>%s</string>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ProcessType</key><string>Background</string>
  <key>ThrottleInterval</key><integer>5</integer>
</dict>
</plist>
`, launchdLabel, escape(executable), escape(workingDirectory))
	decoder := xml.NewDecoder(strings.NewReader(plist))
	for {
		if _, err := decoder.Token(); err != nil {
			if errors.Is(err, io.EOF) {
				break
			}
			return nil, fmt.Errorf("generate launchd plist: %w", err)
		}
	}
	return []byte(plist), nil
}

func (controller darwinController) Uninstall(ctx context.Context) error {
	if err := requireRoot(); err != nil {
		return err
	}
	_, _ = exec.CommandContext(ctx, "launchctl", "bootout", "system/"+launchdLabel).CombinedOutput()
	if err := os.Remove(launchdPlist); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	return nil
}

func (darwinController) Start(ctx context.Context) error {
	if err := requireRoot(); err != nil {
		return err
	}
	current, err := (darwinController{}).Status(ctx)
	if err != nil {
		return err
	}
	if current == NotInstalled {
		return fmt.Errorf("Sempre service is not installed")
	}
	if current == Running {
		return nil
	}
	if err := bootstrapLaunchd(ctx); err != nil {
		return err
	}
	return runCommand(ctx, "launchctl", "enable", "system/"+launchdLabel)
}

func (darwinController) Stop(ctx context.Context) error {
	if err := requireRoot(); err != nil {
		return err
	}
	state, err := (darwinController{}).Status(ctx)
	if err != nil {
		return err
	}
	if state == NotInstalled || state == Stopped {
		return nil
	}
	return runCommand(ctx, "launchctl", "bootout", "system/"+launchdLabel)
}

func (controller darwinController) Restart(ctx context.Context) error {
	if err := requireRoot(); err != nil {
		return err
	}
	_, _ = exec.CommandContext(ctx, "launchctl", "bootout", "system/"+launchdLabel).CombinedOutput()
	if err := bootstrapLaunchd(ctx); err != nil {
		return err
	}
	return runCommand(ctx, "launchctl", "enable", "system/"+launchdLabel)
}

func (darwinController) Status(ctx context.Context) (State, error) {
	output, err := exec.CommandContext(ctx, "launchctl", "print", "system/"+launchdLabel).CombinedOutput()
	text := string(output)
	if err != nil {
		if strings.Contains(text, "Could not find service") {
			if _, statErr := os.Stat(launchdPlist); statErr == nil {
				return Stopped, nil
			}
			return NotInstalled, nil
		}
		return Unknown, fmt.Errorf("query launchd service: %s", strings.TrimSpace(text))
	}
	if strings.Contains(text, "state = running") {
		return Running, nil
	}
	return Stopped, nil
}

func (darwinController) Run(ctx context.Context, daemon func(context.Context) error) error {
	runCtx, cancel := signal.NotifyContext(ctx, os.Interrupt, syscall.SIGTERM)
	defer cancel()
	return daemon(runCtx)
}
