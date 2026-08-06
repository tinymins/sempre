package xray

import (
	"regexp"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/core/v2rayfamily"
)

func New() *v2rayfamily.Adapter {
	return v2rayfamily.New(v2rayfamily.Kind{
		ID: "xray", Name: "Xray-core", Repository: "XTLS/Xray-core", Asset: "Xray",
		VersionRE: regexp.MustCompile(`(?m)^Xray\s+v?([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?)\b`),
		Services:  []string{"HandlerService", "LoggerService", "StatsService", "RoutingService"},
		Features: []string{
			core.CapabilityLoggingLevel,
			core.CapabilityDNSLocalUpstream, core.CapabilityDNSRemoteUpstream, core.CapabilityDNSBootstrapUpstream,
			core.CapabilityDNSPreferIPv4, core.CapabilityDNSRemoteServerName, core.CapabilityDNSSplit, core.CapabilityDNSNative,
			core.CapabilityRoutingRules, core.CapabilityRoutingSelector, core.CapabilityRoutingURLTest,
			core.CapabilityLocalProxy, core.CapabilityTransparentTUN, core.CapabilityTransparentTUNAddress,
			core.CapabilityNativeOverride,
		},
		Protocols: []core.ProtocolCapability{
			{Protocol: "http", Transports: []string{"tcp"}, Security: []string{"none", "tls"}},
			{Protocol: "shadowsocks", Transports: []string{"tcp", "udp"}, Security: []string{"cipher"}},
			{Protocol: "socks5", Transports: []string{"tcp", "udp"}, Security: []string{"none"}},
			{Protocol: "trojan", Transports: []string{"tcp", "ws", "grpc"}, Security: []string{"tls", "reality"}},
			{Protocol: "vless", Transports: []string{"tcp", "ws", "http", "grpc"}, Security: []string{"none", "tls", "reality"}},
			{Protocol: "vmess", Transports: []string{"tcp", "ws", "http", "grpc"}, Security: []string{"none", "tls"}},
		},
	})
}
