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
