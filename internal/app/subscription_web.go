package app

import (
	"errors"
	"net/http"
	"time"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func (admin *adminServer) subscriptionsGet(writer http.ResponseWriter, _ *http.Request) {
	catalog, active, schedule, autoRestart, err := admin.manager.SubscriptionCatalog()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	configurationContext, err := admin.manager.SubscriptionConfigurationContext()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"profiles": catalog.Profiles, "active_profile_id": active, "schedule": schedule, "auto_restart": autoRestart, "targets": subscriptions.AvailableTargets(), "defaults": subscriptions.SystemDefaults(), "editor_defaults": subscriptions.RecommendedEditorDefaults(), "configuration_context": configurationContext})
}

func (admin *adminServer) subscriptionsCreate(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Name        string `json:"name"`
		Mode        string `json:"mode"`
		ManifestURL string `json:"manifest_url"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	var profile subscriptions.Profile
	var err error
	switch input.Mode {
	case "", subscriptions.ProfileLocal:
		profile, err = admin.manager.CreateSubscriptionProfile(input.Name)
	case subscriptions.ProfileRemote:
		profile, err = admin.manager.CreateRemoteSubscriptionProfile(input.Name, input.ManifestURL)
	default:
		apiWriteError(writer, http.StatusBadRequest, "INVALID_SUBSCRIPTION_MODE", "subscription mode must be local or remote", nil)
		return
	}
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

func (admin *adminServer) subscriptionProfilePatch(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Name string `json:"name"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	profile, err := admin.manager.RenameSubscriptionProfile(request.PathValue("id"), input.Name)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, profile)
}

func (admin *adminServer) subscriptionProfilePut(writer http.ResponseWriter, request *http.Request) {
	var profile subscriptions.Profile
	if !admin.decode(writer, request, &profile) {
		return
	}
	change, result, err := admin.manager.SaveSubscriptionProfileForContext(
		request.Context(), request.PathValue("id"), profile,
		request.Header.Get("X-Sempre-Configuration-Context"),
	)
	if err != nil {
		if errors.Is(err, errSubscriptionConfigurationContextChanged) {
			apiWriteError(writer, http.StatusConflict, "CONFIGURATION_CONTEXT_CHANGED", err.Error(), nil)
			return
		}
		admin.operationError(writer, err)
		return
	}
	catalog, _, _, _, err := admin.manager.SubscriptionCatalog()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	saved, err := subscriptions.FindProfile(&catalog, request.PathValue("id"))
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"change": change, "profile": saved, "render": result})
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

func (admin *adminServer) subscriptionProfilePreviewNodes(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Format string `json:"format"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	nodes, err := admin.manager.PreviewSubscriptionNodes(request.Context(), request.PathValue("id"), true)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"nodes": nodes})
}

func (admin *adminServer) subscriptionProfileTraceNode(writer http.ResponseWriter, request *http.Request) {
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
	result, err := admin.manager.TraceSubscriptionNodeSteps(request.Context(), request.PathValue("id"), input.Name, input.Format)
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
	var input struct {
		URL             string `json:"url"`
		UA              string `json:"ua"`
		Prefix          string `json:"prefix"`
		CacheTTLMinutes int    `json:"cacheTtlMinutes"`
		Mode            string `json:"mode"`
		FetchMode       string `json:"fetchMode"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	if input.UA == "" {
		input.UA = subscriptions.DefaultUserAgent
	}
	if input.FetchMode == "" {
		input.FetchMode = subscriptions.FetchAuto
	}
	source := subscriptions.Source{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true, URL: input.URL, Prefix: input.Prefix, UserAgent: input.UA, FetchMode: input.FetchMode, CacheTTLMinutes: input.CacheTTLMinutes}
	started := time.Now()
	admin.beginSSE(writer)
	admin.sse(writer, "message", map[string]any{"type": "config", "data": map[string]any{"url": input.URL, "ua": input.UA, "prefix": input.Prefix, "cacheTtlMinutes": input.CacheTTLMinutes, "mode": input.Mode, "fetchMode": input.FetchMode, "proxyEndpoint": nil, "maxAttempts": 3, "timeoutMs": 15000}})
	cacheStatus := "skipped"
	if input.Mode == "production" {
		cacheStatus = "miss"
	}
	admin.sse(writer, "message", map[string]any{"type": "cache", "data": map[string]any{"status": cacheStatus, "cacheTtlMinutes": input.CacheTTLMinutes, "payload": nil}})
	admin.sse(writer, "message", sourceNetworkStep(input.URL, input.FetchMode))
	admin.sse(writer, "message", map[string]any{"type": "attempt-start", "data": map[string]any{"attempt": 1, "maxAttempts": 3}})
	fetchStarted := time.Now()
	var result subscriptions.SourceResult
	var err error
	if input.Mode == "production" {
		result, err = admin.manager.TestSubscriptionSourceWithCache(request.Context(), source)
	} else {
		result, err = admin.manager.TestSubscriptionSource(request.Context(), source)
	}
	if err != nil {
		admin.sse(writer, "message", map[string]any{"type": "attempt-result", "data": map[string]any{"attempt": 1, "maxAttempts": 3, "success": false, "httpStatus": nil, "finalUrl": input.URL, "httpHeaders": map[string]string{}, "fetchDurationMs": time.Since(fetchStarted).Milliseconds(), "error": err.Error(), "requestError": map[string]any{"message": err.Error(), "debug": err.Error(), "chain": []string{err.Error()}, "isTimeout": false, "isConnect": true, "isRequest": true, "isBody": false, "isDecode": false, "status": nil, "url": input.URL}, "remoteAddress": nil, "httpVersion": nil, "tlsPeerCertificateBytes": nil, "payload": emptySourceDebugPayload()}})
		admin.sse(writer, "message", map[string]any{"type": "fallback", "data": map[string]any{"status": "miss", "payload": nil}})
		admin.sse(writer, "message", map[string]any{"type": "done", "data": map[string]any{"success": false, "resultSource": nil, "nodeCount": 0, "totalDurationMs": time.Since(started).Milliseconds()}})
		return
	}
	raw, err := admin.manager.subscriptions.ReadBlob(result.ContentHash)
	if err != nil {
		admin.sse(writer, "error", map[string]string{"message": err.Error()})
		return
	}
	payload := sourceDebugPayload(result, string(raw))
	admin.sse(writer, "message", map[string]any{"type": "attempt-result", "data": map[string]any{"attempt": 1, "maxAttempts": 3, "success": true, "httpStatus": 200, "finalUrl": input.URL, "httpHeaders": map[string]string{}, "fetchDurationMs": time.Since(fetchStarted).Milliseconds(), "error": nil, "requestError": nil, "remoteAddress": nil, "httpVersion": "HTTP", "tlsPeerCertificateBytes": nil, "payload": payload}})
	resultSource := "live"
	if result.FromCache {
		resultSource = "cache"
		if result.Source.LastStatus == "last-known-good cache" {
			resultSource = "stale-cache"
		}
	}
	admin.sse(writer, "message", map[string]any{"type": "done", "data": map[string]any{"success": true, "resultSource": resultSource, "nodeCount": len(result.Parse.Nodes), "totalDurationMs": time.Since(started).Milliseconds()}})
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
	started := time.Now()
	catalog, catalogErr := admin.manager.subscriptions.Read()
	if catalogErr != nil {
		admin.operationError(writer, catalogErr)
		return
	}
	profile, profileErr := subscriptions.FindProfile(&catalog, request.PathValue("id"))
	if profileErr != nil {
		admin.operationError(writer, profileErr)
		return
	}
	effective := subscriptions.EffectiveProfile(*profile)
	admin.beginSSE(writer)
	admin.sse(writer, "message", map[string]any{"type": "config", "data": profileDebugConfig(*profile, effective)})
	localNodes, localErr := subscriptions.PreviewLocalNodes(*profile, catalog)
	if localErr != nil {
		admin.sse(writer, "error", map[string]string{"message": localErr.Error()})
		return
	}
	admin.sse(writer, "message", map[string]any{"type": "manual-servers", "data": map[string]any{"count": len(localNodes), "nodes": localNodes}})
	for index, source := range profile.Sources {
		if source.Enabled {
			admin.sse(writer, "message", map[string]any{"type": "source-start", "data": map[string]any{"sourceIndex": index + 1, "url": source.URL}})
		}
	}
	result, err := admin.manager.RenderSubscriptionProfile(request.Context(), request.PathValue("id"), input.Format, true)
	if err != nil {
		admin.sse(writer, "error", map[string]string{"message": err.Error()})
		return
	}
	sourceIndexes := map[string]int{}
	for index, source := range profile.Sources {
		sourceIndexes[source.ID] = index + 1
	}
	totalFiltered := 0
	for _, source := range result.SourceResults {
		raw, readErr := admin.manager.subscriptions.ReadBlob(source.ContentHash)
		if readErr != nil {
			admin.sse(writer, "error", map[string]string{"message": readErr.Error()})
			return
		}
		activeNodes := []subscriptions.PreviewNode{}
		filteredNodes := []map[string]any{}
		before := []subscriptions.PreviewNode{}
		for _, proxy := range source.Parse.Nodes {
			node := previewFromProxy(proxy, sourceIndexes[source.Source.ID], source.Source.URL, effective.Filters)
			before = append(before, node)
			if node.Filtered {
				filteredNodes = append(filteredNodes, map[string]any{"node": node, "matchedRule": node.FilteredBy})
				totalFiltered++
			} else {
				activeNodes = append(activeNodes, node)
			}
		}
		admin.sse(writer, "message", map[string]any{"type": "source-result", "data": map[string]any{"sourceIndex": sourceIndexes[source.Source.ID], "url": source.Source.URL, "httpStatus": 200, "httpHeaders": map[string]string{}, "rawText": string(raw), "decodedText": nullableDecoded(source.Parse.DecodedText), "format": source.Parse.Format, "parsedNodeCount": len(source.Parse.Nodes), "nodesBeforeFilter": before, "nodesAfterFilter": activeNodes, "filteredNodes": filteredNodes, "error": nil, "fetchDurationMs": 0, "cached": source.FromCache}})
	}
	warningNodes, ignoredNodes := debugNodeWarnings(result.FieldDiffs)
	admin.sse(writer, "message", map[string]any{"type": "merge", "data": map[string]any{"totalNodesBeforeFilter": len(result.FieldDiffs) + totalFiltered, "totalNodesAfterFilter": len(result.FieldDiffs), "totalFiltered": totalFiltered, "finalNodeNames": fieldDiffNames(result.FieldDiffs), "nodeWarnings": warningNodes, "nodeIgnored": ignoredNodes}})
	admin.sse(writer, "message", map[string]any{"type": "output", "data": map[string]any{"proxyGroupCount": len(effective.Groups), "ruleCount": len(effective.Rules), "ruleProviderCount": len(effective.RuleProviders), "configOutput": result.Content}})
	admin.sse(writer, "message", map[string]any{"type": "rule-sets", "data": debugRuleSets(effective.RuleProviders)})
	admin.sse(writer, "message", map[string]any{"type": "validate", "data": map[string]any{"valid": result.RuntimeValidated, "warnings": result.Warnings, "errors": []string{}, "skipped": !result.RuntimeValidated, "reason": "preview does not stage the active runtime", "method": "Sempre compiler"}})
	admin.sse(writer, "message", map[string]any{"type": "done", "data": map[string]any{"totalDurationMs": time.Since(started).Milliseconds()}})
}

func (admin *adminServer) subscriptionDefaults(writer http.ResponseWriter, _ *http.Request) {
	profile := subscriptions.NewProfile("")
	apiWriteJSON(writer, http.StatusOK, map[string]any{"profile": profile, "defaults": subscriptions.SystemDefaults(), "editor_defaults": subscriptions.RecommendedEditorDefaults(), "targets": subscriptions.AvailableTargets(), "source_defaults": subscriptions.Source{Type: subscriptions.SourceURL, Enabled: true, UserAgent: subscriptions.DefaultUserAgent, FetchMode: subscriptions.FetchAuto}})
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
