package app

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
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
	return manager.withSystemOperation(func() error {
		if err := manager.service.Uninstall(ctx); err != nil {
			return err
		}
		return manager.transparent.Cleanup(ctx)
	})
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
	return manager.withSystemOperation(func() error {
		stopErr := manager.service.Stop(ctx)
		cleanupErr := manager.transparent.Cleanup(ctx)
		return errors.Join(stopErr, cleanupErr)
	})
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
	warnings := 0
	check := func(name string, err error) {
		if err != nil {
			failures++
			fmt.Fprintf(&builder, "[FAIL] %s: %v\n", name, err)
		} else {
			fmt.Fprintf(&builder, "[ OK ] %s\n", name)
		}
	}
	warn := func(name string, err error) {
		if err != nil {
			warnings++
			fmt.Fprintf(&builder, "[WARN] %s: %v\n", name, err)
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
	catalog, catalogErr := manager.subscriptions.Read()
	var profile *subscriptions.Profile
	profileErr := catalogErr
	if profileErr == nil {
		profile, profileErr = subscriptions.FindProfile(&catalog, document.ActiveProfileID)
	}
	if document.Active == nil {
		check("active core", fmt.Errorf("not selected"))
	} else {
		binary, binaryErr := manager.coreBinary(document.Active.Core, document.Active.Repository, document.Active.Version)
		if binaryErr == nil {
			_, binaryErr = os.Stat(binary)
		}
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
	if document.Active != nil && profileErr == nil && document.Runtime.State == "running" {
		check("active source configuration hash", verifyConfigurationHash(
			manager.paths.Config(document.Active.Core, document.Active.ConfigHash),
			document.Active.ConfigHash,
		))
		check("runtime source configuration hash", equalValue(
			document.Runtime.ConfigHash,
			document.Active.ConfigHash,
			"runtime source hash does not match the active deployment",
		))
		if document.Runtime.RuntimeConfig == "" || document.Runtime.RuntimeHash == "" {
			check("prepared runtime configuration hash", fmt.Errorf("runtime configuration metadata is missing; restart Sempre once after upgrading"))
		} else {
			check("prepared runtime configuration hash", verifyConfigurationHash(
				document.Runtime.RuntimeConfig,
				document.Runtime.RuntimeHash,
			))
		}
		adapter, adapterErr := manager.registry.Get(document.Active.Core)
		if adapterErr == nil {
			expected, expectedErr := expectedConfigBuild(*profile, adapter, document.Active.Version)
			if expectedErr == nil && document.ConfigBuilds[document.Active.Core] != expected {
				expectedErr = fmt.Errorf("active configuration was not built from profile revision %d", profile.Revision)
			}
			check("profile settings survive recompilation", expectedErr)
		} else {
			check("profile settings survive recompilation", adapterErr)
		}
		for _, diagnostic := range manager.transparent.Diagnostics(
			ctx,
			document.Active.Core,
			*profile,
			document.Runtime.RuntimeConfig,
		) {
			if diagnostic.Warning {
				warn(diagnostic.Name, diagnostic.Err)
			} else {
				check(diagnostic.Name, diagnostic.Err)
			}
		}
		if profile.ManagementAPI.Enabled {
			check("external management API", probeExternalManagementAPI(ctx, profile.ManagementAPI))
		}
		if profile.TransparentProxy.Mode == subscriptions.TransparentProxyTUN ||
			(profile.TransparentProxy.Mode == subscriptions.TransparentProxyTProxy && profile.TransparentProxy.CaptureHost) {
			for _, result := range transparentNetworkProbes(ctx) {
				warn(result.name, result.err)
			}
		} else if profile.TransparentProxy.Mode == subscriptions.TransparentProxyTProxy {
			warn("LAN transparent traffic probe", fmt.Errorf("a live LAN client is required because capture_host is disabled"))
		}
	}
	if failures == 0 {
		if warnings == 0 {
			fmt.Fprintln(&builder, "All checks passed.")
		} else {
			fmt.Fprintf(&builder, "All required checks passed with %d warning(s).\n", warnings)
		}
	}
	report := strings.TrimRight(builder.String(), "\n")
	if failures > 0 {
		return report, fmt.Errorf("%w: %d check(s) failed", ErrDoctorFailed, failures)
	}
	return report, nil
}

func verifyConfigurationHash(path, expected string) error {
	actual, err := configurationFileHash(path)
	if err != nil {
		return err
	}
	return equalValue(actual, expected, "configuration content hash does not match recorded state")
}

func equalValue(actual, expected, message string) error {
	if actual != expected {
		return fmt.Errorf("%s: got %q, expected %q", message, actual, expected)
	}
	return nil
}

func probeExternalManagementAPI(ctx context.Context, config subscriptions.ManagementAPIConfig) error {
	host, port, err := net.SplitHostPort(config.ExternalController)
	if err != nil {
		return err
	}
	address := net.ParseIP(host)
	if host == "" || address != nil && address.IsUnspecified() {
		host = "127.0.0.1"
	}
	probeCtx, cancel := context.WithTimeout(ctx, 3*time.Second)
	defer cancel()
	request, err := http.NewRequestWithContext(probeCtx, http.MethodGet, "http://"+net.JoinHostPort(host, port)+"/version", nil)
	if err != nil {
		return err
	}
	request.Header.Set("Authorization", "Bearer "+config.Secret)
	response, err := (&http.Client{Transport: &http.Transport{Proxy: nil}}).Do(request)
	if err != nil {
		return err
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return fmt.Errorf("external controller returned HTTP %d", response.StatusCode)
	}
	return nil
}

type networkProbeResult struct {
	name string
	err  error
}

func transparentNetworkProbes(ctx context.Context) []networkProbeResult {
	probes := []struct {
		name string
		url  string
	}{
		{name: "domestic reachability through direct rules", url: "https://www.baidu.com/"},
		{name: "foreign reachability through proxy rules", url: "https://www.google.com/generate_204"},
	}
	results := make(chan networkProbeResult, len(probes)+1)
	for _, probe := range probes {
		go func() {
			probeCtx, cancel := context.WithTimeout(ctx, 8*time.Second)
			defer cancel()
			request, err := http.NewRequestWithContext(probeCtx, http.MethodGet, probe.url, nil)
			if err == nil {
				response, requestErr := (&http.Client{Transport: &http.Transport{Proxy: nil}}).Do(request)
				err = requestErr
				if response != nil {
					_ = response.Body.Close()
					if err == nil && (response.StatusCode < 200 || response.StatusCode >= 400) {
						err = fmt.Errorf("HTTP %d", response.StatusCode)
					}
				}
			}
			results <- networkProbeResult{name: probe.name, err: err}
		}()
	}
	go func() {
		probeCtx, cancel := context.WithTimeout(ctx, 8*time.Second)
		defer cancel()
		addresses, err := net.DefaultResolver.LookupIPAddr(probeCtx, "www.google.com")
		if err == nil {
			for _, value := range addresses {
				if value.IP.IsPrivate() || value.IP.IsLoopback() || value.IP.IsUnspecified() || value.IP.IsMulticast() {
					err = fmt.Errorf("resolver returned non-public address %s", value.IP)
					break
				}
			}
		}
		results <- networkProbeResult{name: "foreign DNS response sanity", err: err}
	}()
	output := make([]networkProbeResult, 0, cap(results))
	for range cap(results) {
		output = append(output, <-results)
	}
	return output
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
