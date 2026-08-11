package transparentproxy

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/netip"
	"strings"
	"time"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"gopkg.in/yaml.v3"
)

func resolveLANInterfaces(configured []string, inventory Inventory) ([]string, error) {
	interfaces := append([]string{}, configured...)
	if len(interfaces) == 0 {
		interfaces = append(interfaces, inventory.RecommendedLANInterfaces...)
	}
	interfaces = uniqueStrings(interfaces)
	available := map[string]bool{}
	for _, current := range inventory.Interfaces {
		available[current.Name] = true
	}
	for _, name := range interfaces {
		if !available[name] {
			return nil, fmt.Errorf("configured LAN interface %q does not exist", name)
		}
	}
	return interfaces, nil
}

func systemDNSIntent(config map[string]any) (bool, int, []string) {
	shared := config
	if nested := object(config["shared"]); len(nested) > 0 {
		shared = nested
	}
	enabled, _ := shared["systemDnsTakeoverEnabled"].(bool)
	port := integer(shared["systemDnsListenPort"])
	if port == 0 {
		port = 53
	}
	return enabled, port, normalizeSystemDNSHosts(stringValues(shared["systemDnsListenHosts"]))
}

func normalizeSystemDNSHosts(values []string) []string {
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
		if !containsString(hosts, host) {
			hosts = append(hosts, host)
		}
	}
	if len(hosts) == 0 {
		return []string{"127.0.0.1"}
	}
	return hosts
}

func containsString(values []string, expected string) bool {
	for _, value := range values {
		if value == expected {
			return true
		}
	}
	return false
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

func validateSystemDNSInbounds(document map[string]any, port int, hosts []string) error {
	tags := systemDNSInboundTags(hosts)
	for index, host := range hosts {
		tag := tags[index]
		inbound, err := findInbound(document, tag, "direct")
		if err != nil {
			return err
		}
		if inbound["listen"] != host || integer(inbound["listen_port"]) != port || inbound["override_address"] != "1.1.1.1" || integer(inbound["override_port"]) != 53 {
			return fmt.Errorf("system DNS inbound %s must listen on %s:%d", tag, host, port)
		}
	}
	rules, _ := object(document["route"])["rules"].([]any)
	if !hasSystemDNSHijackRules(rules, tags) {
		return fmt.Errorf("runtime configuration is missing system DNS hijack route rules")
	}
	return nil
}

func hasSystemDNSHijackRules(rules []any, tags []string) bool {
	for _, tag := range tags {
		found := false
		for index := 0; index+1 < len(rules); index++ {
			first, ok := rules[index].(map[string]any)
			if !ok || first["inbound"] != tag || first["action"] != "sniff" {
				continue
			}
			second, ok := rules[index+1].(map[string]any)
			if ok && second["inbound"] == tag && second["protocol"] == "dns" && second["action"] == "hijack-dns" {
				found = true
				break
			}
		}
		if !found {
			return false
		}
	}
	return true
}

func findInbound(document map[string]any, tag, inboundType string) (map[string]any, error) {
	values, ok := document["inbounds"].([]any)
	if !ok {
		return nil, fmt.Errorf("runtime configuration has no inbounds")
	}
	for _, value := range values {
		inbound, ok := value.(map[string]any)
		if ok && inbound["tag"] == tag && inbound["type"] == inboundType {
			return inbound, nil
		}
	}
	return nil, fmt.Errorf("runtime configuration is missing %s %s inbound", tag, inboundType)
}

func findProtocolInbound(document map[string]any, tag, protocol string) (map[string]any, error) {
	values, ok := document["inbounds"].([]any)
	if !ok {
		return nil, fmt.Errorf("runtime configuration has no inbounds")
	}
	for _, value := range values {
		inbound, ok := value.(map[string]any)
		if ok && inbound["tag"] == tag && inbound["protocol"] == protocol {
			return inbound, nil
		}
	}
	return nil, fmt.Errorf("runtime configuration is missing %s %s inbound", tag, protocol)
}

func supportedCore(coreID string) bool {
	return coreID == "sing-box" || coreID == "mihomo" || coreID == "xray" || coreID == "v2ray" || coreID == "clash-rs"
}

func coreSupportsMode(coreID, mode string) bool {
	switch mode {
	case subscriptions.TransparentProxyDisabled:
		return true
	case subscriptions.TransparentProxyTUN:
		return coreID == "sing-box" || coreID == "mihomo" || coreID == "clash-rs" || coreID == "xray"
	case subscriptions.TransparentProxyTProxy:
		return supportedCore(coreID)
	default:
		return false
	}
}

func decodeRuntimeDocument(coreID string, data []byte) (map[string]any, error) {
	document := map[string]any{}
	if coreID == "mihomo" || coreID == "clash-rs" {
		return document, yaml.Unmarshal(data, &document)
	}
	return document, json.Unmarshal(data, &document)
}

func encodeRuntimeDocument(coreID string, document map[string]any) ([]byte, error) {
	if coreID == "mihomo" || coreID == "clash-rs" {
		return yaml.Marshal(document)
	}
	encoded, err := json.MarshalIndent(document, "", "  ")
	if err != nil {
		return nil, err
	}
	return append(encoded, '\n'), nil
}

func mihomoDNSListener(document map[string]any, port int) bool {
	values, _ := document["listeners"].([]any)
	for _, value := range values {
		listener, _ := value.(map[string]any)
		if listener["type"] == "tproxy" && integer(listener["port"]) == port {
			return true
		}
	}
	return false
}

func listenersReady(plan Plan) error {
	for _, port := range []int{plan.TProxyPort, plan.DNSPort} {
		connection, err := net.DialTimeout("tcp", net.JoinHostPort("127.0.0.1", fmt.Sprint(port)), 100*time.Millisecond)
		if err != nil {
			return fmt.Errorf("TCP port %d is not listening: %w", port, err)
		}
		_ = connection.Close()
	}
	return nil
}

func waitForTCP(ctx context.Context, port int, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for {
		connection, err := net.DialTimeout("tcp", net.JoinHostPort("127.0.0.1", fmt.Sprint(port)), 100*time.Millisecond)
		if err == nil {
			_ = connection.Close()
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("TCP port %d is not listening after %s: %w", port, timeout, err)
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(readinessPollInterval):
		}
	}
}

func waitForSystemDNS(ctx context.Context, hosts []string, port int, timeout time.Duration) error {
	hosts = normalizeSystemDNSHosts(hosts)
	for _, host := range hosts {
		if err := waitForTCPHost(ctx, systemDNSDialHost(host), port, timeout); err != nil {
			return err
		}
	}
	return nil
}

func waitForTCPHost(ctx context.Context, host string, port int, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	address := net.JoinHostPort(host, fmt.Sprint(port))
	for {
		connection, err := net.DialTimeout("tcp", address, 100*time.Millisecond)
		if err == nil {
			_ = connection.Close()
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("%s is not listening after %s: %w", address, timeout, err)
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(readinessPollInterval):
		}
	}
}

func systemDNSDialHost(host string) string {
	if host == "0.0.0.0" {
		return "127.0.0.1"
	}
	return host
}
