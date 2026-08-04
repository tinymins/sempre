package app

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"github.com/tinymins/sempre/internal/supervisor"
)

var ErrDoctorFailed = errors.New("doctor checks failed")

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

func (manager *Manager) InstallService(ctx context.Context, allowReplace bool) error {
	return manager.withSystemOperation(func() error {
		return manager.installSystemService(ctx, allowReplace)
	})
}

func (manager *Manager) InstallApplication(ctx context.Context, allowReplace bool) error {
	executable, err := layout.CurrentExecutable()
	if err != nil {
		return err
	}
	portable := layout.PortableAt(executable)
	if portable.State != manager.paths.State {
		if _, err := os.Stat(portable.State); err == nil {
			source, err := New(portable, manager.output, manager.errors)
			if err != nil {
				return err
			}
			return source.InstallService(ctx, allowReplace)
		} else if !errors.Is(err, os.ErrNotExist) {
			return err
		}
	}
	return manager.InstallService(ctx, allowReplace)
}

func (manager *Manager) DeployService(
	ctx context.Context,
	component DeployComponent,
	allowReplace bool,
) error {
	return manager.withSystemOperation(func() error {
		return manager.deploySystemService(ctx, component, allowReplace)
	})
}

func (manager *Manager) UninstallService(ctx context.Context) error {
	return manager.withSystemOperation(func() error { return manager.service.Uninstall(ctx) })
}

func (manager *Manager) StartService(ctx context.Context) error {
	return manager.withSystemOperation(func() error { return manager.startService(ctx) })
}

func (manager *Manager) startService(ctx context.Context) error {
	systemManager, err := manager.systemManager()
	if err != nil {
		return err
	}
	_ = systemManager
	return manager.service.Start(ctx)
}

func (manager *Manager) StopService(ctx context.Context) error {
	return manager.withSystemOperation(func() error { return manager.service.Stop(ctx) })
}

func (manager *Manager) RestartService(ctx context.Context) error {
	return manager.withSystemOperation(func() error { return manager.restartService(ctx) })
}

func (manager *Manager) restartService(ctx context.Context) error {
	systemManager, err := manager.systemManager()
	if err != nil {
		return err
	}
	_ = systemManager
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
	if document.Selected == nil {
		fmt.Fprintln(&builder, "Selected: none")
	} else {
		fmt.Fprintf(&builder, "Selected: %s\n", selectionRef(*document.Selected))
	}
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
	fmt.Fprintln(&builder, "Desired core state:", document.DesiredState)
	runtimeStatus, runtimeErr := manager.runtimeStatus(document)
	if runtimeErr != nil {
		fmt.Fprintln(&builder, "Supervisor: unavailable:", runtimeErr)
	} else if runtimeStatus != "" {
		fmt.Fprintln(&builder, "Supervisor:", runtimeStatus)
	} else if runtime.State == "" {
		fmt.Fprintln(&builder, "Supervisor: no runtime state")
	} else {
		fmt.Fprintf(&builder, "Supervisor: %s, PID %d, restarts %d\n", runtime.State, runtime.PID, runtime.RestartCount)
	}
	catalog, catalogErr := manager.subscriptions.Read()
	profile, profileErr := subscriptions.FindProfile(&catalog, document.ActiveProfileID)
	if catalogErr != nil || profileErr != nil || len(profile.Sources) == 0 {
		fmt.Fprintln(&builder, "Subscription: not configured")
	} else {
		fmt.Fprintf(&builder, "Subscription: %s (%d sources), every %s\n", profile.Name, len(profile.Sources), document.Subscription.Interval)
		if next, ok := nextSubscriptionCheck(document.Subscription, subscriptionProfileHasScheduledSources(*profile)); ok {
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
	serviceState, serviceErr := manager.service.Status(ctx)
	check("service manager", serviceErr)
	if manager.paths.Mode == layout.System {
		check("data protection", checkProtectedPath(manager.paths.Home))
		if serviceErr == nil && serviceState == service.NotInstalled {
			fmt.Fprintln(&builder, "[INFO] system service: not installed")
		} else {
			check("service executable", manager.checkServiceExecutable())
			check("command registration", manager.commands.Check(manager.paths))
		}
	}
	if document.Active == nil {
		check("active core", fmt.Errorf("not selected"))
	} else {
		binary := manager.paths.CoreBinary(document.Active.Core, document.Active.Repository, document.Active.Version)
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
	runtimeStatus, runtimeErr := manager.runtimeStatus(document)
	if runtimeErr != nil {
		check("runtime state", runtimeErr)
	} else if strings.HasPrefix(runtimeStatus, "stale") {
		check("runtime state", fmt.Errorf("%s", runtimeStatus))
	} else if runtimeStatus == "" {
		fmt.Fprintln(&builder, "[INFO] runtime state: no managed core is running")
	} else {
		fmt.Fprintf(&builder, "[ OK ] runtime state: %s\n", runtimeStatus)
	}
	if failures == 0 {
		fmt.Fprintln(&builder, "All checks passed.")
	}
	report := strings.TrimRight(builder.String(), "\n")
	if failures > 0 {
		return report, fmt.Errorf("%w: %d check(s) failed", ErrDoctorFailed, failures)
	}
	return report, nil
}

func (manager *Manager) runtimeStatus(document state.Document) (string, error) {
	locked, err := manager.store.InstanceRunning()
	if err != nil {
		return "", err
	}
	runtimeState := document.Runtime
	if runtimeState.PID > 0 {
		if !processAlive(runtimeState.PID) {
			return fmt.Sprintf(
				"stale record: PID %d is not running (recorded state %s)",
				runtimeState.PID,
				runtimeState.State,
			), nil
		}
		if !locked {
			return fmt.Sprintf(
				"stale record: PID %d exists but the Sempre instance lock is free",
				runtimeState.PID,
			), nil
		}
		return fmt.Sprintf(
			"%s, PID %d, restarts %d",
			runtimeState.State,
			runtimeState.PID,
			runtimeState.RestartCount,
		), nil
	}
	if locked {
		switch runtimeState.State {
		case "idle", "stopped", "failed":
			return fmt.Sprintf("%s, no running process", runtimeState.State), nil
		default:
			return "starting or stopping; instance lock held before PID was recorded", nil
		}
	}
	switch runtimeState.State {
	case "running", "starting", "restarting":
		return fmt.Sprintf("stale record: state is %s but no managed process or instance lock exists", runtimeState.State), nil
	case "":
		return "", nil
	default:
		return fmt.Sprintf("%s, no running process", runtimeState.State), nil
	}
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
	cursors := map[string]logCursor{}
	for {
		for _, path := range paths {
			cursor, err := printLogDelta(output, filepath.Base(path), path, cursors[path], !follow)
			if err != nil {
				return err
			}
			cursors[path] = cursor
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

type logCursor struct {
	offset  int64
	info    os.FileInfo
	partial []byte
}

func printLogDelta(output io.Writer, label, path string, cursor logCursor, flushPartial bool) (logCursor, error) {
	file, err := os.Open(path)
	if errors.Is(err, os.ErrNotExist) {
		return cursor, nil
	}
	if err != nil {
		return cursor, fmt.Errorf("open log %s: %w", path, err)
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		return cursor, fmt.Errorf("inspect log %s: %w", path, err)
	}
	if cursor.info != nil && (!os.SameFile(cursor.info, info) || info.Size() < cursor.offset) {
		cursor = logCursor{}
	}
	trimInitialLine := false
	if cursor.info == nil && cursor.offset == 0 && info.Size() > 64*1024 {
		cursor.offset = info.Size() - 64*1024
		trimInitialLine = true
	}
	if _, err := file.Seek(cursor.offset, io.SeekStart); err != nil {
		return cursor, fmt.Errorf("seek log %s: %w", path, err)
	}
	data, err := io.ReadAll(file)
	if err != nil {
		return cursor, fmt.Errorf("read log %s: %w", path, err)
	}
	cursor.offset += int64(len(data))
	cursor.info = info
	data = append(cursor.partial, data...)
	cursor.partial = nil
	if trimInitialLine {
		if newline := bytes.IndexByte(data, '\n'); newline >= 0 {
			data = data[newline+1:]
		} else if !flushPartial {
			cursor.partial = data
			return cursor, nil
		}
	}
	for {
		newline := bytes.IndexByte(data, '\n')
		if newline < 0 {
			break
		}
		line := bytes.TrimSuffix(data[:newline], []byte{'\r'})
		if _, err := fmt.Fprintf(output, "[%s] %s\n", label, line); err != nil {
			return cursor, err
		}
		data = data[newline+1:]
	}
	if len(data) > 0 {
		if flushPartial {
			if _, err := fmt.Fprintf(output, "[%s] %s\n", label, bytes.TrimSuffix(data, []byte{'\r'})); err != nil {
				return cursor, err
			}
		} else {
			cursor.partial = append(cursor.partial, data...)
		}
	}
	return cursor, nil
}
