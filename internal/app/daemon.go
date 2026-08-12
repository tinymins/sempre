package app

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/tinymins/sempre/internal/clashproxy"
	"github.com/tinymins/sempre/internal/control"
	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"github.com/tinymins/sempre/internal/supervisor"
	"github.com/tinymins/sempre/internal/transparentproxy"
)

func (manager *Manager) RunDaemon(ctx context.Context) error {
	lease, err := manager.store.AcquireInstance()
	if err != nil {
		return err
	}
	defer lease.Release()
	logger := supervisor.NewRollingWriter(manager.paths.ManagerLog, 10<<20, 3)
	stdout := supervisor.NewRollingWriter(manager.paths.StdoutLog, 10<<20, 3)
	stderr := supervisor.NewRollingWriter(manager.paths.StderrLog, 10<<20, 3)
	logf := func(format string, arguments ...any) {
		_, _ = fmt.Fprintf(logger, time.Now().UTC().Format(time.RFC3339)+" "+format+"\n", arguments...)
	}
	dataPlanePlan := transparentproxy.Plan{}
	externalClashPlan := clashproxy.Config{}
	runtimeConfig := ""
	runtimeConfigHash := ""
	runner := supervisor.Runner{
		Stdout: stdout,
		Stderr: stderr,
		Hooks: supervisor.Hooks{
			Resolve: func(runCtx context.Context) (supervisor.Plan, error) {
				document, err := manager.store.Read()
				if err != nil {
					return supervisor.Plan{}, err
				}
				if document.DesiredState == state.DesiredStopped {
					return supervisor.Plan{}, supervisor.ErrStopped
				}
				if document.Active == nil {
					return supervisor.Plan{}, supervisor.ErrIdle
				}
				deployment, adapter, err := manager.active(document)
				if err != nil {
					return supervisor.Plan{}, err
				}
				binary := coreBinaryPath(manager.paths, adapter, deployment.Repository, deployment.Version)
				config := manager.paths.Config(deployment.Core, deployment.ConfigHash)
				if _, err := os.Stat(binary); err != nil {
					return supervisor.Plan{}, fmt.Errorf("active core binary is unavailable: %w", err)
				}
				if _, err := os.Stat(config); err != nil {
					return supervisor.Plan{}, fmt.Errorf("active configuration is unavailable: %w", err)
				}
				dataDir := filepath.Join(manager.paths.Runtime, deployment.Core)
				if err := os.MkdirAll(dataDir, 0o700); err != nil {
					return supervisor.Plan{}, err
				}
				runtimeSpec := core.RuntimeSpec{Config: config}
				if preparer, ok := adapter.(core.RuntimePreparer); ok {
					runtimeDirectory := filepath.Join(manager.paths.Runtime, deployment.Core, "control")
					if err := os.RemoveAll(runtimeDirectory); err != nil {
						return supervisor.Plan{}, err
					}
					runtimeSpec, err = preparer.PrepareRuntime(config, runtimeDirectory)
					if err != nil {
						return supervisor.Plan{}, err
					}
				}
				catalog, err := manager.subscriptions.Read()
				if err != nil {
					return supervisor.Plan{}, err
				}
				profile, err := subscriptions.FindProfile(&catalog, document.ActiveProfileID)
				if err != nil {
					return supervisor.Plan{}, err
				}
				externalClashPlan = clashproxy.Config{}
				if runtimeSpec.Control.Protocol == core.ControlProtocolClashREST {
					externalClashPlan = clashproxy.Config{External: profile.ManagementAPI, Upstream: runtimeSpec.Control}
				}
				dataPlanePlan, err = manager.transparent.Prepare(
					runCtx,
					deployment.Core,
					*profile,
					runtimeSpec.Config,
				)
				if err != nil {
					return supervisor.Plan{}, err
				}
				if err := manager.validateConfiguration(runCtx, adapter, binary, runtimeSpec.Config, logger, logger); err != nil {
					return supervisor.Plan{}, err
				}
				runtimeConfig = runtimeSpec.Config
				runtimeConfigHash, err = configurationFileHash(runtimeConfig)
				if err != nil {
					return supervisor.Plan{}, err
				}
				return supervisor.Plan{
					Deployment: deployment,
					Spec:       adapter.Run(binary, runtimeSpec.Config, dataDir),
					Control:    runtimeSpec.Control,
				}, nil
			},
			ResolveFailure: func(failure error) (bool, error) {
				if stopErr := manager.externalClash.Stop(ctx); stopErr != nil {
					logf("stop external Clash API after resolve failure: %v", stopErr)
				}
				if stopErr := manager.gateway.Stop(ctx); stopErr != nil {
					logf("stop LAN gateway services after resolve failure: %v", stopErr)
				}
				if cleanupErr := manager.transparent.Cleanup(ctx); cleanupErr != nil {
					logf("clean transparent proxy after resolve failure: %v", cleanupErr)
				}
				logf("resolve deployment failed: %v", failure)
				return manager.rollbackPendingDeployment("resolve failed", failure)
			},
			ScheduledUpdate: func(updateCtx context.Context) (bool, error) {
				change, err := manager.UpdateSubscription(updateCtx)
				if err != nil {
					return false, err
				}
				logf("%s", change.Message)
				document, readErr := manager.store.Read()
				if readErr != nil {
					return false, readErr
				}
				return change.Changed && document.AutoRestart, nil
			},
			NextUpdate: manager.nextSubscriptionUpdate,
			AcquireStart: func(plan supervisor.Plan) (func(), bool, error) {
				manager.lifecycleMu.Lock()
				release := manager.lifecycleMu.Unlock
				document, err := manager.store.Read()
				if err != nil {
					return release, false, err
				}
				allowed := document.DesiredState == state.DesiredRunning &&
					state.SameDeployment(document.Active, &plan.Deployment)
				return release, allowed, nil
			},
			Starting: func(plan supervisor.Plan) error {
				logf("starting %s", deploymentLabel(plan.Deployment))
				if err := manager.externalClash.Stop(ctx); err != nil {
					return fmt.Errorf("stop stale external Clash API: %w", err)
				}
				if err := manager.gateway.Stop(ctx); err != nil {
					return fmt.Errorf("stop stale LAN gateway services: %w", err)
				}
				if err := manager.transparent.Cleanup(ctx); err != nil {
					return fmt.Errorf("clean stale Linux transparent proxy state: %w", err)
				}
				return manager.store.Update(func(document *state.Document) error {
					document.Runtime.State = "starting"
					document.Runtime.PID = 0
					document.Runtime.Core = plan.Deployment.Core
					document.Runtime.Repository = plan.Deployment.Repository
					document.Runtime.Ref = plan.Deployment.Ref
					document.Runtime.Version = plan.Deployment.Version
					document.Runtime.ConfigHash = plan.Deployment.ConfigHash
					document.Runtime.RuntimeConfig = runtimeConfig
					document.Runtime.RuntimeHash = runtimeConfigHash
					document.Runtime.StartedAt = time.Time{}
					document.Runtime.LastTransition = time.Now().UTC()
					return nil
				})
			},
			Started: func(plan supervisor.Plan, pid int) error {
				logf("started %s with PID %d", deploymentLabel(plan.Deployment), pid)
				if err := manager.transparent.Apply(ctx, dataPlanePlan); err != nil {
					return fmt.Errorf("activate Linux transparent proxy: %w", err)
				}
				gatewayConfig, err := manager.gateway.Read()
				if err != nil {
					return fmt.Errorf("read gateway configuration: %w", err)
				}
				if err := manager.gateway.Start(ctx, gatewayConfig); err != nil {
					return fmt.Errorf("start LAN gateway services: %w", err)
				}
				if err := manager.externalClash.Start(ctx, externalClashPlan); err != nil {
					return fmt.Errorf("start external Clash API: %w", err)
				}
				if plan.Control.BaseURL != "" {
					if plan.Control.Protocol == core.ControlProtocolClashREST {
						manager.setControl(control.New(plan.Control.Core, plan.Control.BaseURL, plan.Control.Secret))
					} else {
						manager.setControl(nil)
					}
					data, err := json.Marshal(plan.Control)
					if err != nil {
						return err
					}
					if err := state.WriteAtomic(manager.paths.CoreControl, append(data, '\n'), 0o600); err != nil {
						return err
					}
				} else {
					manager.setControl(nil)
					_ = os.Remove(manager.paths.CoreControl)
				}
				return manager.store.Update(func(document *state.Document) error {
					document.Runtime.State = "starting"
					document.Runtime.PID = pid
					document.Runtime.Core = plan.Deployment.Core
					document.Runtime.Repository = plan.Deployment.Repository
					document.Runtime.Ref = plan.Deployment.Ref
					document.Runtime.Version = plan.Deployment.Version
					document.Runtime.ConfigHash = plan.Deployment.ConfigHash
					document.Runtime.RuntimeConfig = runtimeConfig
					document.Runtime.RuntimeHash = runtimeConfigHash
					document.Runtime.StartedAt = time.Now().UTC()
					return nil
				})
			},
			Healthy: func(plan supervisor.Plan) error {
				logf("healthy %s", deploymentLabel(plan.Deployment))
				if err := manager.transparent.Verify(ctx, dataPlanePlan); err != nil {
					return fmt.Errorf("verify Linux transparent proxy: %w", err)
				}
				var cleanupCore, cleanupRepository, cleanupVersion string
				err := manager.store.Update(func(document *state.Document) error {
					cleanupCore, cleanupRepository, cleanupVersion = manager.markRuntimeHealthy(document, plan)
					return nil
				})
				if err == nil && cleanupVersion != "" {
					_ = os.RemoveAll(manager.paths.CoreVersionDir(cleanupCore, cleanupRepository, cleanupVersion))
				}
				if err == nil {
					err = manager.garbageCollectConfigs()
				}
				return err
			},
			Stopping: func(plan supervisor.Plan) error {
				logf("stopping %s", deploymentLabel(plan.Deployment))
				if err := manager.externalClash.Stop(ctx); err != nil {
					logf("stop external Clash API: %v", err)
				}
				if err := manager.gateway.Stop(ctx); err != nil {
					logf("stop LAN gateway services: %v", err)
				}
				if err := manager.transparent.Cleanup(ctx); err != nil {
					logf("clean Linux transparent proxy while stopping: %v", err)
				}
				return manager.store.Update(func(document *state.Document) error {
					document.Runtime.State = "stopping"
					if document.DesiredState == state.DesiredStopped {
						document.Runtime.LastExit = "stopped by user"
					} else {
						document.Runtime.LastExit = "restart requested"
					}
					document.Runtime.LastTransition = time.Now().UTC()
					return nil
				})
			},
			Restarting: func(plan supervisor.Plan) error {
				logf("restarting %s", deploymentLabel(plan.Deployment))
				return manager.store.Update(func(document *state.Document) error {
					document.Runtime.State = "restarting"
					document.Runtime.PID = 0
					document.Runtime.LastTransition = time.Now().UTC()
					return nil
				})
			},
			EarlyFailure: func(plan supervisor.Plan, failure error) error {
				if stopErr := manager.externalClash.Stop(ctx); stopErr != nil {
					logf("stop external Clash API after startup failure: %v", stopErr)
				}
				if stopErr := manager.gateway.Stop(ctx); stopErr != nil {
					logf("stop LAN gateway services after startup failure: %v", stopErr)
				}
				if cleanupErr := manager.transparent.Cleanup(ctx); cleanupErr != nil {
					logf("clean Linux transparent proxy after startup failure: %v", cleanupErr)
				}
				logf("startup failed for %s: %v", deploymentLabel(plan.Deployment), failure)
				_, err := manager.rollbackPendingDeployment(
					"startup failed for "+deploymentLabel(plan.Deployment),
					failure,
				)
				return err
			},
			Exited: func(plan supervisor.Plan, failure error, _ int) error {
				if stopErr := manager.externalClash.Stop(ctx); stopErr != nil {
					logf("stop external Clash API after core exit: %v", stopErr)
				}
				if stopErr := manager.gateway.Stop(ctx); stopErr != nil {
					logf("stop LAN gateway services after core exit: %v", stopErr)
				}
				if cleanupErr := manager.transparent.Cleanup(ctx); cleanupErr != nil {
					logf("clean Linux transparent proxy after core exit: %v", cleanupErr)
				}
				manager.setControl(nil)
				_ = os.Remove(manager.paths.CoreControl)
				logf("exited %s: %v", deploymentLabel(plan.Deployment), failure)
				return manager.store.Update(func(document *state.Document) error {
					if document.DesiredState == state.DesiredStopped {
						document.Runtime.State = "stopped"
					} else {
						document.Runtime.State = "failed"
						document.Runtime.LastError = fmt.Sprint(failure)
					}
					document.Runtime.RestartCount++
					document.Runtime.PID = 0
					document.Runtime.LastExit = fmt.Sprint(failure)
					document.Runtime.LastTransition = time.Now().UTC()
					return nil
				})
			},
			Stopped: func() error {
				if stopErr := manager.externalClash.Stop(ctx); stopErr != nil {
					logf("stop external Clash API after stop: %v", stopErr)
				}
				if stopErr := manager.gateway.Stop(ctx); stopErr != nil {
					logf("stop LAN gateway services after stop: %v", stopErr)
				}
				if cleanupErr := manager.transparent.Cleanup(ctx); cleanupErr != nil {
					logf("clean Linux transparent proxy after stop: %v", cleanupErr)
				}
				manager.setControl(nil)
				_ = os.Remove(manager.paths.CoreControl)
				logf("managed core stopped")
				return manager.store.Update(func(document *state.Document) error {
					if document.Active == nil {
						if document.DesiredState == state.DesiredStopped {
							document.Runtime.State = "stopped"
							document.Runtime.PID = 0
							document.Runtime.LastExit = "stopped by user"
							document.Runtime.LastTransition = time.Now().UTC()
							return nil
						}
						if document.Runtime.LastError != "" || document.LastError != "" {
							document.Runtime.State = "failed"
							document.Runtime.PID = 0
							return nil
						}
						if document.Runtime.State == "idle" && document.Runtime.PID == 0 {
							return nil
						}
						document.Runtime = state.Runtime{
							State:          "idle",
							LastTransition: time.Now().UTC(),
						}
						return nil
					}
					if document.Runtime.State == "stopped" && document.Runtime.PID == 0 {
						return nil
					}
					document.Runtime.State = "stopped"
					document.Runtime.PID = 0
					if document.DesiredState == state.DesiredStopped {
						document.Runtime.LastExit = "stopped by user"
					} else {
						document.Runtime.LastExit = "Sempre service stopped"
					}
					document.Runtime.LastTransition = time.Now().UTC()
					return nil
				})
			},
			Idle: func() error {
				if stopErr := manager.externalClash.Stop(ctx); stopErr != nil {
					logf("stop external Clash API while idle: %v", stopErr)
				}
				if stopErr := manager.gateway.Stop(ctx); stopErr != nil {
					logf("stop LAN gateway services while idle: %v", stopErr)
				}
				if cleanupErr := manager.transparent.Cleanup(ctx); cleanupErr != nil {
					logf("clean Linux transparent proxy while idle: %v", cleanupErr)
				}
				manager.setControl(nil)
				_ = os.Remove(manager.paths.CoreControl)
				logf("waiting for an active core deployment")
				return manager.store.Update(func(document *state.Document) error {
					if document.Runtime.LastError != "" || document.LastError != "" {
						document.Runtime.State = "failed"
						document.Runtime.PID = 0
						return nil
					}
					if document.Runtime.State == "idle" && document.Runtime.PID == 0 {
						return nil
					}
					document.Runtime = state.Runtime{
						State:          "idle",
						LastTransition: time.Now().UTC(),
					}
					return nil
				})
			},
			Log:    logf,
			Reload: manager.reload,
		},
	}
	return manager.service.Run(ctx, func(serviceContext context.Context) error {
		if err := manager.tunnels.Start(serviceContext); err != nil {
			return fmt.Errorf("start tunnel supervisor: %w", err)
		}
		defer manager.tunnels.Stop()
		return manager.runControlPlane(serviceContext, runner.Run)
	})
}
