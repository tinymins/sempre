package subscription

import (
	"context"
	"fmt"
	"net"
	"strings"
	"sync"

	singboxcore "github.com/tinymins/sempre/internal/core/singbox"
)

func (compiler *Compiler) buildSingBox(ctx context.Context, profile Profile, proxies []Proxy, target Target, force bool) (map[string]any, []FieldDiff, []string, error) {
	modern := target.Version != "11"
	policy := singboxcore.ResolvePlatformPolicy(target.Version, target.Platform)
	private := resolvePrivateAccess(profile.PrivateAccess, modern, target.Platform != "default")
	shared := resolveDNSShared(profile.DNS)
	outbounds := []any{map[string]any{"type": "direct", "tag": "direct"}, map[string]any{"type": "block", "tag": "reject"}}
	if !modern {
		outbounds = append(outbounds, map[string]any{"type": "dns", "tag": "dns-out"})
	}
	names := make([]string, 0, len(proxies))
	diffs := []FieldDiff{}
	warnings := []string{}
	if shared.FakeIPEnabled && !policy.FakeIP {
		shared.FakeIPEnabled = false
		warnings = append(warnings, "FakeIP is unavailable for standalone sing-box on macOS without system DNS integration; using the compatible real-IP mode")
	}
	if target.Platform == "macos" && (target.Version == "13" || target.Version == "14") {
		warnings = append(warnings, "standalone sing-box v1.13 and newer on macOS cannot replace the removed sniff destination override; transparent TUN depends on the existing system resolver")
	}
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
		if modern && net.ParseIP(proxy.Server) == nil {
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
			defaultMember := defaultSelectorMember(group, members)
			if !configuredMember(members, defaultMember) {
				return nil, diffs, warnings, fmt.Errorf("proxy group %q default %q is not an available member", group.Name, group.Default)
			}
			outbound["default"] = defaultMember
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
	if target.Platform == "default" && shared.SystemDNSTakeoverEnabled {
		for _, tag := range systemDNSInboundTags(shared.SystemDNSListenHosts) {
			routeRules = append(routeRules, map[string]any{"inbound": tag, "action": "sniff"}, map[string]any{"inbound": tag, "protocol": "dns", "action": "hijack-dns"})
		}
	}
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
	if err := validateManagedOverrides(profile, target); err != nil {
		return nil, diffs, warnings, err
	}
	if err := validateSingBoxSystemDNS(shared, target, profile); err != nil {
		return nil, diffs, warnings, err
	}
	inbounds := singBoxInbounds(target, modern, policy, profile.TransparentProxy, profile.LocalProxy, shared)
	remoteOutbound := valueOr(shared.RemoteDetour, foreignOutbound(groups, final))
	dns := singBoxDNS(profile.DNS, target.Version, shared, remoteOutbound)
	if servers, ok := dns["servers"].([]any); ok {
		dns["servers"] = append(servers, private.DNSServers...)
	}
	if rules, ok := dns["rules"].([]any); ok {
		privateRules := append([]any{}, private.DNSRules...)
		if len(private.DirectDomains) > 0 {
			privateRules = append([]any{map[string]any{"domain": private.DirectDomains, "action": "route", "server": "local"}}, privateRules...)
		}
		dns["rules"] = append(privateRules, rules...)
	}
	route := map[string]any{"rules": routeRules, "rule_set": ruleSets, "final": final}
	if modern {
		route["default_domain_resolver"] = map[string]any{"server": "bootstrap", "strategy": "ipv4_only"}
	}
	if target.Platform != "default" || profile.TransparentProxy.Mode == TransparentProxyTUN {
		route["auto_detect_interface"] = true
	}
	config := map[string]any{"log": singBoxLog(profile.LogLevel), "dns": dns, "inbounds": inbounds, "outbounds": outbounds, "route": route, "experimental": map[string]any{"cache_file": map[string]any{"enabled": true, "store_fakeip": shared.FakeIPEnabled && target.Platform == "default", "store_rdrc": false}, "clash_api": map[string]any{"external_controller": "", "external_ui": "", "secret": "", "default_mode": "rule"}}}
	if len(private.Endpoints) > 0 {
		config["endpoints"] = private.Endpoints
	}
	deepMerge(config, profile.CoreOverrides["sing-box"])
	if !policy.FakeIP {
		if mergedDNS, ok := objectValue(config["dns"]); ok {
			config["dns"] = singBoxDNSWithoutFakeIP(mergedDNS)
		}
	}
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
