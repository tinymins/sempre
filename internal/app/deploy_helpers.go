package app

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"sort"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
)

func deploymentDocument(document state.Document) state.Document {
	document.Normalize()
	document.Runtime = state.Runtime{}
	return document
}

func meaningfulState(document state.Document) bool {
	return meaningfulDeploymentState(document) ||
		document.DesiredState == state.DesiredStopped
}

func meaningfulDeploymentState(document state.Document) bool {
	return document.Selected != nil ||
		document.Active != nil ||
		document.Previous != nil ||
		len(document.Cores) != 0 ||
		len(document.Configs) != 0 ||
		document.Subscription.URL != ""
}

func sameDeploymentData(left, right state.Document) bool {
	left = deploymentDocument(left)
	right = deploymentDocument(right)
	left.UpdatedAt = right.UpdatedAt
	return reflect.DeepEqual(left, right)
}

func (manager *Manager) sameSubscriptionCatalog(target layout.Layout) (bool, error) {
	left, err := os.ReadFile(manager.paths.SubscriptionStore)
	if err != nil {
		return false, fmt.Errorf("read portable subscription catalog: %w", err)
	}
	right, err := os.ReadFile(target.SubscriptionStore)
	if errors.Is(err, os.ErrNotExist) {
		return false, nil
	}
	if err != nil {
		return false, fmt.Errorf("read system subscription catalog: %w", err)
	}
	return bytes.Equal(left, right), nil
}

func deploymentReplacementSummary(document state.Document) string {
	selected := "none"
	if document.Selected != nil {
		selected = selectionRef(*document.Selected).String()
	}
	active := "none"
	if document.Active != nil {
		active = deploymentLabel(*document.Active)
	}
	configured := "no"
	if document.ActiveProfileID != "" || document.Subscription.URL != "" {
		configured = "yes"
	}
	versions := 0
	for _, coreState := range document.Cores {
		for _, source := range coreState.SourceEntries() {
			versions += len(source.State.Installed)
		}
	}
	return fmt.Sprintf(
		"Existing system data will be replaced:\n  Selected: %s\n  Active: %s\n  Core versions: %d\n  Subscription configured: %s",
		selected,
		active,
		versions,
		configured,
	)
}

func referencedConfigs(document state.Document) map[string]map[string]bool {
	result := map[string]map[string]bool{}
	add := func(coreID, hash string) {
		if coreID == "" || hash == "" {
			return
		}
		if result[coreID] == nil {
			result[coreID] = map[string]bool{}
		}
		result[coreID][hash] = true
	}
	for coreID, hash := range document.Configs {
		add(coreID, hash)
	}
	if document.Active != nil {
		add(document.Active.Core, document.Active.ConfigHash)
	}
	if document.Previous != nil {
		add(document.Previous.Core, document.Previous.ConfigHash)
	}
	return result
}

func sortedCoreIDs(document state.Document) []string {
	ids := make([]string, 0, len(document.Cores))
	for coreID := range document.Cores {
		ids = append(ids, coreID)
	}
	sort.Strings(ids)
	return ids
}

type installedCore struct {
	Repository string
	Version    string
}

func sortedInstallations(coreState *state.CoreState) []installedCore {
	var installations []installedCore
	for _, source := range coreState.SourceEntries() {
		for version := range source.State.Installed {
			installations = append(installations, installedCore{Repository: source.Repository, Version: version})
		}
	}
	sort.Slice(installations, func(i, j int) bool {
		if installations[i].Repository == installations[j].Repository {
			return installations[i].Version < installations[j].Version
		}
		return installations[i].Repository < installations[j].Repository
	})
	return installations
}

func stagedCoreVersionDir(staging, coreID, repository, version string) string {
	if repository == "" {
		return filepath.Join(staging, coreID, version)
	}
	return filepath.Join(staging, coreID, "sources", filepath.FromSlash(repository), version)
}

func validateCoreVersion(coreID, version string) error {
	ref, err := core.ParseRef(coreID + "@" + version)
	if err != nil || ref.IsChannel() {
		return fmt.Errorf("invalid managed core version %s@%s", coreID, version)
	}
	return nil
}
