package subscription

import (
	"fmt"
	"net/netip"
	"strings"

	singboxcore "github.com/tinymins/sempre/internal/core/singbox"
)

func singBoxInbounds(target Target, modern bool, policy singboxcore.PlatformPolicy, transparent TransparentProxyConfig, localProxy LocalProxyConfig, shared dnsShared) []any {
	inbounds := localProxySingBoxInbounds(localProxy)
	if target.Platform == "default" && shared.SystemDNSTakeoverEnabled {
		for index, host := range shared.SystemDNSListenHosts {
			inbounds = append(inbounds, map[string]any{"type": "direct", "tag": systemDNSInboundTag(host, index), "listen": host, "listen_port": shared.SystemDNSListenPort, "override_address": "1.1.1.1", "override_port": 53})
		}
	}
	if target.Platform != "default" {
		inbound := map[string]any{"type": "tun", "tag": "tun-in", "address": []string{"172.19.0.1/30"}, "auto_route": true, "strict_route": true, "stack": "mixed"}
		if target.Platform == "windows" {
			inbound["interface_name"] = "sing-box"
		}
		if policy.LegacySniffOverride {
			inbound["sniff"] = true
			inbound["sniff_override_destination"] = true
		}
		if policy.TUNDNSMode != "" {
			inbound["dns_mode"] = policy.TUNDNSMode
		}
		return append(inbounds, inbound)
	}
	switch transparent.Mode {
	case TransparentProxyDisabled:
		return inbounds
	case TransparentProxyTUN:
		address := valueOr(transparent.TUN.Address, "172.19.0.1/30")
		inbound := map[string]any{
			"type": "tun", "tag": "tun-in", "interface_name": transparent.TUN.InterfaceName,
			"address": []string{address}, "auto_route": true, "auto_redirect": true,
			"strict_route": true, "stack": "system",
		}
		if len(transparent.RouteExclusions) > 0 {
			inbound["route_exclude_address"] = transparent.RouteExclusions
		}
		return append(inbounds, inbound)
	}
	dnsInbound := map[string]any{"type": "direct", "tag": "dns-in", "listen": "::", "listen_port": transparent.TProxy.DNSListenPort}
	tproxy := map[string]any{"type": "tproxy", "tag": "tproxy-in", "listen": "::", "listen_port": transparent.TProxy.ListenPort, "tcp_multi_path": false, "tcp_fast_open": true, "udp_fragment": true}
	if !modern {
		dnsInbound["sniff"] = true
		tproxy["sniff"] = true
		tproxy["sniff_override_destination"] = false
	}
	return append(inbounds, dnsInbound, tproxy)
}

func localProxySingBoxInbounds(config LocalProxyConfig) []any {
	users := []map[string]any{{"username": config.Username, "password": config.Password}}
	return []any{
		map[string]any{"type": "socks", "tag": "sempre-socks-in", "listen": "127.0.0.1", "listen_port": config.SOCKSPort, "users": users},
		map[string]any{"type": "http", "tag": "sempre-http-in", "listen": "127.0.0.1", "listen_port": config.HTTPPort, "users": users},
	}
}

func singBoxDNS(custom map[string]any, version string, shared dnsShared, remoteOutbound string) map[string]any {
	modern := version != "11"
	if override := singBoxDNSOverride(custom, modern); override != nil {
		return override
	}
	localDNS, localSystem := localDNSServer(shared.LocalDNS)
	if modern {
		local := map[string]any{"type": "local", "tag": "local"}
		if !localSystem {
			local = map[string]any{"type": "udp", "tag": "local", "server": localDNS, "server_port": shared.LocalDNSPort}
		}
		servers := []any{local}
		if shared.FakeIPEnabled {
			servers = append(servers, map[string]any{"type": "fakeip", "tag": "fakeip", "inet4_range": shared.FakeIPIPv4Range, "inet6_range": shared.FakeIPIPv6Range})
		}
		if !localSystem {
			servers = append(servers, map[string]any{"type": "udp", "tag": "local_v4", "server": localDNS, "server_port": shared.LocalDNSPort})
		}
		bootstrap := map[string]any{"type": "tls", "tag": "bootstrap", "server": shared.BootstrapDNS, "server_port": shared.BootstrapDNSPort, "tls": map[string]any{"server_name": shared.BootstrapServerName}}
		remote := map[string]any{"type": "tls", "tag": "remote", "server": shared.RemoteDNS, "server_port": shared.RemoteDNSPort, "tls": map[string]any{"server_name": shared.RemoteServerName}}
		setSingBoxDNSDetour(remote, remoteOutbound)
		servers = append(servers,
			bootstrap,
			remote,
		)
		rules := singBoxDNSRules(shared, true, version == "14", shared.FakeIPEnabled)
		result := map[string]any{"servers": servers, "rules": rules, "independent_cache": false, "reverse_mapping": true, "final": "remote"}
		if shared.PreferIPv4 {
			result["strategy"] = "prefer_ipv4"
		}
		return result
	}
	servers := []any{map[string]any{"tag": "local", "address": localDNS}}
	if shared.FakeIPEnabled {
		servers = append(servers, map[string]any{"tag": "fakeip", "address": "fakeip", "strategy": "ipv4_only"})
	}
	if !localSystem {
		servers = append(servers, map[string]any{"tag": "local_v4", "address": localDNS, "strategy": "ipv4_only"})
	}
	bootstrap := map[string]any{"tag": "bootstrap", "address": fmt.Sprintf("tls://%s:%d", shared.BootstrapDNS, shared.BootstrapDNSPort)}
	remote := map[string]any{"tag": "remote", "address": fmt.Sprintf("tls://%s:%d", shared.RemoteDNS, shared.RemoteDNSPort)}
	setSingBoxDNSDetour(remote, remoteOutbound)
	servers = append(servers,
		bootstrap,
		remote,
	)
	result := map[string]any{"disable_cache": false, "servers": servers, "rules": singBoxDNSRules(shared, false, false, shared.FakeIPEnabled), "disable_expire": false, "independent_cache": false, "reverse_mapping": true, "final": "remote"}
	if shared.PreferIPv4 {
		result["strategy"] = "prefer_ipv4"
	}
	if shared.FakeIPEnabled {
		result["fakeip"] = map[string]any{"enabled": true, "inet4_range": shared.FakeIPIPv4Range, "inet6_range": shared.FakeIPIPv6Range}
	}
	return result
}

func setSingBoxDNSDetour(server map[string]any, detour string) {
	if detour == "" || detour == "direct" {
		return
	}
	server["detour"] = detour
}

func localDNSServer(value string) (string, bool) {
	for part := range strings.SplitSeq(value, ",") {
		part = strings.TrimSpace(part)
		if part == "" {
			continue
		}
		return part, strings.EqualFold(part, "local")
	}
	return "local", true
}

type dnsShared struct {
	LocalDNS, FakeIPIPv4Range, FakeIPIPv6Range                                   string
	BootstrapDNS, BootstrapServerName, RemoteDNS, RemoteServerName, RemoteDetour string
	SystemDNSListenHosts                                                         []string
	LocalDNSPort, FakeIPTTL                                                      int
	BootstrapDNSPort, RemoteDNSPort                                              int
	SystemDNSListenPort                                                          int
	FakeIPEnabled, RejectHTTPS, CNDomainLocalDNS, PreferIPv4                     bool
	SystemDNSTakeoverEnabled                                                     bool
}

func resolveDNSShared(config map[string]any) dnsShared {
	result := dnsShared{LocalDNS: "local", LocalDNSPort: 53, FakeIPIPv4Range: "198.18.0.0/15", FakeIPIPv6Range: "fc00::/18", FakeIPEnabled: true, FakeIPTTL: 300, RejectHTTPS: true, CNDomainLocalDNS: true, BootstrapDNS: "223.5.5.5", BootstrapDNSPort: 853, BootstrapServerName: "dns.alidns.com", RemoteDNS: "8.8.8.8", RemoteDNSPort: 853, RemoteServerName: "dns.google", PreferIPv4: true, SystemDNSListenPort: 53, SystemDNSListenHosts: []string{"127.0.0.1"}}
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
	result.RejectHTTPS = boolDefault(shared["rejectHttps"], result.RejectHTTPS)
	result.CNDomainLocalDNS = boolDefault(shared["cnDomainLocalDns"], result.CNDomainLocalDNS)
	result.BootstrapDNS = valueOr(stringValue(shared["bootstrapDns"]), result.BootstrapDNS)
	result.BootstrapDNSPort = integerDefault(shared["bootstrapDnsPort"], result.BootstrapDNSPort)
	result.BootstrapServerName = valueOr(stringValue(shared["bootstrapServerName"]), result.BootstrapServerName)
	result.RemoteDNS = valueOr(stringValue(shared["remoteDns"]), result.RemoteDNS)
	result.RemoteDNSPort = integerDefault(shared["remoteDnsPort"], result.RemoteDNSPort)
	result.RemoteServerName = valueOr(stringValue(shared["remoteServerName"]), result.RemoteServerName)
	result.RemoteDetour = stringValue(shared["remoteDetour"])
	result.PreferIPv4 = boolDefault(shared["preferIpv4"], result.PreferIPv4)
	result.SystemDNSTakeoverEnabled = boolDefault(shared["systemDnsTakeoverEnabled"], result.SystemDNSTakeoverEnabled)
	result.SystemDNSListenPort = integerDefault(shared["systemDnsListenPort"], result.SystemDNSListenPort)
	result.SystemDNSListenHosts = normalizeSystemDNSListenHosts(stringListValue(shared["systemDnsListenHosts"]))
	return result
}

func validateSingBoxSystemDNS(shared dnsShared, target Target, profile Profile) error {
	if !shared.SystemDNSTakeoverEnabled {
		return nil
	}
	if target.Platform != "default" {
		return fmt.Errorf("system DNS takeover is only available for Linux system sing-box runtime")
	}
	if shared.SystemDNSListenPort != 53 {
		return fmt.Errorf("system DNS takeover requires listen port 53 because resolv.conf cannot specify ports")
	}
	if !containsString(shared.SystemDNSListenHosts, "127.0.0.1") && !containsString(shared.SystemDNSListenHosts, "0.0.0.0") {
		return fmt.Errorf("system DNS takeover listen hosts must include 127.0.0.1 or 0.0.0.0")
	}
	if _, localSystem := localDNSServer(shared.LocalDNS); localSystem {
		return fmt.Errorf("system DNS takeover requires an explicit local DNS upstream instead of local")
	}
	for _, port := range []int{profile.LocalProxy.SOCKSPort, profile.LocalProxy.HTTPPort, profile.TransparentProxy.TProxy.ListenPort, profile.TransparentProxy.TProxy.DNSListenPort} {
		if port == shared.SystemDNSListenPort {
			return fmt.Errorf("system DNS takeover port %d conflicts with another managed listener", shared.SystemDNSListenPort)
		}
	}
	return nil
}

func normalizeSystemDNSListenHosts(values []string) []string {
	hosts := []string{}
	for _, value := range values {
		host := strings.TrimSpace(value)
		if host == "" {
			continue
		}
		address, err := netip.ParseAddr(host)
		if err != nil || !address.Is4() {
			continue
		}
		host = address.String()
		if host == "0.0.0.0" {
			return []string{"0.0.0.0"}
		}
		hosts = appendUnique(hosts, host)
	}
	if len(hosts) == 0 {
		return []string{"127.0.0.1"}
	}
	return hosts
}

func systemDNSInboundTags(hosts []string) []string {
	tags := make([]string, 0, len(hosts))
	for index, host := range hosts {
		tags = append(tags, systemDNSInboundTag(host, index))
	}
	return tags
}

func systemDNSInboundTag(host string, index int) string {
	switch host {
	case "127.0.0.1":
		return "system-dns-in"
	case "0.0.0.0":
		return "system-dns-in-any"
	default:
		return fmt.Sprintf("system-dns-in-%d", index)
	}
}

func validateManagedOverrides(profile Profile, target Target) error {
	override := profile.CoreOverrides["sing-box"]
	if target.Platform != "default" {
		return nil
	}
	if _, exists := override["inbounds"]; exists {
		return fmt.Errorf("top-level inbound overrides require transparent_proxy.mode=disabled on Linux")
	}
	if route, ok := objectValue(override["route"]); ok {
		if value, exists := route["auto_detect_interface"]; exists && value != true && profile.TransparentProxy.Mode == TransparentProxyTUN {
			return fmt.Errorf("route.auto_detect_interface cannot be disabled in Linux tun-router mode")
		}
	}
	if experimental, ok := objectValue(override["experimental"]); ok {
		if clashAPI, ok := objectValue(experimental["clash_api"]); ok {
			for _, key := range []string{"external_controller", "secret", "external_ui"} {
				if _, exists := clashAPI[key]; exists {
					return fmt.Errorf("experimental.clash_api.%s is managed by the structured management API settings", key)
				}
			}
		}
	}
	return nil
}

func validateMihomoOverrides(profile Profile, coreID string) error {
	override := profile.CoreOverrides[coreID]
	if profile.TransparentProxy.Mode == TransparentProxyTUN {
		if _, exists := override["tun"]; exists {
			return fmt.Errorf("top-level tun overrides require transparent_proxy.mode=disabled")
		}
	}
	keys := []string{}
	if coreID == "mihomo" || profile.TransparentProxy.Mode == TransparentProxyTProxy {
		keys = append(keys, "listeners")
	}
	if profile.TransparentProxy.Mode == TransparentProxyTProxy {
		keys = append(keys, "tproxy-port", "routing-mark")
	}
	for _, key := range keys {
		if _, exists := override[key]; exists {
			return fmt.Errorf("top-level %s is managed by Sempre runtime settings", key)
		}
	}
	for _, key := range []string{"external-controller", "external-controller-tls", "external-controller-unix", "external-controller-pipe", "secret", "external-ui"} {
		if _, exists := override[key]; exists {
			return fmt.Errorf("top-level %s is managed by the structured management API settings", key)
		}
	}
	return nil
}

func singBoxDNSOverride(config map[string]any, modern bool) map[string]any {
	key := "sing_box_v11"
	if modern {
		key = "sing_box_v12"
	}
	return coreDNSOverride(config, key)
}

func singBoxDNSWithoutFakeIP(config map[string]any) map[string]any {
	result := cloneMap(config)
	delete(result, "fakeip")
	if servers, ok := result["servers"].([]any); ok {
		filtered := make([]any, 0, len(servers))
		for _, raw := range servers {
			server, _ := raw.(map[string]any)
			if stringValue(server["tag"]) == "fakeip" || stringValue(server["type"]) == "fakeip" || stringValue(server["address"]) == "fakeip" {
				continue
			}
			filtered = append(filtered, raw)
		}
		result["servers"] = filtered
	}
	if rules, ok := result["rules"].([]any); ok {
		filtered := make([]any, 0, len(rules))
		for _, raw := range rules {
			rule, _ := raw.(map[string]any)
			if stringValue(rule["server"]) == "fakeip" {
				continue
			}
			filtered = append(filtered, raw)
		}
		result["rules"] = filtered
	}
	return result
}

func coreDNSOverride(config map[string]any, key string) map[string]any {
	modes, _ := objectValue(config["modes"])
	if stringValue(modes[key]) != "native" {
		return nil
	}
	overrides, _ := objectValue(config["overrides"])
	result, _ := objectValue(overrides[key])
	return result
}

func singBoxDNSRules(shared dnsShared, modern, responseMatching, fakeIP bool) []any {
	rules := []any{}
	if shared.RejectHTTPS {
		rules = append(rules, map[string]any{"query_type": []string{"HTTPS"}, "action": "reject"})
	}
	privateCIDRs := []string{"127.0.0.0/8", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"}
	if responseMatching {
		rules = append(rules,
			map[string]any{"action": "evaluate", "server": "local"},
			map[string]any{"match_response": true, "ip_cidr": privateCIDRs, "action": "respond"},
		)
	} else {
		privateRule := map[string]any{"ip_cidr": privateCIDRs, "server": "local"}
		if modern {
			privateRule["action"] = "route"
		}
		rules = append(rules, privateRule)
	}
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
