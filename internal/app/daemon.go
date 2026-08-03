package app

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/control"
	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	"github.com/tinymins/sempre/internal/supervisor"
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
	runner := supervisor.Runner{
		Stdout: stdout,
		Stderr: stderr,
		Hooks: supervisor.Hooks{
			Resolve: func(runCtx context.Context) (supervisor.Plan, error) {
				document, err := manager.store.Read()
				if err != nil {
					return supervisor.Plan{}, err
				}
				if document.Active == nil {
					return supervisor.Plan{}, supervisor.ErrIdle
				}
				deployment, adapter, err := manager.active(document)
				if err != nil {
					return supervisor.Plan{}, err
				}
				binary := manager.paths.CoreBinary(deployment.Core, deployment.Repository, deployment.Version)
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
				if err := manager.validateConfiguration(runCtx, adapter, binary, config, logger, logger); err != nil {
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
				return supervisor.Plan{
					Deployment: deployment,
					Spec:       adapter.Run(binary, runtimeSpec.Config, dataDir),
					Control:    runtimeSpec.Control,
				}, nil
			},
			ResolveFailure: func(failure error) (bool, error) {
				logf("resolve deployment failed: %v", failure)
				return manager.rollbackPendingDeployment("resolve failed", failure)
			},
			ScheduledUpdate: func(updateCtx context.Context) (bool, error) {
				change, err := manager.UpdateSubscription(updateCtx)
				if err != nil {
					return false, err
				}
				logf("%s", change.Message)
				return change.Changed, nil
			},
			NextUpdate: manager.nextSubscriptionUpdate,
			Started: func(plan supervisor.Plan, pid int) error {
				logf("started %s with PID %d", deploymentLabel(plan.Deployment), pid)
				if plan.Control.BaseURL != "" {
					manager.setControl(control.New(plan.Control.BaseURL, plan.Control.Secret))
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
					document.Runtime = state.Runtime{
						State:          "starting",
						PID:            pid,
						Core:           plan.Deployment.Core,
						Repository:     plan.Deployment.Repository,
						Version:        plan.Deployment.Version,
						StartedAt:      time.Now().UTC(),
						RestartCount:   document.Runtime.RestartCount,
						LastTransition: time.Now().UTC(),
					}
					return nil
				})
			},
			Healthy: func(plan supervisor.Plan) error {
				logf("healthy %s", deploymentLabel(plan.Deployment))
				var cleanupCore, cleanupRepository, cleanupVersion string
				err := manager.store.Update(func(document *state.Document) error {
					if document.Pending && state.SameDeployment(document.Active, &plan.Deployment) {
						old := document.Previous
						document.Previous = nil
						document.Pending = false
						document.LastError = ""
						if old != nil && manager.collectWeakVersion(document, old.Core, old.Repository, old.Version) {
							cleanupCore = old.Core
							cleanupRepository = old.Repository
							cleanupVersion = old.Version
						}
					}
					document.Runtime.State = "running"
					document.Runtime.LastTransition = time.Now().UTC()
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
			EarlyFailure: func(plan supervisor.Plan, failure error) error {
				logf("startup failed for %s: %v", deploymentLabel(plan.Deployment), failure)
				_, err := manager.rollbackPendingDeployment(
					"startup failed for "+deploymentLabel(plan.Deployment),
					failure,
				)
				return err
			},
			Exited: func(plan supervisor.Plan, failure error, restarts int) error {
				manager.setControl(nil)
				_ = os.Remove(manager.paths.CoreControl)
				logf("exited %s: %v", deploymentLabel(plan.Deployment), failure)
				return manager.store.Update(func(document *state.Document) error {
					document.Runtime.State = "restarting"
					document.Runtime.PID = 0
					document.Runtime.RestartCount = restarts
					document.Runtime.LastExit = fmt.Sprint(failure)
					document.Runtime.LastTransition = time.Now().UTC()
					return nil
				})
			},
			Stopped: func() error {
				manager.setControl(nil)
				_ = os.Remove(manager.paths.CoreControl)
				logf("daemon stopped")
				return manager.store.Update(func(document *state.Document) error {
					document.Runtime.State = "stopped"
					document.Runtime.PID = 0
					document.Runtime.LastTransition = time.Now().UTC()
					return nil
				})
			},
			Idle: func() error {
				manager.setControl(nil)
				_ = os.Remove(manager.paths.CoreControl)
				logf("waiting for an active core deployment")
				return manager.store.Update(func(document *state.Document) error {
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
		return manager.runControlPlane(serviceContext, runner.Run)
	})
}

func (manager *Manager) rollbackPendingDeployment(stage string, failure error) (bool, error) {
	retry := false
	changed := false
	err := manager.store.Update(func(document *state.Document) error {
		document.LastError = fmt.Sprintf("%s: %v", stage, failure)
		if document.Pending {
			changed = true
			if document.Previous != nil {
				restored := *document.Previous
				document.Active = &restored
				document.Configs[restored.Core] = restored.ConfigHash
				retry = true
			} else {
				document.Active = nil
			}
			document.Previous = nil
			document.Pending = false
		}
		document.Runtime.State = "failed"
		document.Runtime.PID = 0
		document.Runtime.LastExit = fmt.Sprint(failure)
		document.Runtime.LastTransition = time.Now().UTC()
		return nil
	})
	if err != nil {
		return false, err
	}
	if changed {
		if err := manager.garbageCollectConfigs(); err != nil {
			return false, err
		}
	}
	return retry, nil
}

func (manager *Manager) garbageCollectConfigs() error {
	lease, err := manager.store.AcquireConfig()
	if err != nil {
		return err
	}
	defer lease.Release()
	document, err := manager.store.Read()
	if err != nil {
		return err
	}
	references := referencedConfigs(document)
	coreDirectories, err := os.ReadDir(manager.paths.Configs)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	for _, coreDirectory := range coreDirectories {
		if !coreDirectory.IsDir() || coreDirectory.Type()&os.ModeSymlink != 0 {
			continue
		}
		coreID := coreDirectory.Name()
		directory := filepath.Join(manager.paths.Configs, coreID)
		entries, err := os.ReadDir(directory)
		if err != nil {
			return err
		}
		for _, entry := range entries {
			if entry.IsDir() || entry.Type()&os.ModeSymlink != 0 ||
				filepath.Ext(entry.Name()) != ".json" {
				continue
			}
			hash := strings.TrimSuffix(entry.Name(), ".json")
			if !references[coreID][hash] {
				if err := os.Remove(filepath.Join(directory, entry.Name())); err != nil {
					return err
				}
			}
		}
		remaining, err := os.ReadDir(directory)
		if err != nil {
			return err
		}
		if len(remaining) == 0 {
			if err := os.Remove(directory); err != nil {
				return err
			}
		}
	}
	return nil
}

func (manager *Manager) nextSubscriptionUpdate() (time.Duration, bool) {
	document, err := manager.store.Read()
	if err != nil || document.Subscription.URL == "" || document.Subscription.Interval == "off" {
		return 0, false
	}
	interval, err := time.ParseDuration(document.Subscription.Interval)
	if err != nil {
		return 0, false
	}
	if document.Subscription.LastCheck.IsZero() {
		return time.Second, true
	}
	return time.Until(document.Subscription.LastCheck.Add(interval)), true
}

func (manager *Manager) deploymentSpec(ctx context.Context, referenceValue string) (state.Deployment, core.RunSpec, error) {
	document, err := manager.store.Read()
	if err != nil {
		return state.Deployment{}, core.RunSpec{}, err
	}
	deployment, adapter, err := manager.active(document)
	if err != nil {
		return state.Deployment{}, core.RunSpec{}, err
	}
	if referenceValue != "" {
		reference, resolvedAdapter, err := manager.resolveReference(referenceValue)
		if err != nil {
			return state.Deployment{}, core.RunSpec{}, err
		}
		version, err := resolveInstalledVersion(document, reference)
		if err != nil {
			return state.Deployment{}, core.RunSpec{}, err
		}
		adapter = resolvedAdapter
		configHash := document.Configs[reference.Core]
		if configHash == "" {
			return state.Deployment{}, core.RunSpec{}, fmt.Errorf("%s has no active configuration", reference.Core)
		}
		deployment = state.Deployment{
			Core:       reference.Core,
			Repository: reference.Repository,
			Ref:        reference.Value,
			Version:    version,
			ConfigHash: configHash,
		}
	}
	binary := manager.paths.CoreBinary(deployment.Core, deployment.Repository, deployment.Version)
	config := manager.paths.Config(deployment.Core, deployment.ConfigHash)
	dataDir := filepath.Join(manager.paths.Runtime, deployment.Core)
	if err := os.MkdirAll(dataDir, 0o700); err != nil {
		return state.Deployment{}, core.RunSpec{}, err
	}
	if err := manager.validateConfiguration(ctx, adapter, binary, config, manager.output, manager.errors); err != nil {
		return state.Deployment{}, core.RunSpec{}, err
	}
	return deployment, adapter.Run(binary, config, dataDir), nil
}
