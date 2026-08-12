package subscription

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	singboxcore "github.com/tinymins/sempre/internal/core/singbox"
)

type Compiler struct {
	store                *Store
	fetcher              *Fetcher
	resolveTunnelForward TunnelForwardResolver
}

type TunnelForward struct {
	Host string
	Port int
}

type TunnelForwardResolver func(string) (TunnelForward, bool)

type Target struct {
	Core     string `json:"core,omitempty"`
	Format   string `json:"format"`
	Version  string `json:"version,omitempty"`
	Platform string `json:"platform,omitempty"`
}

func NewCompiler(store *Store, tunnelResolver ...TunnelForwardResolver) *Compiler {
	compiler := &Compiler{store: store, fetcher: NewFetcher(store)}
	if len(tunnelResolver) > 0 {
		compiler.resolveTunnelForward = tunnelResolver[0]
	}
	return compiler
}

func ResolveSingBoxTarget(coreVersion, platform string) (Target, []string) {
	version, warnings := singboxcore.ResolveCompilerVersion(coreVersion)
	platform = normalizePlatform(platform)
	format := "sing-box-v" + version
	if version == "11" {
		format = "sing-box"
	}
	if platform == "windows" {
		format += "-windows"
	}
	if platform == "macos" {
		format += "-macos"
	}
	return Target{Core: "sing-box", Format: format, Version: version, Platform: platform}, warnings
}

func ParseTarget(format string) (Target, error) {
	switch format {
	case "clash", "clash-meta":
		return Target{Format: format}, nil
	case "xray", "v2ray", "clash-rs", "dae":
		return Target{Core: format, Format: format, Platform: "default"}, nil
	}
	result := Target{Format: format, Version: "11", Platform: "default"}
	value := format
	if strings.HasSuffix(value, "-windows") {
		result.Platform = "windows"
		value = strings.TrimSuffix(value, "-windows")
	}
	if strings.HasSuffix(value, "-macos") {
		result.Platform = "macos"
		value = strings.TrimSuffix(value, "-macos")
	}
	switch value {
	case "sing-box":
		result.Version = "11"
	case "sing-box-v12":
		result.Version = "12"
	case "sing-box-v13":
		result.Version = "13"
	case "sing-box-v14":
		result.Version = "14"
	default:
		return Target{}, fmt.Errorf("unsupported output format %q", format)
	}
	return result, nil
}

func AvailableTargets() []Target {
	formats := []string{"clash", "clash-meta", "sing-box", "sing-box-windows", "sing-box-macos", "sing-box-v12", "sing-box-v12-windows", "sing-box-v12-macos", "sing-box-v13", "sing-box-v13-windows", "sing-box-v13-macos", "sing-box-v14", "sing-box-v14-windows", "sing-box-v14-macos", "xray", "v2ray", "clash-rs", "dae"}
	result := make([]Target, 0, len(formats))
	for _, format := range formats {
		target, _ := ParseTarget(format)
		result = append(result, target)
	}
	return result
}

func (compiler *Compiler) Render(ctx context.Context, profile Profile, catalog Catalog, target Target, force bool) (RenderResult, Profile, error) {
	parsedTarget, err := ParseTarget(target.Format)
	if err != nil {
		return RenderResult{}, profile, err
	}
	if target.Core != "" {
		parsedTarget.Core = target.Core
	}
	effective := EffectiveProfile(profile)
	nodes, sources, updatedEffective, warnings, origins, err := compiler.collectNodes(ctx, effective, catalog, force)
	if err != nil {
		return RenderResult{}, profile, err
	}
	if len(nodes) == 0 {
		return RenderResult{}, profile, fmt.Errorf("subscription profile produced no usable nodes")
	}
	result := RenderResult{Format: parsedTarget.Format, Version: parsedTarget.Version, Platform: parsedTarget.Platform, NodeCount: len(nodes), SourceResults: sources, FieldDiffs: []FieldDiff{}, NodeOrigins: origins, Warnings: warnings}
	if parsedTarget.Format == "clash" || parsedTarget.Format == "clash-meta" || parsedTarget.Format == "clash-rs" {
		represented := nodes
		unsupportedDiffs := []FieldDiff{}
		if parsedTarget.Core == "clash-rs" {
			represented = make([]Proxy, 0, len(nodes))
			for _, node := range nodes {
				if clashRSSupportsProxy(node.Type) {
					represented = append(represented, node)
					continue
				}
				warning := node.Name + ": unsupported proxy type " + node.Type
				result.Warnings = append(result.Warnings, warning)
				unsupportedDiffs = append(unsupportedDiffs, FieldDiff{Node: node.Name, Dropped: sortedKeys(node.Extra), Warnings: []string{warning}, FieldOrigins: map[string]FieldOrigin{}})
			}
			if len(represented) == 0 {
				return RenderResult{}, profile, fmt.Errorf("no nodes can be represented by clash-rs")
			}
		}
		content, err := buildClash(effective, represented, parsedTarget.Format != "clash", parsedTarget.Core)
		if err != nil {
			return RenderResult{}, profile, err
		}
		result.Content = content
		result.FieldDiffs = append(clashFieldDiffs(represented), unsupportedDiffs...)
		result.NodeCount = len(represented)
		profile.Sources = updatedEffective.Sources
		return result, profile, nil
	}
	if parsedTarget.Format == "dae" {
		content, diffs, buildWarnings, err := buildDae(effective, nodes)
		if err != nil {
			return RenderResult{}, profile, err
		}
		result.Content = content
		result.FieldDiffs = diffs
		result.NodeCount = representedNodeCount(diffs)
		result.Warnings = append(result.Warnings, buildWarnings...)
		profile.Sources = updatedEffective.Sources
		return result, profile, nil
	}
	if parsedTarget.Format == "xray" || parsedTarget.Format == "v2ray" {
		config, diffs, buildWarnings, err := buildV2RayFamily(effective, nodes, parsedTarget.Format)
		if err != nil {
			return RenderResult{}, profile, err
		}
		result.FieldDiffs = diffs
		result.NodeCount = representedNodeCount(diffs)
		result.Warnings = append(result.Warnings, buildWarnings...)
		encoded, err := json.MarshalIndent(config, "", "  ")
		if err != nil {
			return RenderResult{}, profile, err
		}
		result.Content = string(append(encoded, '\n'))
		profile.Sources = updatedEffective.Sources
		return result, profile, nil
	}
	config, diffs, buildWarnings, err := compiler.buildSingBox(ctx, effective, nodes, parsedTarget, force)
	if err != nil {
		return RenderResult{}, profile, err
	}
	result.FieldDiffs = diffs
	result.NodeCount = 0
	for _, diff := range diffs {
		if diff.Outbound != nil {
			result.NodeCount++
		}
	}
	result.Warnings = append(result.Warnings, buildWarnings...)
	encoded, err := json.MarshalIndent(config, "", "  ")
	if err != nil {
		return RenderResult{}, profile, err
	}
	result.Content = string(append(encoded, '\n'))
	profile.Sources = updatedEffective.Sources
	return result, profile, nil
}

func clashRSSupportsProxy(proxyType string) bool {
	switch proxyType {
	case "ss", "socks5", "anytls", "trojan", "vmess", "vless", "tuic", "hysteria2":
		return true
	default:
		return false
	}
}

func representedNodeCount(diffs []FieldDiff) int {
	count := 0
	for _, diff := range diffs {
		if diff.Outbound != nil {
			count++
		}
	}
	return count
}

func (compiler *Compiler) collectNodes(ctx context.Context, profile Profile, catalog Catalog, force bool) ([]Proxy, []SourceResult, Profile, []string, map[string]string, error) {
	nodes, err := ManualServers(profile)
	if err != nil {
		return nil, nil, profile, nil, nil, err
	}
	nodeOrigins := make([]string, len(nodes))
	for index := range nodeOrigins {
		nodeOrigins[index] = fmt.Sprintf("manual-server:%d", index+1)
	}
	results := []SourceResult{}
	warnings := []string{}
	updated := profile
	for index, source := range profile.Sources {
		if !source.Enabled {
			continue
		}
		data, fetched, fromCache, err := compiler.fetcher.LoadValidated(ctx, source, force, validateSubscriptionContent)
		if err != nil {
			return nil, nil, profile, warnings, nil, fmt.Errorf("source %q: %w", sourceLabel(source), err)
		}
		parsed := Parse(string(data))
		if len(parsed.Nodes) == 0 {
			return nil, nil, profile, warnings, nil, fmt.Errorf("source %q produced no usable nodes: %s", sourceLabel(source), strings.Join(parsed.Diagnostics, "; "))
		}
		for _, diagnostic := range parsed.Diagnostics {
			warnings = append(warnings, sourceLabel(source)+": "+diagnostic)
		}
		for nodeIndex := range parsed.Nodes {
			if fetched.Prefix != "" {
				parsed.Nodes[nodeIndex].Name = normalizePrefix(fetched.Prefix) + parsed.Nodes[nodeIndex].Name
			}
			nodes = append(nodes, parsed.Nodes[nodeIndex])
			nodeOrigins = append(nodeOrigins, fmt.Sprintf("source:%s:%s", fetched.ID, sourceLabel(fetched)))
		}
		updated.Sources[index] = fetched
		results = append(results, SourceResult{Source: redactSource(fetched), Parse: parsed, FromCache: fromCache, ContentHash: fetched.SnapshotHash, Bytes: len(data)})
	}
	selected := map[string]bool{}
	for _, id := range profile.CustomNodeIDs {
		selected[id] = true
	}
	for _, node := range catalog.CustomNodes {
		if selected[node.ID] {
			proxy, err := ProxyFromMap(node.Proxy)
			if err != nil {
				return nil, nil, profile, warnings, nil, fmt.Errorf("custom node %q: %w", node.Name, err)
			}
			nodes = append(nodes, proxy)
			nodeOrigins = append(nodeOrigins, "custom-node:"+node.ID+":"+node.Name)
		}
	}
	filtered := nodes[:0]
	filteredOrigins := nodeOrigins[:0]
	for index, node := range nodes {
		excluded := false
		if strings.HasPrefix(nodeOrigins[index], "source:") {
			for _, filter := range profile.Filters {
				if filter != "" && strings.Contains(node.Name, filter) {
					excluded = true
					break
				}
			}
		}
		if !excluded {
			filtered = append(filtered, node)
			filteredOrigins = append(filteredOrigins, nodeOrigins[index])
		}
	}
	for index := range filtered {
		filtered[index].Name = appendIcon(filtered[index].Name)
	}
	nodes, origins := uniqueNodeNames(filtered, filteredOrigins)
	if len(nodes) == 0 {
		return nil, nil, profile, warnings, nil, fmt.Errorf("all nodes were removed by filters")
	}
	return nodes, results, updated, warnings, origins, nil
}
