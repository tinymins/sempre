package core

import "sort"

const (
	CapabilityLoggingLevel           = "logging.level"
	CapabilityDNSLocalUpstream       = "dns.local_upstream"
	CapabilityDNSRemoteUpstream      = "dns.remote_upstream"
	CapabilityDNSBootstrapUpstream   = "dns.bootstrap_upstream"
	CapabilityDNSBootstrapPort       = "dns.bootstrap_port"
	CapabilityDNSBootstrapServerName = "dns.bootstrap_server_name"
	CapabilityDNSFakeIP              = "dns.fake_ip"
	CapabilityDNSSplit               = "dns.split"
	CapabilityDNSNative              = "dns.native"
	CapabilityDNSPreferIPv4          = "dns.prefer_ipv4"
	CapabilityDNSRemoteServerName    = "dns.remote_server_name"
	CapabilityDNSRemoteDetour        = "dns.remote_detour"
	CapabilityDNSRejectHTTPS         = "dns.reject_https"
	CapabilityRoutingRules           = "routing.rules"
	CapabilityRoutingRuleProviders   = "routing.rule_providers"
	CapabilityRoutingSelector        = "routing.selector"
	CapabilityRoutingURLTest         = "routing.url_test"
	CapabilityLocalProxy             = "inbound.local_proxy"
	CapabilityTransparentTUN         = "transparent.tun"
	CapabilityTransparentTUNAddress  = "transparent.tun.address"
	CapabilityTransparentInterfaces  = "transparent.interface_policy"
	CapabilityTransparentTProxy      = "transparent.tproxy"
	CapabilityManagementConnections  = "management.connections"
	CapabilityManagementSelectors    = "management.selector_switch"
	CapabilityManagementDelay        = "management.delay"
	CapabilityManagementTraffic      = "management.traffic"
	CapabilityManagementExternalAPI  = "management.external_api"
	CapabilityPrivateAccess          = "private_access"
	CapabilityNativeOverride         = "native_override"
)

const (
	StabilityStable       = "stable"
	StabilityExperimental = "experimental"
)

type ProtocolCapability struct {
	Protocol       string   `json:"protocol"`
	Transports     []string `json:"transports"`
	Security       []string `json:"security"`
	MinimumVersion string   `json:"minimum_version,omitempty"`
}

type Capabilities struct {
	Features   []string             `json:"features"`
	EnumValues map[string][]string  `json:"enum_values"`
	Protocols  []ProtocolCapability `json:"protocols"`
}

type CapabilityProvider interface {
	Capabilities(string, Target) Capabilities
	Stability() string
}

func NormalizeCapabilities(value Capabilities) Capabilities {
	value.Features = uniqueSorted(value.Features)
	if value.EnumValues == nil {
		value.EnumValues = map[string][]string{}
	}
	for key, values := range value.EnumValues {
		value.EnumValues[key] = uniqueSorted(values)
	}
	if value.Protocols == nil {
		value.Protocols = []ProtocolCapability{}
	}
	for index := range value.Protocols {
		value.Protocols[index].Transports = uniqueSorted(value.Protocols[index].Transports)
		value.Protocols[index].Security = uniqueSorted(value.Protocols[index].Security)
	}
	sort.Slice(value.Protocols, func(left, right int) bool {
		return value.Protocols[left].Protocol < value.Protocols[right].Protocol
	})
	return value
}

func IntersectCapabilities(values []Capabilities) Capabilities {
	if len(values) == 0 {
		return NormalizeCapabilities(Capabilities{})
	}
	result := NormalizeCapabilities(values[0])
	for _, next := range values[1:] {
		next = NormalizeCapabilities(next)
		result.Features = intersectStrings(result.Features, next.Features)
		for key, current := range result.EnumValues {
			other, exists := next.EnumValues[key]
			if !exists {
				delete(result.EnumValues, key)
				continue
			}
			result.EnumValues[key] = intersectStrings(current, other)
		}
		result.Protocols = intersectProtocols(result.Protocols, next.Protocols)
	}
	return result
}

func intersectProtocols(first, second []ProtocolCapability) []ProtocolCapability {
	available := map[string]ProtocolCapability{}
	for _, protocol := range second {
		available[protocol.Protocol] = protocol
	}
	result := []ProtocolCapability{}
	for _, protocol := range first {
		other, exists := available[protocol.Protocol]
		if !exists {
			continue
		}
		protocol.Transports = intersectStrings(protocol.Transports, other.Transports)
		protocol.Security = intersectStrings(protocol.Security, other.Security)
		if other.MinimumVersion > protocol.MinimumVersion {
			protocol.MinimumVersion = other.MinimumVersion
		}
		result = append(result, protocol)
	}
	return result
}

func uniqueSorted(values []string) []string {
	seen := map[string]bool{}
	result := make([]string, 0, len(values))
	for _, value := range values {
		if value != "" && !seen[value] {
			seen[value] = true
			result = append(result, value)
		}
	}
	sort.Strings(result)
	return result
}

func intersectStrings(first, second []string) []string {
	available := map[string]bool{}
	for _, value := range second {
		available[value] = true
	}
	result := []string{}
	for _, value := range first {
		if available[value] {
			result = append(result, value)
		}
	}
	return uniqueSorted(result)
}
