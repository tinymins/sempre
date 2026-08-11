package app

import (
	"net/http"
	"sort"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func (admin *adminServer) cores(writer http.ResponseWriter, request *http.Request) {
	document, err := admin.manager.store.Read()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	type installation struct {
		Core         string              `json:"core"`
		Repository   string              `json:"repository"`
		Reference    string              `json:"reference"`
		Official     bool                `json:"official"`
		Version      string              `json:"version"`
		Channels     []string            `json:"channels"`
		Installation *state.Installation `json:"installation"`
	}
	result := make([]installation, 0)
	for coreID, coreState := range document.Cores {
		adapter, err := admin.manager.registry.Get(coreID)
		if err != nil {
			admin.internalError(writer, err)
			return
		}
		for _, source := range coreState.SourceEntries() {
			repository := source.Repository
			official := repository == ""
			if official {
				repository = adapter.DefaultRepository()
			}
			for version, item := range source.State.Installed {
				reference := core.Ref{Core: coreID, Repository: source.Repository, Value: version}.String()
				entry := installation{Core: coreID, Repository: repository, Reference: reference, Official: official, Version: version, Channels: []string{}, Installation: item}
				for channel, target := range source.State.Channels {
					if target == version {
						entry.Channels = append(entry.Channels, channel)
					}
				}
				sort.Strings(entry.Channels)
				result = append(result, entry)
			}
		}
	}
	sort.Slice(result, func(i, j int) bool {
		if result[i].Core != result[j].Core {
			return result[i].Core < result[j].Core
		}
		if result[i].Repository != result[j].Repository {
			return result[i].Repository < result[j].Repository
		}
		return result[i].Version < result[j].Version
	})
	apiWriteJSON(writer, http.StatusOK, map[string]any{
		"supported": admin.manager.CoreIDs(), "catalog": admin.manager.CoreDefinitions(), "installed": result,
		"selected": document.Selected, "active": document.Active,
	})
}

func (admin *adminServer) coreInstall(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Reference string `json:"reference"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	change, err := admin.manager.InstallCore(request.Context(), input.Reference)
	admin.writeChange(writer, change, err)
}

func (admin *adminServer) coreUpdate(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Reference string `json:"reference"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	changes, err := admin.manager.UpdateCores(request.Context(), input.Reference)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	for index := range changes {
		change := &changes[index]
		if change.NeedsRestart {
			reloaded, reloadErr := admin.manager.RequestReloadIfRunning()
			if reloadErr != nil {
				admin.internalError(writer, reloadErr)
				return
			}
			if !reloaded {
				change.Message += "; it will take effect the next time the managed core starts"
			}
		}
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"changes": changes})
}

func (admin *adminServer) coreUse(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Reference string `json:"reference"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	change, err := admin.manager.UseCore(request.Context(), input.Reference)
	admin.writeChange(writer, change, err)
}

func (admin *adminServer) coreRemove(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Reference string `json:"reference"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	change, err := admin.manager.RemoveCore(input.Reference)
	admin.writeChange(writer, change, err)
}

func (admin *adminServer) subscriptionGet(writer http.ResponseWriter, request *http.Request) {
	catalog, active, schedule, autoRestart, err := admin.manager.SubscriptionCatalog()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"profile": profile, "interval": schedule.Interval, "last_check": schedule.LastCheck, "last_change": schedule.LastChange, "last_result": schedule.LastResult, "auto_restart": autoRestart})
}

func (admin *adminServer) subscriptionPatch(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		URL         *string `json:"url"`
		Interval    *string `json:"interval"`
		AutoRestart *bool   `json:"auto_restart"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	var changes []Change
	if input.URL != nil {
		change, err := admin.manager.SetSubscription(request.Context(), *input.URL)
		if err != nil {
			admin.operationError(writer, err)
			return
		}
		changes = append(changes, change)
	}
	if input.Interval != nil {
		change, err := admin.manager.SetSubscriptionSchedule(*input.Interval)
		if err != nil {
			admin.operationError(writer, err)
			return
		}
		changes = append(changes, change)
	}
	if input.AutoRestart != nil {
		change, err := admin.manager.SetSubscriptionAutoRestart(*input.AutoRestart)
		if err != nil {
			admin.operationError(writer, err)
			return
		}
		changes = append(changes, change)
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"changes": changes})
}

func (admin *adminServer) subscriptionUpdate(writer http.ResponseWriter, request *http.Request) {
	change, err := admin.manager.UpdateSubscription(request.Context())
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, change)
}

func (admin *adminServer) configGet(writer http.ResponseWriter, request *http.Request) {
	data, hash, err := admin.manager.CurrentConfigContent()
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"hash": hash, "content": string(data)})
}

func (admin *adminServer) configWriteRemoved(writer http.ResponseWriter, _ *http.Request) {
	apiWriteError(writer, http.StatusGone, "DIRECT_CONFIG_REMOVED", "generated configurations are read-only; edit a subscription profile instead", nil)
}

func (admin *adminServer) configValidate(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Content string `json:"content"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	if err := admin.manager.ValidateConfigContent(request.Context(), []byte(input.Content)); err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]bool{"valid": true})
}
