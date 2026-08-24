package app

import (
	"context"
	"errors"
	"fmt"
	"os"
	"slices"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
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
		return manager.installSystemService(ctx, allowReplace, false)
	})
}

func (manager *Manager) InstallApplication(ctx context.Context, allowReplace, replaceUI bool) error {
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
			return source.installApplicationService(ctx, allowReplace, replaceUI)
		} else if !errors.Is(err, os.ErrNotExist) {
			return err
		}
	}
	return manager.installApplicationService(ctx, allowReplace, replaceUI)
}

func (manager *Manager) installApplicationService(ctx context.Context, allowReplace, replaceUI bool) error {
	return manager.withSystemOperation(func() error {
		return manager.installSystemService(ctx, allowReplace, replaceUI)
	})
}

func (manager *Manager) RestoreBundleApplication(ctx context.Context, allowReplace bool) error {
	return manager.withSystemOperation(func() error {
		return manager.RestoreBundle(ctx, allowReplace)
	})
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
		return errors.Join(manager.gateway.Stop(ctx), manager.transparent.Cleanup(ctx))
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
		gatewayErr := manager.gateway.Stop(ctx)
		cleanupErr := manager.transparent.Cleanup(ctx)
		return errors.Join(stopErr, gatewayErr, cleanupErr)
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
		build := document.ConfigBuilds[document.Active.Core]
		if initialConfigurationWithoutProfileBuild(*profile, build) {
			fmt.Fprintln(&builder, "[INFO] profile settings survive recompilation: not applicable to the initial manually imported configuration")
		} else if adapterErr == nil {
			expected, expectedErr := expectedConfigBuild(*profile, adapter, document.Active.Version)
			if expectedErr == nil && build != expected {
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
		if adapterErr == nil && slices.Contains(manager.registry.Capabilities(adapter, document.Active.Version, core.CurrentTarget()).Features, core.CapabilityManagementExternalAPI) {
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
