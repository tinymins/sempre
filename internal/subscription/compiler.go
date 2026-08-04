package subscription

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"sort"
	"strconv"
	"strings"
	"sync"

	"gopkg.in/yaml.v3"
)

type Compiler struct {
	store   *Store
	fetcher *Fetcher
}

type Target struct {
	Format   string `json:"format"`
	Version  string `json:"version,omitempty"`
	Platform string `json:"platform,omitempty"`
}

func NewCompiler(store *Store) *Compiler {
	return &Compiler{store: store, fetcher: NewFetcher(store)}
}

func ResolveSingBoxTarget(coreVersion, platform string) (Target, []string) {
	warnings := []string{}
	version := "13"
	parts := strings.Split(strings.TrimPrefix(coreVersion, "v"), ".")
	if len(parts) >= 2 {
		major, majorErr := strconv.Atoi(parts[0])
		minor, minorErr := strconv.Atoi(parts[1])
		if majorErr == nil && minorErr == nil && major == 1 {
			switch {
			case minor < 11:
				version = "11"
				warnings = append(warnings, "installed sing-box is older than the minimum compiler target; using v11")
			case minor == 11:
				version = "11"
			case minor == 12:
				version = "12"
			default:
				version = "13"
				if minor > 13 {
					warnings = append(warnings, "no exact compiler for this sing-box minor version; using the newest compatible v13 compiler")
				}
			}
		} else {
			warnings = append(warnings, "unknown sing-box major version; using the default v13 compiler")
		}
	} else {
		warnings = append(warnings, "unrecognized sing-box version; using the default v13 compiler")
	}
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
	return Target{Format: format, Version: version, Platform: platform}, warnings
}

func ParseTarget(format string) (Target, error) {
	switch format {
	case "clash", "clash-meta":
		return Target{Format: format}, nil
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
	default:
		return Target{}, fmt.Errorf("unsupported output format %q", format)
	}
	return result, nil
}

func AvailableTargets() []Target {
	formats := []string{"clash", "clash-meta", "sing-box", "sing-box-windows", "sing-box-macos", "sing-box-v12", "sing-box-v12-windows", "sing-box-v12-macos", "sing-box-v13", "sing-box-v13-windows", "sing-box-v13-macos"}
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
	effective := EffectiveProfile(profile)
	nodes, sources, updatedEffective, warnings, origins, err := compiler.collectNodes(ctx, effective, catalog, force)
	if err != nil {
		return RenderResult{}, profile, err
	}
	if len(nodes) == 0 {
		return RenderResult{}, profile, fmt.Errorf("subscription profile produced no usable nodes")
	}
	result := RenderResult{Format: parsedTarget.Format, Version: parsedTarget.Version, Platform: parsedTarget.Platform, NodeCount: len(nodes), SourceResults: sources, FieldDiffs: []FieldDiff{}, NodeOrigins: origins, Warnings: warnings}
	if parsedTarget.Format == "clash" || parsedTarget.Format == "clash-meta" {
		content, err := buildClash(effective, nodes, parsedTarget.Format == "clash-meta")
		if err != nil {
			return RenderResult{}, profile, err
		}
		result.Content = content
		result.FieldDiffs = clashFieldDiffs(nodes)
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

func buildClash(profile Profile, proxies []Proxy, meta bool) (string, error) {
	proxyMaps := make([]map[string]any, 0, len(proxies))
	names := make([]string, 0, len(proxies))
	for _, proxy := range proxies {
		proxyMaps = append(proxyMaps, proxy.Map())
		names = append(names, proxy.Name)
	}
	groups := clashGroups(profile.Groups, names)
	rules := append([]string{}, profile.Rules...)
	providers := map[string]any{}
	for _, provider := range profile.RuleProviders {
		behavior := valueOr(provider.Behavior, "classical")
		providers[provider.Tag] = map[string]any{
			"type": "http", "behavior": behavior, "url": provider.URL,
			"path": "./rules/" + provider.Tag, "interval": 86400,
		}
		rules = append(rules, "RULE-SET,"+provider.Tag+","+valueOr(provider.Outbound, groups[0]["name"].(string)))
	}
	rules = append(rules, "DOMAIN-SUFFIX,local,DIRECT", "GEOIP,LAN,DIRECT,no-resolve", "GEOIP,CN,DIRECT,no-resolve", "MATCH,"+clashFinalGroup(profile.Groups))
	shared := resolveDNSShared(profile.DNS)
	config := map[string]any{
		"tproxy-port": shared.TProxyPort, "allow-lan": true, "mode": "Rule", "log-level": clashLogLevel(profile.LogLevel),
		"secret": shared.ClashAPISecret, "proxies": proxyMaps, "proxy-groups": groups,
		"rule-providers": providers, "rules": rules,
		"profile": map[string]any{"store-selected": true, "store-fake-ip": true, "tracing": true},
	}
	if meta {
		config["unified-delay"] = true
		config["tcp-concurrent"] = true
		config["find-process-mode"] = "strict"
		config["global-client-fingerprint"] = "chrome"
		config["geodata-mode"] = true
		config["geo-auto-update"] = true
		config["geo-update-interval"] = 24
		config["sniffer"] = map[string]any{
			"enable": true, "force-dns-mapping": true, "parse-pure-ip": true, "override-destination": true,
			"sniff": map[string]any{
				"HTTP": map[string]any{"ports": []any{80, "8080-8880"}, "override-destination": true},
				"TLS":  map[string]any{"ports": []any{443, 8443}},
				"QUIC": map[string]any{"ports": []any{443, 8443}},
			},
		}
	}
	if override := clashDNSOverride(profile.DNS, meta); override != nil {
		config["dns"] = override
	}
	encoded, err := yaml.Marshal(config)
	if err != nil {
		return "", err
	}
	return string(encoded), nil
}

func clashLogLevel(level string) string {
	switch level {
	case "off":
		return "silent"
	case "warn":
		return "warning"
	case "error", "info", "debug":
		return level
	default:
		return "info"
	}
}

func singBoxLog(level string) map[string]any {
	disabled := level == "off"
	if disabled || (level != "error" && level != "warn" && level != "info" && level != "debug") {
		level = "info"
	}
	return map[string]any{"disabled": disabled, "level": level, "timestamp": true}
}

func clashGroups(configured []ProxyGroup, names []string) []map[string]any {
	if len(configured) == 0 {
		return []map[string]any{{"name": "proxy", "type": "select", "proxies": append([]string{"DIRECT"}, names...)}}
	}
	result := []map[string]any{}
	for _, group := range configured {
		proxies := append([]string{}, group.Proxies...)
		if !group.Readonly {
			proxies = appendUnique(proxies, names...)
		}
		if len(proxies) == 0 {
			proxies = append(proxies, names...)
		}
		item := map[string]any{"name": group.Name, "type": group.Type, "proxies": proxies}
		if group.URL != "" {
			item["url"] = group.URL
		}
		if group.Interval > 0 {
			item["interval"] = group.Interval
		}
		if group.Tolerance > 0 {
			item["tolerance"] = group.Tolerance
		}
		result = append(result, item)
	}
	return result
}

func clashFinalGroup(groups []ProxyGroup) string {
	for _, group := range groups {
		if group.Name == "⚓️ 其他流量" {
			return group.Name
		}
	}
	if len(groups) > 0 {
		return groups[0].Name
	}
	return "proxy"
}

func clashDNSOverride(config map[string]any, meta bool) map[string]any {
	key := "clash"
	if meta {
		key = "clashMeta"
	}
	overrides, _ := objectValue(config["overrides"])
	if result, ok := objectValue(overrides[key]); ok {
		return result
	}
	if meta {
		if result, ok := objectValue(overrides["clash"]); ok {
			return result
		}
	}
	if result, ok := objectValue(config[key]); ok {
		return result
	}
	return nil
}

func (compiler *Compiler) buildSingBox(ctx context.Context, profile Profile, proxies []Proxy, target Target, force bool) (map[string]any, []FieldDiff, []string, error) {
	modern := target.Version != "11"
	private := resolvePrivateAccess(profile.PrivateAccess, modern, target.Platform != "default")
	shared := resolveDNSShared(profile.DNS)
	outbounds := []any{map[string]any{"type": "direct", "tag": "direct"}, map[string]any{"type": "block", "tag": "reject"}}
	if !modern {
		outbounds = append(outbounds, map[string]any{"type": "dns", "tag": "dns-out"})
	}
	names := make([]string, 0, len(proxies))
	diffs := []FieldDiff{}
	warnings := []string{}
	for _, proxy := range proxies {
		if target.Version == "11" && proxy.Type == "anytls" {
			diffs = append(diffs, FieldDiff{Node: proxy.Name, Consumed: []string{}, Ignored: []string{}, Dropped: sortedKeys(proxy.Extra), Warnings: []string{"anytls requires sing-box v1.12 or newer"}, FieldOrigins: map[string]FieldOrigin{}})
			warnings = append(warnings, proxy.Name+": anytls requires sing-box v1.12 or newer")
			continue
		}
		outbound, diff, ok := ConvertProxy(proxy)
		diffs = append(diffs, diff)
		if !ok {
			warnings = append(warnings, proxy.Name+": unsupported proxy type "+proxy.Type)
			continue
		}
		if target.Platform != "default" && modern && net.ParseIP(proxy.Server) == nil {
			outbound["domain_resolver"] = map[string]any{"server": "bootstrap", "strategy": "ipv4_only"}
		}
		diff.Outbound = outbound
		diff.FieldOrigins = buildFieldOrigins(proxy, outbound)
		outbounds = append(outbounds, outbound)
		names = append(names, proxy.Name)
	}
	if len(names) == 0 {
		return nil, diffs, warnings, fmt.Errorf("no nodes can be represented by sing-box")
	}
	outbounds = append(outbounds, private.Outbounds...)
	groups := profile.Groups
	if len(groups) == 0 {
		groups = []ProxyGroup{{Name: "proxy", Type: "select", IncludeAll: true}}
	}
	for _, group := range groups {
		if strings.TrimSpace(group.Name) == "" {
			return nil, diffs, warnings, fmt.Errorf("proxy group name is required")
		}
		members := append([]string{}, group.Proxies...)
		if !group.Readonly || group.IncludeAll || len(members) == 0 {
			members = appendUnique(members, names...)
		}
		members = normalizeOutboundNames(members)
		if builtinOutboundName(group.Name) {
			continue
		}
		if len(members) == 0 {
			return nil, diffs, warnings, fmt.Errorf("proxy group %q has no members", group.Name)
		}
		kind := "selector"
		if group.Type == "url-test" {
			kind = "urltest"
		}
		outbound := map[string]any{"type": kind, "tag": group.Name, "outbounds": members}
		if kind == "selector" {
			outbound["default"] = members[0]
			outbound["interrupt_exist_connections"] = true
		} else {
			outbound["url"] = valueOr(group.URL, "https://www.gstatic.com/generate_204")
			if group.Interval > 0 {
				outbound["interval"] = fmt.Sprintf("%ds", group.Interval)
			}
			if group.Tolerance > 0 {
				outbound["tolerance"] = group.Tolerance
			}
		}
		outbounds = append(outbounds, outbound)
	}
	final := normalizeOutboundName(clashFinalGroup(groups))
	routeRules := []any{}
	if modern {
		routeRules = append(routeRules, map[string]any{"action": "sniff"}, map[string]any{"protocol": "dns", "action": "hijack-dns"})
	} else {
		routeRules = append(routeRules, map[string]any{"protocol": "dns", "outbound": "dns-out"})
	}
	if len(private.DirectDomains) > 0 {
		routeRules = append(routeRules, map[string]any{"domain": private.DirectDomains, "action": "route", "outbound": "direct"})
	}
	routeRules = append(routeRules, private.RouteRules...)
	routeRules = append(routeRules, map[string]any{"ip_is_private": true, "outbound": "direct"})
	for _, line := range profile.Rules {
		if rule, ok := convertRule(line); ok {
			routeRules = append(routeRules, rule)
		} else {
			warnings = append(warnings, "unsupported rule: "+line)
		}
	}
	ruleSets := []any{
		officialRuleSet("geoip-cn", "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-cn.srs"),
		officialRuleSet("geoip-hk", "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-hk.srs"),
		officialRuleSet("geosite-openai", "https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-openai.srs"),
		officialRuleSet("geosite-cn", "https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs"),
	}
	routeRules = append(routeRules, map[string]any{"rule_set": []string{"geoip-cn", "geosite-cn"}, "outbound": "direct"})
	providerSets, providerRoutes, providerWarnings, err := compiler.loadRuleProviders(ctx, profile.RuleProviders, final, force, profile.UseSystemRules)
	if err != nil {
		return nil, diffs, warnings, err
	}
	ruleSets = append(ruleSets, providerSets...)
	routeRules = append(routeRules, providerRoutes...)
	warnings = append(warnings, providerWarnings...)
	inbounds := singBoxInbounds(target, modern, shared)
	dns := singBoxDNS(profile.DNS, modern, target.Platform != "default", shared, foreignOutbound(groups, final))
	if servers, ok := dns["servers"].([]any); ok {
		dns["servers"] = append(servers, private.DNSServers...)
	}
	if rules, ok := dns["rules"].([]any); ok {
		privateRules := append([]any{}, private.DNSRules...)
		if len(private.DirectDomains) > 0 {
			resolver := "local"
			if target.Platform != "default" {
				resolver = "bootstrap"
			}
			privateRules = append([]any{map[string]any{"domain": private.DirectDomains, "action": "route", "server": resolver}}, privateRules...)
		}
		dns["rules"] = append(privateRules, rules...)
	}
	route := map[string]any{"rules": routeRules, "rule_set": ruleSets, "final": final}
	if modern {
		if target.Platform == "default" {
			route["default_domain_resolver"] = "local"
		} else {
			route["default_domain_resolver"] = "bootstrap"
		}
	}
	if target.Platform != "default" {
		route["auto_detect_interface"] = true
	}
	config := map[string]any{"log": singBoxLog(profile.LogLevel), "dns": dns, "inbounds": inbounds, "outbounds": outbounds, "route": route, "experimental": map[string]any{"cache_file": map[string]any{"enabled": true, "store_fakeip": shared.FakeIPEnabled && target.Platform == "default", "store_rdrc": false}, "clash_api": map[string]any{"external_controller": fmt.Sprintf("127.0.0.1:%d", shared.ClashAPIPort), "external_ui": shared.ClashAPIUIPath, "secret": shared.ClashAPISecret, "default_mode": "rule"}}}
	if len(private.Endpoints) > 0 {
		config["endpoints"] = private.Endpoints
	}
	deepMerge(config, profile.CustomConfig)
	return config, diffs, warnings, nil
}

type ruleProviderLoad struct {
	rules       []any
	diagnostics []string
	err         error
}

func (compiler *Compiler) loadRuleProviders(
	ctx context.Context,
	providers []RuleProvider,
	fallback string,
	force bool,
	allowFailures bool,
) ([]any, []any, []string, error) {
	loads := make([]ruleProviderLoad, len(providers))
	jobs := make(chan int)
	workers := 6
	if len(providers) < workers {
		workers = len(providers)
	}
	var group sync.WaitGroup
	for range workers {
		group.Add(1)
		go func() {
			defer group.Done()
			for index := range jobs {
				provider := providers[index]
				data, _, _, err := compiler.fetcher.LoadValidated(
					ctx,
					Source{ID: provider.Tag, Type: SourceURL, Enabled: true, URL: provider.URL, UserAgent: DefaultUserAgent, FetchMode: FetchAuto},
					force,
					validateRuleProviderContent,
				)
				if err != nil {
					loads[index].err = fmt.Errorf("rule provider %q: %w", provider.Tag, err)
					continue
				}
				loads[index].rules, loads[index].diagnostics, loads[index].err = parseRuleProvider(data)
			}
		}()
	}
	for index := range providers {
		jobs <- index
	}
	close(jobs)
	group.Wait()

	ruleSets := []any{}
	routes := []any{}
	warnings := []string{}
	for index, provider := range providers {
		loaded := loads[index]
		if loaded.err != nil {
			if !allowFailures {
				return nil, nil, warnings, loaded.err
			}
			warnings = append(warnings, loaded.err.Error())
			continue
		}
		for _, diagnostic := range loaded.diagnostics {
			warnings = append(warnings, provider.Tag+": "+diagnostic)
		}
		ruleSets = append(ruleSets, map[string]any{"type": "inline", "tag": provider.Tag, "rules": loaded.rules})
		routes = append(routes, map[string]any{"rule_set": []string{provider.Tag}, "outbound": normalizeOutboundName(valueOr(provider.Outbound, fallback))})
	}
	return ruleSets, routes, warnings, nil
}

func singBoxInbounds(target Target, modern bool, dns dnsShared) []any {
	if target.Platform != "default" {
		inbound := map[string]any{"type": "tun", "tag": "tun-in", "address": []string{"172.19.0.1/30"}, "auto_route": true, "strict_route": true, "stack": "mixed"}
		if target.Platform == "windows" {
			inbound["interface_name"] = "sing-box"
		}
		return []any{inbound}
	}
	dnsInbound := map[string]any{"type": "direct", "tag": "dns-in", "listen": "::", "listen_port": dns.DNSListenPort}
	tproxy := map[string]any{"type": "tproxy", "tag": "tproxy-in", "listen": "::", "listen_port": dns.TProxyPort, "tcp_multi_path": false, "tcp_fast_open": true, "udp_fragment": true}
	if !modern {
		dnsInbound["sniff"] = true
		tproxy["sniff"] = true
		tproxy["sniff_override_destination"] = false
	}
	return []any{dnsInbound, tproxy}
}

func singBoxDNS(custom map[string]any, modern, desktop bool, shared dnsShared, remoteOutbound string) map[string]any {
	if override := singBoxDNSOverride(custom, modern); override != nil {
		return override
	}
	if modern {
		servers := []any{map[string]any{"type": "local", "tag": "local"}}
		if shared.FakeIPEnabled && !desktop {
			servers = append(servers, map[string]any{"type": "fakeip", "tag": "fakeip", "inet4_range": shared.FakeIPIPv4Range, "inet6_range": shared.FakeIPIPv6Range})
		}
		servers = append(servers, map[string]any{"type": "udp", "tag": "local_v4", "server": shared.LocalDNS, "server_port": shared.LocalDNSPort})
		if desktop {
			servers = append(servers,
				map[string]any{"type": "tls", "tag": "bootstrap", "server": "223.5.5.5", "server_port": 853, "tls": map[string]any{"server_name": "dns.alidns.com"}},
				map[string]any{"type": "tls", "tag": "remote", "server": "8.8.8.8", "server_port": 853, "tls": map[string]any{"server_name": "dns.google"}, "detour": remoteOutbound},
			)
		}
		rules := singBoxDNSRules(shared, true, !desktop)
		result := map[string]any{"servers": servers, "rules": rules, "independent_cache": false}
		if desktop {
			result["reverse_mapping"] = true
			result["final"] = "remote"
		}
		return result
	}
	servers := []any{map[string]any{"tag": "local", "address": shared.LocalDNS, "detour": "direct"}}
	if shared.FakeIPEnabled && !desktop {
		servers = append(servers, map[string]any{"tag": "fakeip", "address": "fakeip", "strategy": "ipv4_only"})
	}
	servers = append(servers, map[string]any{"tag": "local_v4", "address": shared.LocalDNS, "strategy": "ipv4_only", "detour": "direct"})
	result := map[string]any{"disable_cache": false, "servers": servers, "rules": singBoxDNSRules(shared, false, !desktop), "disable_expire": false, "independent_cache": false, "reverse_mapping": desktop}
	if shared.FakeIPEnabled && !desktop {
		result["fakeip"] = map[string]any{"enabled": true, "inet4_range": shared.FakeIPIPv4Range, "inet6_range": shared.FakeIPIPv6Range}
	}
	return result
}

type dnsShared struct {
	LocalDNS, FakeIPIPv4Range, FakeIPIPv6Range, ClashAPISecret, ClashAPIUIPath string
	LocalDNSPort, FakeIPTTL, DNSListenPort, TProxyPort, ClashAPIPort           int
	FakeIPEnabled, RejectHTTPS, CNDomainLocalDNS                               bool
}

func resolveDNSShared(config map[string]any) dnsShared {
	result := dnsShared{LocalDNS: "127.0.0.1", LocalDNSPort: 53, FakeIPIPv4Range: "198.18.0.0/15", FakeIPIPv6Range: "fc00::/18", FakeIPEnabled: true, FakeIPTTL: 300, DNSListenPort: 1053, TProxyPort: 7893, RejectHTTPS: true, CNDomainLocalDNS: true, ClashAPIPort: 9999, ClashAPISecret: "123456", ClashAPIUIPath: "/etc/sb/ui"}
	shared := config
	if nested, ok := objectValue(config["shared"]); ok {
		shared = nested
	}
	result.LocalDNS = valueOr(stringValue(shared["localDns"]), result.LocalDNS)
	result.LocalDNSPort = integerDefault(shared["localDnsPort"], result.LocalDNSPort)
	result.FakeIPIPv4Range = valueOr(stringValue(shared["fakeipIpv4Range"]), result.FakeIPIPv4Range)
	result.FakeIPIPv6Range = valueOr(stringValue(shared["fakeipIpv6Range"]), result.FakeIPIPv6Range)
	result.FakeIPEnabled = boolDefault(shared["fakeipEnabled"], result.FakeIPEnabled)
	result.FakeIPTTL = integerDefault(shared["fakeipTtl"], result.FakeIPTTL)
	result.DNSListenPort = integerDefault(shared["dnsListenPort"], result.DNSListenPort)
	result.TProxyPort = integerDefault(shared["tproxyPort"], result.TProxyPort)
	result.RejectHTTPS = boolDefault(shared["rejectHttps"], result.RejectHTTPS)
	result.CNDomainLocalDNS = boolDefault(shared["cnDomainLocalDns"], result.CNDomainLocalDNS)
	result.ClashAPIPort = integerDefault(shared["clashApiPort"], result.ClashAPIPort)
	result.ClashAPISecret = valueOr(stringValue(shared["clashApiSecret"]), result.ClashAPISecret)
	result.ClashAPIUIPath = valueOr(stringValue(shared["clashApiUiPath"]), result.ClashAPIUIPath)
	return result
}

func singBoxDNSOverride(config map[string]any, modern bool) map[string]any {
	key := "singbox"
	if modern {
		key = "singboxV12"
	}
	overrides, _ := objectValue(config["overrides"])
	if result, ok := objectValue(overrides[key]); ok {
		return result
	}
	if modern {
		if result, ok := objectValue(overrides["singbox"]); ok {
			return result
		}
	}
	if result, ok := objectValue(config[key]); ok {
		return result
	}
	return nil
}

func singBoxDNSRules(shared dnsShared, modern, fakeIP bool) []any {
	rules := []any{}
	if shared.RejectHTTPS {
		rules = append(rules, map[string]any{"query_type": []string{"HTTPS"}, "action": "reject"})
	}
	privateRule := map[string]any{"ip_cidr": []string{"127.0.0.0/8", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"}, "server": "local"}
	if modern {
		privateRule["action"] = "route"
	}
	rules = append(rules, privateRule)
	if shared.CNDomainLocalDNS {
		cnRule := map[string]any{"rule_set": []string{"geosite-cn"}, "server": "local"}
		if modern {
			cnRule["action"] = "route"
		}
		rules = append(rules, cnRule)
	}
	if shared.FakeIPEnabled && fakeIP {
		fakeRule := map[string]any{"disable_cache": false, "rewrite_ttl": shared.FakeIPTTL, "query_type": []string{"A", "AAAA"}, "server": "fakeip"}
		if modern {
			fakeRule["action"] = "route"
		}
		rules = append(rules, fakeRule)
	}
	return rules
}

type privateAccessResolved struct {
	Endpoints     []any
	Outbounds     []any
	DirectDomains []string
	RouteRules    []any
	DNSServers    []any
	DNSRules      []any
}

func resolvePrivateAccess(config map[string]any, modern, desktop bool) privateAccessResolved {
	result := privateAccessResolved{Endpoints: []any{}, Outbounds: []any{}, DirectDomains: []string{}, RouteRules: []any{}, DNSServers: []any{}, DNSRules: []any{}}
	enabled, _ := config["enabled"].(bool)
	if !modern || !enabled {
		return result
	}
	connectors, _ := config["connectors"].([]any)
	for index, item := range connectors {
		connector, ok := objectValue(item)
		if !ok {
			continue
		}
		if enabled, present := connector["enabled"].(bool); present && !enabled {
			continue
		}
		tag := valueOr(stringValue(connector["tag"]), fmt.Sprintf("private-access-%d", index+1))
		kind := valueOr(stringValue(connector["type"]), "outbound")
		if endpoint, ok := objectValue(connector["endpoint"]); ok && (kind == "wireguard" || kind == "tailscale") {
			endpoint = cloneMap(endpoint)
			normalizePrivateKeys(endpoint)
			endpoint["type"] = kind
			endpoint["tag"] = tag
			if domain := firstEndpointDomain(endpoint); domain != "" {
				result.DirectDomains = appendUnique(result.DirectDomains, domain)
				if endpoint["domain_resolver"] == nil {
					resolver := "local"
					if desktop {
						resolver = "bootstrap"
					}
					endpoint["domain_resolver"] = map[string]any{"server": resolver, "strategy": "ipv4_only"}
				}
			}
			result.Endpoints = append(result.Endpoints, endpoint)
		} else if outbound, ok := objectValue(connector["outbound"]); ok {
			if !supportedPrivateOutbound(kind) {
				continue
			}
			outbound = cloneMap(outbound)
			normalizePrivateKeys(outbound)
			if kind != "outbound" && kind != "v2ray" && kind != "xray" {
				outbound["type"] = map[bool]string{true: "socks", false: kind}[kind == "socks5"]
			}
			outbound["tag"] = tag
			if domain := domainName(stringValue(outbound["server"])); domain != "" {
				result.DirectDomains = appendUnique(result.DirectDomains, domain)
				if desktop && outbound["domain_resolver"] == nil {
					outbound["domain_resolver"] = map[string]any{"server": "bootstrap", "strategy": "ipv4_only"}
				}
			}
			result.Outbounds = append(result.Outbounds, outbound)
		} else {
			continue
		}
		if routes, ok := objectValue(connector["routes"]); ok {
			rule := map[string]any{"action": "route", "outbound": tag}
			privateMatchers(rule, routes)
			if len(rule) > 2 {
				result.RouteRules = append(result.RouteRules, rule)
			}
		}
		if dnsItems, ok := connector["dns"].([]any); ok {
			for dnsIndex, dnsItem := range dnsItems {
				dns, ok := objectValue(dnsItem)
				if !ok || stringValue(dns["server"]) == "" {
					continue
				}
				dnsTag := valueOr(stringValue(dns["tag"]), fmt.Sprintf("%s-dns-%d", tag, dnsIndex+1))
				result.DNSServers = append(result.DNSServers, map[string]any{"type": "udp", "tag": dnsTag, "server": dns["server"], "server_port": integerDefault(dns["serverPort"], 53), "detour": tag})
				rule := map[string]any{"action": "route", "server": dnsTag}
				privateMatchers(rule, dns)
				result.DNSRules = append(result.DNSRules, rule)
			}
		}
	}
	return result
}

func privateMatchers(target, source map[string]any) {
	aliases := map[string]string{"ipCidrs": "ip_cidr", "domains": "domain", "domainSuffixes": "domain_suffix", "domainKeywords": "domain_keyword", "domainRegexes": "domain_regex"}
	for from, to := range aliases {
		if value := cleanStringValues(source[from]); len(value) > 0 {
			target[to] = value
		}
	}
}
func cleanStringValues(value any) []string {
	items, ok := value.([]any)
	if !ok {
		if strings, ok := value.([]string); ok {
			items = make([]any, len(strings))
			for index := range strings {
				items[index] = strings[index]
			}
		}
	}
	result := []string{}
	for _, item := range items {
		if text := strings.TrimSpace(stringValue(item)); text != "" {
			result = append(result, text)
		}
	}
	return result
}
func supportedPrivateOutbound(value string) bool {
	switch value {
	case "outbound", "v2ray", "xray", "vmess", "vless", "trojan", "socks", "socks5", "http", "ssh", "hysteria2", "tuic", "anytls":
		return true
	default:
		return false
	}
}
func firstEndpointDomain(endpoint map[string]any) string {
	peers, _ := endpoint["peers"].([]any)
	if len(peers) == 0 {
		return ""
	}
	peer, _ := objectValue(peers[0])
	return domainName(stringValue(peer["address"]))
}
func domainName(value string) string {
	value = strings.TrimSpace(value)
	if value == "" || net.ParseIP(value) != nil {
		return ""
	}
	return value
}
func cloneMap(value map[string]any) map[string]any {
	encoded, _ := json.Marshal(value)
	var result map[string]any
	_ = json.Unmarshal(encoded, &result)
	return result
}
func normalizePrivateKeys(value map[string]any) {
	aliases := map[string]string{"privateKey": "private_key", "publicKey": "public_key", "preSharedKey": "pre_shared_key", "allowedIps": "allowed_ips", "persistentKeepaliveInterval": "persistent_keepalive_interval", "domainResolver": "domain_resolver", "serverPort": "server_port", "listenPort": "listen_port", "alterId": "alter_id"}
	for from, to := range aliases {
		if item := value[from]; item != nil {
			value[to] = item
			delete(value, from)
		}
	}
	for _, item := range value {
		if child, ok := objectValue(item); ok {
			normalizePrivateKeys(child)
		}
		if list, ok := item.([]any); ok {
			for _, nested := range list {
				if child, ok := objectValue(nested); ok {
					normalizePrivateKeys(child)
				}
			}
		}
	}
}

func convertRule(line string) (map[string]any, bool) {
	parts := strings.Split(line, ",")
	if len(parts) < 3 {
		return nil, false
	}
	rule := map[string]any{"outbound": normalizeOutboundName(strings.TrimSpace(parts[2]))}
	value := strings.TrimSpace(parts[1])
	switch strings.TrimSpace(parts[0]) {
	case "DOMAIN":
		rule["domain"] = value
	case "DOMAIN-SUFFIX":
		rule["domain_suffix"] = value
	case "DOMAIN-KEYWORD":
		rule["domain_keyword"] = value
	case "DOMAIN-REGEX":
		rule["domain_regex"] = value
	case "IP-CIDR":
		rule["ip_cidr"] = value
	case "SRC-IP-CIDR":
		rule["source_ip_cidr"] = value
	default:
		return nil, false
	}
	return rule, true
}

func parseRuleProvider(data []byte) ([]any, []string, error) {
	var document struct {
		Payload []string `yaml:"payload"`
	}
	lines := []string{}
	if err := yaml.Unmarshal(data, &document); err == nil {
		lines = document.Payload
	}
	if len(lines) == 0 {
		var sequence []string
		if err := yaml.Unmarshal(data, &sequence); err == nil {
			lines = sequence
		}
	}
	if len(lines) == 0 {
		for _, line := range strings.Split(string(data), "\n") {
			line = strings.TrimSpace(line)
			if line == "" || strings.HasPrefix(line, "#") || line == "payload:" {
				continue
			}
			lines = append(lines, strings.TrimSpace(strings.TrimPrefix(line, "- ")))
		}
	}
	grouped := map[string]any{}
	diagnostics := []string{}
	appendString := func(key, value string) {
		values, _ := grouped[key].([]string)
		grouped[key] = append(values, value)
	}
	appendPort := func(key, value string) bool {
		port, err := strconv.Atoi(value)
		if err != nil || port < 0 || port > 65535 {
			return false
		}
		values, _ := grouped[key].([]int)
		grouped[key] = append(values, port)
		return true
	}
	for _, raw := range lines {
		line := strings.TrimSpace(raw)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		parts := strings.SplitN(line, ",", 3)
		if len(parts) == 1 {
			appendString("domain", strings.TrimSpace(parts[0]))
			continue
		}
		kind, value := strings.TrimSpace(parts[0]), strings.TrimSpace(parts[1])
		switch kind {
		case "DOMAIN", "+", "HOST":
			appendString("domain", value)
		case "DOMAIN-SUFFIX", "HOST-SUFFIX":
			appendString("domain_suffix", value)
		case "DOMAIN-KEYWORD", "HOST-KEYWORD":
			appendString("domain_keyword", value)
		case "DOMAIN-REGEX":
			appendString("domain_regex", value)
		case "IP-CIDR", "IP-CIDR6":
			appendString("ip_cidr", value)
		case "SRC-IP-CIDR":
			appendString("source_ip_cidr", value)
		case "DST-PORT":
			if !appendPort("port", value) {
				diagnostics = append(diagnostics, "invalid provider port rule: "+line)
			}
		case "SRC-PORT":
			if !appendPort("source_port", value) {
				diagnostics = append(diagnostics, "invalid provider port rule: "+line)
			}
		case "PROCESS-NAME", "PROCESS-PATH":
			// These rules can crash sing-box mobile clients and are intentionally omitted.
		default:
			diagnostics = append(diagnostics, "unsupported provider rule: "+line)
		}
	}
	if len(grouped) == 0 {
		return nil, diagnostics, fmt.Errorf("provider has no convertible rules")
	}
	return []any{grouped}, diagnostics, nil
}
func validateSubscriptionContent(data []byte) error {
	parsed := Parse(string(data))
	if len(parsed.Nodes) == 0 {
		return fmt.Errorf("no usable proxy nodes: %s", strings.Join(parsed.Diagnostics, "; "))
	}
	return nil
}
func validateRuleProviderContent(data []byte) error {
	_, _, err := parseRuleProvider(data)
	return err
}
func officialRuleSet(tag, url string) map[string]any {
	return map[string]any{"tag": tag, "type": "remote", "format": "binary", "url": url, "download_detour": "direct"}
}
func normalizePlatform(value string) string {
	switch value {
	case "windows":
		return "windows"
	case "darwin", "macos":
		return "macos"
	default:
		return "default"
	}
}
func normalizePrefix(value string) string {
	if value == "" {
		return ""
	}
	for _, suffix := range []string{"-", " ", "丨", "|", "｜", "/", "_", "·", ")", "）", "]", "】", "}", "》", ">", "」"} {
		if strings.HasSuffix(value, suffix) {
			return value
		}
	}
	return value + "丨"
}
func sourceLabel(source Source) string {
	if source.Remark != "" {
		return source.Remark
	}
	if source.URL != "" {
		return source.URL
	}
	return source.ID
}
func redactSource(source Source) Source {
	if source.Type == SourceRaw {
		source.Content = ""
	}
	return source
}
func uniqueNodeNames(nodes []Proxy, nodeOrigins []string) ([]Proxy, map[string]string) {
	counts := map[string]int{}
	origins := map[string]string{}
	result := make([]Proxy, 0, len(nodes))
	for index, node := range nodes {
		base := node.Name
		counts[base]++
		if counts[base] > 1 {
			node.Name = fmt.Sprintf("%s (%d)", base, counts[base])
		}
		result = append(result, node)
		origins[node.Name] = nodeOrigins[index]
	}
	return result, origins
}
func appendUnique(target []string, values ...string) []string {
	seen := map[string]bool{}
	for _, value := range target {
		seen[value] = true
	}
	for _, value := range values {
		if !seen[value] {
			target = append(target, value)
			seen[value] = true
		}
	}
	return target
}
func normalizeOutboundName(value string) string {
	switch value {
	case "DIRECT", "🚀 直接连接":
		return "direct"
	case "REJECT":
		return "reject"
	default:
		return value
	}
}
func builtinOutboundName(value string) bool {
	normalized := normalizeOutboundName(value)
	return normalized == "direct" || normalized == "reject" || normalized == "dns-out"
}
func foreignOutbound(groups []ProxyGroup, fallback string) string {
	for _, group := range groups {
		if group.Name == "🔰 国外流量" {
			return group.Name
		}
	}
	return fallback
}
func normalizeOutboundNames(values []string) []string {
	result := make([]string, 0, len(values))
	for _, value := range values {
		result = append(result, normalizeOutboundName(value))
	}
	return result
}
func deepMerge(target, source map[string]any) {
	for key, value := range source {
		if child, ok := objectValue(value); ok {
			if current, ok := objectValue(target[key]); ok {
				deepMerge(current, child)
				continue
			}
		}
		target[key] = value
	}
}
func sortedKeys(value map[string]any) []string {
	result := make([]string, 0, len(value))
	for key := range value {
		result = append(result, key)
	}
	sort.Strings(result)
	return result
}
