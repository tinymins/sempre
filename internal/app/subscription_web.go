package app

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func (admin *adminServer) subscriptionsGet(writer http.ResponseWriter, _ *http.Request) {
	catalog, active, schedule, autoRestart, err := admin.manager.SubscriptionCatalog()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"profiles": catalog.Profiles, "active_profile_id": active, "schedule": schedule, "auto_restart": autoRestart, "targets": subscriptions.AvailableTargets(), "defaults": subscriptions.SystemDefaults()})
}

func (admin *adminServer) subscriptionsCreate(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Name string `json:"name"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	profile, err := admin.manager.CreateSubscriptionProfile(input.Name)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusCreated, profile)
}

func (admin *adminServer) subscriptionProfileGet(writer http.ResponseWriter, request *http.Request) {
	catalog, active, schedule, autoRestart, err := admin.manager.SubscriptionCatalog()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	profile, err := subscriptions.FindProfile(&catalog, request.PathValue("id"))
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"profile": profile, "active": active == profile.ID, "schedule": schedule, "auto_restart": autoRestart})
}

func (admin *adminServer) subscriptionProfilePut(writer http.ResponseWriter, request *http.Request) {
	var profile subscriptions.Profile
	if !admin.decode(writer, request, &profile) {
		return
	}
	change, result, err := admin.manager.SaveSubscriptionProfile(request.Context(), request.PathValue("id"), profile)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"change": change, "render": result})
}

func (admin *adminServer) subscriptionProfileDelete(writer http.ResponseWriter, request *http.Request) {
	change, err := admin.manager.RemoveSubscriptionProfile(request.PathValue("id"))
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, change)
}
func (admin *adminServer) subscriptionProfileActivate(writer http.ResponseWriter, request *http.Request) {
	change, result, err := admin.manager.UseSubscriptionProfile(request.Context(), request.PathValue("id"))
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"change": change, "render": result})
}
func (admin *adminServer) subscriptionProfileRefresh(writer http.ResponseWriter, request *http.Request) {
	change, result, err := admin.manager.RefreshSubscriptionProfile(request.Context(), request.PathValue("id"))
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"change": change, "render": result})
}

func (admin *adminServer) subscriptionProfileRender(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Format string `json:"format"`
		Force  bool   `json:"force"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	if input.Format == "" {
		input.Format = "sing-box-v13"
	}
	result, err := admin.manager.RenderSubscriptionProfile(request.Context(), request.PathValue("id"), input.Format, input.Force)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, result)
}

func (admin *adminServer) subscriptionProfileTrace(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Name   string `json:"name"`
		Format string `json:"format"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	if input.Format == "" {
		input.Format = "sing-box-v13"
	}
	result, err := admin.manager.TraceSubscriptionNode(request.Context(), request.PathValue("id"), input.Name, input.Format)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, result)
}

func (admin *adminServer) subscriptionSourceTest(writer http.ResponseWriter, request *http.Request) {
	var source subscriptions.Source
	if !admin.decode(writer, request, &source) {
		return
	}
	result, err := admin.manager.TestSubscriptionSource(request.Context(), source)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, result)
}

func (admin *adminServer) subscriptionSourceDebug(writer http.ResponseWriter, request *http.Request) {
	var source subscriptions.Source
	if !admin.decode(writer, request, &source) {
		return
	}
	admin.beginSSE(writer)
	admin.sse(writer, "fetch", map[string]any{"status": "started"})
	result, err := admin.manager.TestSubscriptionSource(request.Context(), source)
	if err != nil {
		admin.sse(writer, "error", map[string]string{"message": err.Error()})
		return
	}
	admin.sse(writer, "parse", map[string]any{"status": "complete", "format": result.Parse.Format, "nodes": len(result.Parse.Nodes), "diagnostics": result.Parse.Diagnostics})
	raw, err := admin.manager.subscriptions.ReadBlob(result.ContentHash)
	if err != nil {
		admin.sse(writer, "error", map[string]string{"message": err.Error()})
		return
	}
	admin.sse(writer, "result", map[string]any{"source": result, "raw_text": string(raw)})
}

func (admin *adminServer) subscriptionProfileDebug(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Format string `json:"format"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	if input.Format == "" {
		input.Format = "sing-box-v13"
	}
	admin.beginSSE(writer)
	admin.sse(writer, "pipeline", map[string]string{"stage": "fetch", "status": "started"})
	result, err := admin.manager.RenderSubscriptionProfile(request.Context(), request.PathValue("id"), input.Format, true)
	if err != nil {
		admin.sse(writer, "error", map[string]string{"message": err.Error()})
		return
	}
	for _, source := range result.SourceResults {
		raw, readErr := admin.manager.subscriptions.ReadBlob(source.ContentHash)
		if readErr != nil {
			admin.sse(writer, "error", map[string]string{"message": readErr.Error()})
			return
		}
		admin.sse(writer, "source", map[string]any{"result": source, "raw_text": string(raw)})
	}
	admin.sse(writer, "pipeline", map[string]any{"stage": "fetch", "status": "complete", "sources": len(result.SourceResults)})
	admin.sse(writer, "pipeline", map[string]any{"stage": "merge", "status": "complete", "nodes": result.NodeCount})
	admin.sse(writer, "pipeline", map[string]any{"stage": "convert", "status": "complete", "nodes": result.NodeCount, "field_diffs": len(result.FieldDiffs)})
	admin.sse(writer, "pipeline", map[string]any{"stage": "output", "status": "complete", "format": result.Format, "bytes": len(result.Content)})
	admin.sse(writer, "result", result)
}

func (admin *adminServer) subscriptionDefaults(writer http.ResponseWriter, _ *http.Request) {
	profile := subscriptions.NewProfile("")
	apiWriteJSON(writer, http.StatusOK, map[string]any{"profile": profile, "defaults": subscriptions.SystemDefaults(), "targets": subscriptions.AvailableTargets(), "source_defaults": subscriptions.Source{Type: subscriptions.SourceURL, Enabled: true, UserAgent: subscriptions.DefaultUserAgent, FetchMode: subscriptions.FetchAuto}})
}
func (admin *adminServer) subscriptionCacheClear(writer http.ResponseWriter, _ *http.Request) {
	change, err := admin.manager.ClearSubscriptionCache()
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, change)
}

func (admin *adminServer) customNodesGet(writer http.ResponseWriter, _ *http.Request) {
	nodes, err := admin.manager.CustomNodes()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"nodes": nodes})
}
func (admin *adminServer) customNodePost(writer http.ResponseWriter, request *http.Request) {
	var node subscriptions.CustomNode
	if !admin.decode(writer, request, &node) {
		return
	}
	node.ID = ""
	saved, err := admin.manager.SaveCustomNode(node)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusCreated, saved)
}
func (admin *adminServer) customNodePut(writer http.ResponseWriter, request *http.Request) {
	var node subscriptions.CustomNode
	if !admin.decode(writer, request, &node) {
		return
	}
	node.ID = request.PathValue("id")
	saved, err := admin.manager.SaveCustomNode(node)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, saved)
}
func (admin *adminServer) customNodeDelete(writer http.ResponseWriter, request *http.Request) {
	change, err := admin.manager.RemoveCustomNode(request.PathValue("id"))
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, change)
}

func (admin *adminServer) beginSSE(writer http.ResponseWriter) {
	writer.Header().Set("Content-Type", "text/event-stream")
	writer.Header().Set("Cache-Control", "no-store")
	writer.Header().Set("X-Accel-Buffering", "no")
	writer.WriteHeader(http.StatusOK)
	if flusher, ok := writer.(http.Flusher); ok {
		flusher.Flush()
	}
}
func (admin *adminServer) sse(writer http.ResponseWriter, event string, value any) {
	data, err := json.Marshal(value)
	if err != nil {
		data = []byte(`{"message":"encode debug event failed"}`)
	}
	fmt.Fprintf(writer, "event: %s\ndata: %s\n\n", strings.ReplaceAll(event, "\n", ""), data)
	if flusher, ok := writer.(http.Flusher); ok {
		flusher.Flush()
	}
}
