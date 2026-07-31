package app

import (
	"bufio"
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/sempre-lab/sempre/internal/layout"
	"github.com/sempre-lab/sempre/internal/service"
	"github.com/sempre-lab/sempre/internal/supervisor"
)

func (manager *Manager) RunDirect(ctx context.Context, reference string) error {
	lease, err := manager.store.AcquireInstance()
	if err != nil {
		return err
	}
	defer lease.Release()
	deployment, spec, err := manager.deploymentSpec(ctx, reference)
	if err != nil {
		return err
	}
	fmt.Fprintf(manager.output, "Starting %s. Press Ctrl+C to stop.\n", deploymentLabel(deployment))
	return supervisor.RunForeground(ctx, spec, manager.output, manager.errors)
}

func (manager *Manager) InstallService(ctx context.Context) error {
	return manager.installSystemService(ctx)
}

func (manager *Manager) UninstallService(ctx context.Context) error {
	return manager.service.Uninstall(ctx)
}

func (manager *Manager) StartService(ctx context.Context) error {
	systemManager, err := manager.systemManager()
	if err != nil {
		return err
	}
	if _, _, err := systemManager.deploymentSpec(ctx, ""); err != nil {
		return err
	}
	return manager.service.Start(ctx)
}

func (manager *Manager) StopService(ctx context.Context) error {
	return manager.service.Stop(ctx)
}

func (manager *Manager) RestartService(ctx context.Context) error {
	systemManager, err := manager.systemManager()
	if err != nil {
		return err
	}
	if _, _, err := systemManager.deploymentSpec(ctx, ""); err != nil {
		return err
	}
	return manager.service.Restart(ctx)
}

func (manager *Manager) ServiceState(ctx context.Context) (service.State, error) {
	return manager.service.Status(ctx)
}

func (manager *Manager) Status(ctx context.Context) (string, error) {
	document, err := manager.store.Read()
	if err != nil {
		return "", err
	}
	serviceState, serviceErr := manager.service.Status(ctx)
	var builder strings.Builder
	fmt.Fprintln(&builder, "Mode:", manager.paths.Mode)
	if document.Active == nil {
		fmt.Fprintln(&builder, "Core: not selected")
	} else {
		fmt.Fprintln(&builder, "Core:", deploymentLabel(*document.Active))
	}
	fmt.Fprintln(&builder, "Deployment pending:", document.Pending)
	if document.LastError != "" {
		fmt.Fprintln(&builder, "Last deployment error:", document.LastError)
	}
	if serviceErr != nil {
		fmt.Fprintln(&builder, "System service: unavailable:", serviceErr)
	} else {
		fmt.Fprintln(&builder, "System service:", serviceState)
	}
	runtime := document.Runtime
	if runtime.State == "" {
		fmt.Fprintln(&builder, "Supervisor: no runtime state")
	} else {
		fmt.Fprintf(&builder, "Supervisor: %s, PID %d, restarts %d\n", runtime.State, runtime.PID, runtime.RestartCount)
	}
	if document.Subscription.URL == "" {
		fmt.Fprintln(&builder, "Subscription: not configured")
	} else {
		fmt.Fprintf(&builder, "Subscription: %s, every %s\n", redactedURL(document.Subscription.URL), document.Subscription.Interval)
		if next, ok := nextSubscriptionCheck(document.Subscription); ok {
			fmt.Fprintln(&builder, "Next subscription check:", next.Format(time.RFC3339))
		}
	}
	fmt.Fprintln(&builder, "Data:", manager.paths.Home)
	fmt.Fprintln(&builder, "Stdout log:", manager.paths.StdoutLog)
	fmt.Fprintln(&builder, "Stderr log:", manager.paths.StderrLog)
	return strings.TrimRight(builder.String(), "\n"), nil
}

func (manager *Manager) Doctor(ctx context.Context) (string, error) {
	document, err := manager.store.Read()
	if err != nil {
		return "", err
	}
	var builder strings.Builder
	failures := 0
	check := func(name string, err error) {
		if err != nil {
			failures++
			fmt.Fprintf(&builder, "[FAIL] %s: %v\n", name, err)
		} else {
			fmt.Fprintf(&builder, "[ OK ] %s\n", name)
		}
	}
	check("data directory", writableDirectory(manager.paths.Home))
	if manager.paths.Mode == layout.System {
		check("data protection", checkProtectedPath(manager.paths.Home))
		check("service executable", manager.checkServiceExecutable())
	}
	if document.Active == nil {
		check("active core", fmt.Errorf("not selected"))
	} else {
		binary := manager.paths.CoreBinary(document.Active.Core, document.Active.Version)
		_, binaryErr := os.Stat(binary)
		check("active core binary", binaryErr)
		config := manager.paths.Config(document.Active.Core, document.Active.ConfigHash)
		_, configErr := os.Stat(config)
		check("active configuration", configErr)
		if binaryErr == nil && configErr == nil {
			_, _, validationErr := manager.deploymentSpec(ctx, "")
			check("configuration validation", validationErr)
		}
	}
	_, serviceErr := manager.service.Status(ctx)
	check("service manager", serviceErr)
	if failures == 0 {
		fmt.Fprintln(&builder, "All checks passed.")
	}
	return strings.TrimRight(builder.String(), "\n"), nil
}

func writableDirectory(path string) error {
	file, err := os.CreateTemp(path, ".write-check-*")
	if err != nil {
		return err
	}
	name := file.Name()
	if err := file.Close(); err != nil {
		return err
	}
	return os.Remove(name)
}

func FollowLogs(ctx context.Context, output io.Writer, paths []string, follow bool) error {
	offsets := map[string]int64{}
	for {
		for _, path := range paths {
			offsets[path] = printLogDelta(output, filepath.Base(path), path, offsets[path])
		}
		if !follow {
			return nil
		}
		select {
		case <-ctx.Done():
			return nil
		case <-time.After(250 * time.Millisecond):
		}
	}
}

func printLogDelta(output io.Writer, label, path string, offset int64) int64 {
	file, err := os.Open(path)
	if err != nil {
		return offset
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		return offset
	}
	if info.Size() < offset {
		offset = 0
	}
	if offset == 0 && info.Size() > 64*1024 {
		offset = info.Size() - 64*1024
	}
	if _, err := file.Seek(offset, io.SeekStart); err != nil {
		return offset
	}
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		fmt.Fprintf(output, "[%s] %s\n", label, scanner.Text())
	}
	position, err := file.Seek(0, io.SeekCurrent)
	if err != nil {
		return offset
	}
	return position
}
