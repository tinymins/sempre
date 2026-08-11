package subscription

import (
	"net"
	"strconv"
	"strings"

	"gopkg.in/yaml.v3"
)

func buildClash(profile Profile, proxies []Proxy, meta bool, coreID string) (string, error) {
	proxyMaps := make([]map[string]any, 0, len(proxies))
	names := make([]string, 0, len(proxies))
	for _, proxy := range proxies {
		mapped := proxy.Map()
		if coreID == "clash-rs" && proxy.Type == "hysteria2" {
			if _, exists := mapped["skip-cert-verify"]; !exists {
				mapped["skip-cert-verify"] = false
			}
		}
		proxyMaps = append(proxyMaps, mapped)
		names = append(names, proxy.Name)
	}
	groups := clashGroups(profile.Groups, names)
	rules := append([]string{}, profile.Rules...)
	providers := map[string]any{}
	for _, provider := range profile.RuleProviders {
		behavior := valueOr(provider.Behavior, "classical")
		item := map[string]any{
			"type": "http", "behavior": behavior, "url": provider.URL,
			"path": "./rules/" + provider.Tag, "interval": 86400,
		}
		if provider.Format != "" {
			item["format"] = provider.Format
		}
		providers[provider.Tag] = item
		rules = append(rules, "RULE-SET,"+provider.Tag+","+valueOr(provider.Outbound, groups[0]["name"].(string)))
	}
	rules = append(rules, "DOMAIN-SUFFIX,local,DIRECT", "GEOIP,LAN,DIRECT,no-resolve", "GEOIP,CN,DIRECT,no-resolve", "MATCH,"+clashFinalGroup(profile.Groups))
	if (coreID == "mihomo" || coreID == "clash-rs") && profile.TransparentProxy.Mode == TransparentProxyTProxy {
		proxyMaps = append(proxyMaps, map[string]any{"name": "sempre-dns-out", "type": "dns"})
		rules = append([]string{"DST-PORT,53,sempre-dns-out"}, rules...)
	}
	config := map[string]any{
		"allow-lan": true, "mode": "Rule", "log-level": clashLogLevel(profile.LogLevel),
		"proxies": proxyMaps, "proxy-groups": groups,
		"rule-providers": providers, "rules": rules,
		"profile": map[string]any{"store-selected": true, "store-fake-ip": true, "tracing": true},
	}
	if meta {
		config["unified-delay"] = true
		config["tcp-concurrent"] = true
		config["find-process-mode"] = "strict"
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
	if coreID == "mihomo" || coreID == "clash-rs" {
		if err := validateMihomoOverrides(profile, coreID); err != nil {
			return "", err
		}
		configureClashRuntime(config, profile, coreID)
		if override := coreDNSOverride(profile.DNS, coreID); override != nil {
			config["dns"] = override
		} else if coreID == "clash-rs" {
			config["dns"] = clashRSDNS(profile)
		} else {
			config["dns"] = mihomoDNS(profile)
		}
		deepMerge(config, profile.CoreOverrides[coreID])
	} else if override := legacyClashDNSOverride(profile.DNS, meta); override != nil {
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
		if group.Default != "" && configuredMember(proxies, group.Default) {
			ordered := []string{group.Default}
			for _, proxy := range proxies {
				if proxy != group.Default {
					ordered = append(ordered, proxy)
				}
			}
			proxies = ordered
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

func legacyClashDNSOverride(config map[string]any, meta bool) map[string]any {
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

func configureClashRuntime(config map[string]any, profile Profile, coreID string) {
	transparent := profile.TransparentProxy
	listeners := []map[string]any{}
	if coreID == "clash-rs" {
		config["socks-port"] = profile.LocalProxy.SOCKSPort
		config["port"] = profile.LocalProxy.HTTPPort
		config["bind-address"] = "127.0.0.1"
		config["allow-lan"] = false
		config["authentication"] = []string{profile.LocalProxy.Username + ":" + profile.LocalProxy.Password}
	} else {
		users := []map[string]any{{"username": profile.LocalProxy.Username, "password": profile.LocalProxy.Password}}
		listeners = append(listeners,
			map[string]any{"name": "sempre-socks-in", "type": "socks", "listen": "127.0.0.1", "port": profile.LocalProxy.SOCKSPort, "udp": true, "users": users},
			map[string]any{"name": "sempre-http-in", "type": "http", "listen": "127.0.0.1", "port": profile.LocalProxy.HTTPPort, "users": users},
		)
	}
	switch transparent.Mode {
	case TransparentProxyTUN:
		var tun map[string]any
		if coreID == "clash-rs" {
			tun = map[string]any{
				"enable": true, "device": transparent.TUN.InterfaceName,
				"gateway": valueOr(transparent.TUN.Address, "198.18.0.1/30"), "route-all": true, "dns-hijack": true,
			}
		} else {
			tun = map[string]any{
				"enable": true, "stack": "system", "device": transparent.TUN.InterfaceName,
				"auto-route": true, "auto-redirect": true, "strict-route": true, "auto-detect-interface": true,
				"dns-hijack": []string{"any:53", "tcp://any:53"},
			}
			if len(transparent.RouteExclusions) > 0 {
				tun["route-exclude-address"] = transparent.RouteExclusions
			}
			if transparent.InterfaceMode == "include" {
				tun["include-interface"] = transparent.Interfaces
			} else if transparent.InterfaceMode == "exclude" {
				tun["exclude-interface"] = transparent.Interfaces
			}
		}
		config["tun"] = tun
	case TransparentProxyTProxy:
		config["tproxy-port"] = transparent.TProxy.ListenPort
		listeners = append(listeners, map[string]any{
			"name": "sempre-dns-in", "type": "tproxy", "listen": "0.0.0.0",
			"port": transparent.TProxy.DNSListenPort, "udp": true,
		})
	}
	if len(listeners) > 0 {
		config["listeners"] = listeners
	}
}

func clashRSDNS(profile Profile) map[string]any {
	shared := resolveDNSShared(profile.DNS)
	remote := "tls://" + net.JoinHostPort(shared.RemoteDNS, strconv.Itoa(shared.RemoteDNSPort))
	remoteDetour := valueOr(shared.RemoteDetour, clashFinalGroup(profile.Groups))
	if remoteDetour != "" {
		remote += "#" + remoteDetour
	}
	local := "udp://" + net.JoinHostPort(shared.LocalDNS, strconv.Itoa(shared.LocalDNSPort))
	result := map[string]any{
		"enable": true, "ipv6": true, "respect-rules": true,
		"enhanced-mode":           map[bool]string{true: "fake-ip", false: "redir-host"}[shared.FakeIPEnabled],
		"default-nameserver":      []string{shared.BootstrapDNS},
		"proxy-server-nameserver": []string{shared.BootstrapDNS},
		"nameserver":              []string{remote},
	}
	if shared.CNDomainLocalDNS {
		result["nameserver-policy"] = map[string]string{"geosite:cn": local}
	}
	if shared.FakeIPEnabled {
		result["fake-ip-range"] = shared.FakeIPIPv4Range
	}
	return result
}

func mihomoDNS(profile Profile) map[string]any {
	shared := resolveDNSShared(profile.DNS)
	remote := "tls://" + net.JoinHostPort(shared.RemoteDNS, strconv.Itoa(shared.RemoteDNSPort))
	remoteDetour := valueOr(shared.RemoteDetour, clashFinalGroup(profile.Groups))
	if remoteDetour != "" {
		remote += "#" + remoteDetour
	}
	if shared.RejectHTTPS {
		if strings.Contains(remote, "#") {
			remote += "&disable-qtype-65=true"
		} else {
			remote += "#disable-qtype-65=true"
		}
	}
	local := net.JoinHostPort(shared.LocalDNS, strconv.Itoa(shared.LocalDNSPort))
	if shared.RejectHTTPS {
		local += "#disable-qtype-65=true"
	}
	result := map[string]any{
		"enable": true, "ipv6": true, "respect-rules": true,
		"enhanced-mode":           map[bool]string{true: "fake-ip", false: "redir-host"}[shared.FakeIPEnabled],
		"default-nameserver":      []string{shared.BootstrapDNS},
		"proxy-server-nameserver": []string{shared.BootstrapDNS},
		"direct-nameserver":       []string{local},
		"nameserver":              []string{remote},
	}
	if shared.CNDomainLocalDNS {
		result["nameserver-policy"] = map[string]any{"geosite:cn": []string{local}}
	}
	if shared.FakeIPEnabled {
		result["fake-ip-range"] = shared.FakeIPIPv4Range
		result["fake-ip-range6"] = shared.FakeIPIPv6Range
		result["fake-ip-ttl"] = shared.FakeIPTTL
	}
	return result
}
