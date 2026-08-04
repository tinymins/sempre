package app

import (
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func (admin *adminServer) subscriptionsGet(writer http.ResponseWriter, _ *http.Request) {
	catalog, active, schedule, autoRestart, err := admin.manager.SubscriptionCatalog()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"profiles": catalog.Profiles, "active_profile_id": active, "schedule": schedule, "auto_restart": autoRestart, "targets": subscriptions.AvailableTargets(), "defaults": subscriptions.SystemDefaults(), "editor_defaults": subscriptions.SystemEditorDefaults()})
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
	manual, manualErr := subscriptions.ManualServers(*profile)
	if manualErr != nil {
		admin.sse(writer, "error", map[string]string{"message": manualErr.Error()})
		return
	}
	manualNodes := make([]subscriptions.PreviewNode, 0, len(manual))
	for _, proxy := range manual {
		manualNodes = append(manualNodes, previewFromProxy(proxy, 0, "manual", nil))
	}
	admin.sse(writer, "message", map[string]any{"type": "manual-servers", "data": map[string]any{"count": len(manualNodes), "nodes": manualNodes}})
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
	apiWriteJSON(writer, http.StatusOK, map[string]any{"profile": profile, "defaults": subscriptions.SystemDefaults(), "editor_defaults": subscriptions.SystemEditorDefaults(), "targets": subscriptions.AvailableTargets(), "source_defaults": subscriptions.Source{Type: subscriptions.SourceURL, Enabled: true, UserAgent: subscriptions.DefaultUserAgent, FetchMode: subscriptions.FetchAuto}})
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

func sourceNetworkStep(rawURL, fetchMode string) map[string]any {
	parsed, err := url.Parse(rawURL)
	host := ""
	port := 0
	scheme := ""
	if err == nil {
		host = parsed.Hostname()
		scheme = parsed.Scheme
		if parsed.Port() != "" {
			fmt.Sscanf(parsed.Port(), "%d", &port)
		} else if scheme == "https" {
			port = 443
		} else {
			port = 80
		}
	}
	dnsStarted := time.Now()
	addresses := []string{}
	dnsError := any(nil)
	if host != "" {
		resolved, lookupErr := net.LookupIP(host)
		if lookupErr != nil {
			dnsError = lookupErr.Error()
		} else {
			for _, address := range resolved {
				addresses = append(addresses, address.String())
			}
		}
	}
	probes := []map[string]any{}
	for _, address := range addresses {
		probeStarted := time.Now()
		connection, dialErr := net.DialTimeout("tcp", net.JoinHostPort(address, fmt.Sprintf("%d", port)), 2*time.Second)
		probe := map[string]any{"address": address, "success": dialErr == nil, "durationMs": time.Since(probeStarted).Milliseconds(), "localAddress": nil, "remoteAddress": nil, "error": nil}
		if dialErr != nil {
			probe["error"] = dialErr.Error()
		} else {
			probe["localAddress"] = connection.LocalAddr().String()
			probe["remoteAddress"] = connection.RemoteAddr().String()
			connection.Close()
		}
		probes = append(probes, probe)
		if len(probes) == 3 {
			break
		}
	}
	return map[string]any{"type": "network", "data": map[string]any{"fetchMode": fetchMode, "connectionKind": "origin", "proxyEndpoint": nil, "scheme": nullableStringValue(scheme), "host": nullableStringValue(host), "port": port, "resolverConfig": []string{}, "proxyEnvironmentVariables": []string{}, "dnsDurationMs": time.Since(dnsStarted).Milliseconds(), "resolvedAddresses": addresses, "dnsError": dnsError, "tcpProbes": probes}}
}

func sourceDebugPayload(result subscriptions.SourceResult, raw string) map[string]any {
	nodes := make([]subscriptions.PreviewNode, 0, len(result.Parse.Nodes))
	for _, proxy := range result.Parse.Nodes {
		nodes = append(nodes, previewFromProxy(proxy, 1, result.Source.URL, nil))
	}
	discarded := make([]subscriptions.PreviewNode, 0, len(result.Parse.DiscardedPlaceholders))
	for _, proxy := range result.Parse.DiscardedPlaceholders {
		discarded = append(discarded, previewFromProxy(proxy, 1, result.Source.URL, nil))
	}
	return map[string]any{"format": result.Parse.Format, "rawText": raw, "decodedText": nullableDecoded(result.Parse.DecodedText), "bodyBytes": len(raw), "parsedNodeCount": len(nodes), "nodes": nodes, "discardedPlaceholderNodes": discarded, "diagnostics": result.Parse.Diagnostics}
}

func emptySourceDebugPayload() map[string]any {
	return map[string]any{"format": "unknown", "rawText": "", "decodedText": nil, "bodyBytes": 0, "parsedNodeCount": 0, "nodes": []any{}, "discardedPlaceholderNodes": []any{}, "diagnostics": []string{}}
}

func nullableDecoded(value string) any {
	if value == "" {
		return nil
	}
	return value
}

func nullableStringValue(value string) any {
	if value == "" {
		return nil
	}
	return value
}

func previewFromProxy(proxy subscriptions.Proxy, sourceIndex int, sourceURL string, filters []string) subscriptions.PreviewNode {
	proxy.Name = subscriptions.EnrichNodeName(proxy.Name)
	result := subscriptions.PreviewNode{Name: proxy.Name, Type: proxy.Type, Server: proxy.Server, Port: proxy.Port, SourceIndex: sourceIndex, SourceURL: sourceURL, Raw: proxy.Map()}
	for _, filter := range filters {
		if filter != "" && strings.Contains(proxy.Name, filter) {
			result.Filtered = true
			result.FilteredBy = filter
			break
		}
	}
	return result
}

func profileDebugConfig(profile, effective subscriptions.Profile) map[string]any {
	providers := map[string][]map[string]any{}
	for _, provider := range effective.RuleProviders {
		providers[provider.Outbound] = append(providers[provider.Outbound], map[string]any{"name": provider.Tag, "url": provider.URL, "type": provider.Behavior})
	}
	urls := []string{}
	for _, source := range profile.Sources {
		if source.Enabled && source.Type == subscriptions.SourceURL {
			urls = append(urls, source.URL)
		}
	}
	manual, _ := subscriptions.ManualServers(profile)
	servers := make([]map[string]any, 0, len(manual))
	for _, proxy := range manual {
		servers = append(servers, proxy.Map())
	}
	shared := map[string]any{}
	overrides := map[string]any{}
	if value, ok := effective.DNS["shared"].(map[string]any); ok {
		shared = value
	}
	if value, ok := effective.DNS["overrides"].(map[string]any); ok {
		overrides = value
	}
	var private any
	if len(effective.PrivateAccess) > 0 {
		private = effective.PrivateAccess
	}
	groups := make([]map[string]any, 0, len(effective.Groups))
	for _, group := range effective.Groups {
		proxies := group.Proxies
		if proxies == nil {
			proxies = []string{}
		}
		groups = append(groups, map[string]any{"name": group.Name, "type": group.Type, "proxies": proxies, "readonly": group.Readonly})
	}
	return map[string]any{"subscribeUrls": urls, "filters": effective.Filters, "groups": groups, "ruleProviders": providers, "customConfig": effective.Rules, "servers": servers, "privateAccessConfig": private, "dnsConfig": map[string]any{"shared": shared, "overrides": overrides}}
}

func fieldDiffNames(diffs []subscriptions.FieldDiff) []string {
	result := make([]string, 0, len(diffs))
	for _, diff := range diffs {
		if diff.Outbound != nil || len(diff.Dropped) == 0 {
			result = append(result, diff.Node)
		}
	}
	return result
}

func debugNodeWarnings(diffs []subscriptions.FieldDiff) ([]string, []string) {
	warnings := []string{}
	ignored := []string{}
	for _, diff := range diffs {
		if len(diff.Dropped) > 0 || len(diff.Warnings) > 0 {
			warnings = append(warnings, diff.Node)
		}
		if len(diff.Ignored) > 0 {
			ignored = append(ignored, diff.Node)
		}
	}
	return warnings, ignored
}

func debugRuleSets(providers []subscriptions.RuleProvider) map[string]any {
	items := make([]map[string]any, 0, len(providers))
	for _, provider := range providers {
		items = append(items, map[string]any{"tag": provider.Tag, "url": provider.URL, "effectiveUrl": provider.URL, "group": provider.Outbound, "status": "ok", "ruleCount": 0, "sampleRules": []string{}, "builtin": false, "format": provider.Format})
	}
	return map[string]any{"totalCount": len(items), "totalRules": 0, "errorCount": 0, "items": items}
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
