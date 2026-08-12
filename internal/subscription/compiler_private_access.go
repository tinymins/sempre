package subscription

import (
	"encoding/json"
	"fmt"
	"net"
	"strings"
)

type privateAccessResolved struct {
	Endpoints     []any
	Outbounds     []any
	DirectDomains []string
	RouteRules    []any
	DNSServers    []any
	DNSRules      []any
	UsesTunnel    bool
}

func resolvePrivateAccess(config map[string]any, modern, desktop bool, resolveTunnel TunnelForwardResolver) (privateAccessResolved, error) {
	result := privateAccessResolved{Endpoints: []any{}, Outbounds: []any{}, DirectDomains: []string{}, RouteRules: []any{}, DNSServers: []any{}, DNSRules: []any{}}
	enabled, _ := config["enabled"].(bool)
	if !modern || !enabled {
		return result, nil
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
			forwardID := valueOr(stringValue(connector["tunnel_forward_id"]), stringValue(connector["tunnelForwardId"]))
			if forwardID != "" {
				if kind != "wireguard" {
					return result, fmt.Errorf("private connector %q only supports tunnel forwards for WireGuard", tag)
				}
				if resolveTunnel == nil {
					return result, fmt.Errorf("private connector %q references unavailable tunnel forward %q", tag, forwardID)
				}
				forward, found := resolveTunnel(forwardID)
				if !found {
					return result, fmt.Errorf("private connector %q references missing tunnel forward %q", tag, forwardID)
				}
				peers, _ := endpoint["peers"].([]any)
				if len(peers) == 0 {
					return result, fmt.Errorf("private connector %q requires a WireGuard peer", tag)
				}
				peer, ok := objectValue(peers[0])
				if !ok {
					return result, fmt.Errorf("private connector %q has an invalid WireGuard peer", tag)
				}
				peer["address"] = forward.Host
				peer["port"] = forward.Port
				peers[0] = peer
				endpoint["peers"] = peers
				result.UsesTunnel = true
			}
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
	return result, nil
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
