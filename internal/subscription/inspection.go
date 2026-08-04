package subscription

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
)

type PreviewNode struct {
	Name         string         `json:"name"`
	Type         string         `json:"type"`
	Server       string         `json:"server"`
	Port         int            `json:"port"`
	SourceIndex  int            `json:"sourceIndex"`
	SourceURL    string         `json:"sourceUrl"`
	Raw          map[string]any `json:"raw"`
	Filtered     bool           `json:"filtered,omitempty"`
	FilteredBy   string         `json:"filteredBy,omitempty"`
	originalName string
}

func (compiler *Compiler) PreviewNodes(ctx context.Context, profile Profile, catalog Catalog, force bool) ([]PreviewNode, error) {
	effective := EffectiveProfile(profile)
	nodes := []PreviewNode{}
	manual, err := ManualServers(effective)
	if err != nil {
		return nil, err
	}
	for _, proxy := range manual {
		nodes = append(nodes, previewNode(proxy, proxy.Name, 0, "manual", nil))
	}
	selected := map[string]bool{}
	for _, id := range profile.CustomNodeIDs {
		selected[id] = true
	}
	for _, node := range catalog.CustomNodes {
		if !selected[node.ID] {
			continue
		}
		proxy, parseErr := ProxyFromMap(node.Proxy)
		if parseErr != nil {
			return nil, fmt.Errorf("custom node %q: %w", node.Name, parseErr)
		}
		nodes = append(nodes, previewNode(proxy, proxy.Name, 0, "custom-node:"+node.ID, nil))
	}
	for index, source := range effective.Sources {
		if !source.Enabled {
			continue
		}
		data, fetched, _, fetchErr := compiler.fetcher.LoadValidated(ctx, source, force, validateSubscriptionContent)
		if fetchErr != nil {
			return nil, fmt.Errorf("source %q: %w", sourceLabel(source), fetchErr)
		}
		parsed := Parse(string(data))
		if len(parsed.Nodes) == 0 {
			return nil, fmt.Errorf("source %q produced no usable nodes: %s", sourceLabel(source), strings.Join(parsed.Diagnostics, "; "))
		}
		prefix := normalizePrefix(fetched.Prefix)
		for _, proxy := range parsed.Nodes {
			originalName := prefix + proxy.Name
			proxy.Name = originalName
			nodes = append(nodes, previewNode(proxy, originalName, index+1, fetched.URL, effective.Filters))
		}
	}
	return nodes, nil
}

func previewNode(proxy Proxy, originalName string, sourceIndex int, sourceURL string, filters []string) PreviewNode {
	proxy.Name = appendIcon(proxy.Name)
	result := PreviewNode{Name: proxy.Name, Type: proxy.Type, Server: proxy.Server, Port: proxy.Port, SourceIndex: sourceIndex, SourceURL: sourceURL, Raw: proxy.Map(), originalName: originalName}
	for _, filter := range filters {
		if filter != "" && strings.Contains(proxy.Name, filter) {
			result.Filtered = true
			result.FilteredBy = filter
			break
		}
	}
	return result
}

func (compiler *Compiler) TraceNode(ctx context.Context, profile Profile, catalog Catalog, name, format string) (map[string]any, error) {
	nodes, err := compiler.PreviewNodes(ctx, profile, catalog, true)
	if err != nil {
		return nil, err
	}
	var selected *PreviewNode
	position := 0
	activePosition := 0
	for index := range nodes {
		if !nodes[index].Filtered {
			activePosition++
		}
		if nodes[index].Name == name {
			selected = &nodes[index]
			position = activePosition
			break
		}
	}
	if selected == nil {
		return nil, fmt.Errorf("node %q was not found", name)
	}
	effective := EffectiveProfile(profile)
	originalRaw := cloneMap(selected.Raw)
	originalRaw["name"] = selected.originalName
	steps := []any{
		map[string]any{"type": "source", "data": map[string]any{"sourceIndex": selected.SourceIndex, "sourceUrl": selected.SourceURL, "format": sourceFormat(selected.SourceIndex, selected.SourceURL), "rawData": originalRaw}},
		map[string]any{"type": "parse", "data": map[string]any{"clashProxy": originalRaw}},
		map[string]any{"type": "filter", "data": map[string]any{"passed": !selected.Filtered, "matchedRule": nullableString(selected.FilteredBy), "filtersApplied": effective.Filters}},
		map[string]any{"type": "enrich", "data": map[string]any{"originalName": selected.originalName, "enrichedName": selected.Name}},
	}
	if !selected.Filtered {
		steps = append(steps, map[string]any{"type": "merge", "data": map[string]any{"positionInFinalList": position, "totalNodes": activeNodeCount(nodes)}})
		groups := []map[string]string{}
		for _, group := range effective.Groups {
			if !group.Readonly || containsString(group.Proxies, selected.Name) {
				groups = append(groups, map[string]string{"name": group.Name, "type": group.Type})
			}
		}
		steps = append(steps, map[string]any{"type": "group-assign", "data": map[string]any{"assignedGroups": groups}})
		if strings.HasPrefix(format, "sing-box") {
			proxy, parseErr := ProxyFromMap(selected.Raw)
			if parseErr != nil {
				return nil, parseErr
			}
			outbound, diff, ok := ConvertProxy(proxy)
			if ok {
				steps = append(steps, map[string]any{"type": "convert", "data": map[string]any{"singboxOutbound": outbound, "lostFields": diff.Dropped, "ignoredFields": diff.Ignored, "fieldOrigins": camelFieldOrigins(buildFieldOrigins(proxy, outbound))}})
			}
		}
		fragment, _ := json.MarshalIndent(selected.Raw, "", "  ")
		steps = append(steps, map[string]any{"type": "output", "data": map[string]any{"configFragment": string(fragment)}})
	}
	return map[string]any{"nodeName": selected.Name, "steps": steps}, nil
}

func sourceFormat(index int, sourceURL string) string {
	if index == 0 || strings.HasPrefix(sourceURL, "manual") || strings.HasPrefix(sourceURL, "custom-node:") {
		return "manual"
	}
	return "yaml"
}

func nullableString(value string) any {
	if value == "" {
		return nil
	}
	return value
}

func activeNodeCount(nodes []PreviewNode) int {
	count := 0
	for _, node := range nodes {
		if !node.Filtered {
			count++
		}
	}
	return count
}

func containsString(values []string, expected string) bool {
	for _, value := range values {
		if value == expected {
			return true
		}
	}
	return false
}

func camelFieldOrigins(origins map[string]FieldOrigin) map[string]any {
	result := map[string]any{}
	for path, origin := range origins {
		result[path] = map[string]any{"sourceKey": nullableString(origin.SourceKey), "sourceValue": origin.SourceValue, "step": origin.Step, "transform": origin.Transform, "reason": origin.Reason, "sources": origin.Sources}
	}
	return result
}
