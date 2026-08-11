package app

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"github.com/tinymins/sempre/internal/supervisor"
)

func configurationFileHash(path string) (string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return "", fmt.Errorf("read prepared runtime configuration: %w", err)
	}
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:]), nil
}

func (manager *Manager) rollbackPendingDeployment(stage string, failure error) (bool, error) {
	retry := false
	changed := false
	err := manager.store.Update(func(document *state.Document) error {
		document.LastError = fmt.Sprintf("%s: %v", stage, failure)
		if document.Pending {
			changed = true
			failedCore := ""
			if document.Active != nil {
				failedCore = document.Active.Core
			}
			if document.Previous != nil {
				restored := *document.Previous
				document.Active = &restored
				document.Configs[restored.Core] = restored.ConfigHash
				if failedCore == restored.Core {
					delete(document.ConfigBuilds, restored.Core)
				}
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
		document.Runtime.LastError = fmt.Sprint(failure)
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

func (manager *Manager) markRuntimeHealthy(document *state.Document, plan supervisor.Plan) (string, string, string) {
	var cleanupCore, cleanupRepository, cleanupVersion string
	if document.Pending && state.SameDeployment(document.Active, &plan.Deployment) {
		old := document.Previous
		document.Previous = nil
		document.Pending = false
		if old != nil && manager.collectWeakVersion(document, old.Core, old.Repository, old.Version) {
			cleanupCore = old.Core
			cleanupRepository = old.Repository
			cleanupVersion = old.Version
		}
	}
	document.LastError = ""
	document.Runtime.State = "running"
	document.Runtime.LastError = ""
	document.Runtime.LastTransition = time.Now().UTC()
	return cleanupCore, cleanupRepository, cleanupVersion
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
	if err != nil || document.Subscription.Interval == "off" {
		return 0, false
	}
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return 0, false
	}
	profile, err := subscriptions.FindProfile(&catalog, document.ActiveProfileID)
	if err != nil || !subscriptionProfileHasScheduledSources(*profile) {
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
	binary := coreBinaryPath(manager.paths, adapter, deployment.Repository, deployment.Version)
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
