package transparentproxy

import (
	"fmt"
	"net/netip"
	"sort"
	"strings"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func validateV2RayRuntimePlan(plan Plan, document map[string]any) error {
	if plan.Mode == subscriptions.TransparentProxyTUN {
		inbound, err := findProtocolInbound(document, "tun-in", "tun")
		if err != nil {
			return err
		}
		settings := object(inbound["settings"])
		if settings["name"] != plan.TUNInterface || firstString(settings["gateway"]) == "" || firstString(settings["autoSystemRoutingTable"]) == "" || settings["autoOutboundsInterface"] != "auto" {
			return fmt.Errorf("Xray TUN automatic routing is incomplete")
		}
		return nil
	}
	if _, err := findProtocolInbound(document, "tproxy-in", "dokodemo-door"); err != nil {
		return err
	}
	if _, err := findProtocolInbound(document, "dns-in", "dokodemo-door"); err != nil {
		return err
	}
	values, _ := document["outbounds"].([]any)
	for _, value := range values {
		outbound, _ := value.(map[string]any)
		if number, ok := numberAsUint32(object(object(outbound["streamSettings"])["sockopt"])["mark"]); !ok || number != BypassMark {
			return fmt.Errorf("outbound %v does not use the Sempre bypass mark", outbound["tag"])
		}
	}
	return nil
}

func validateV2RaySplitDNS(document map[string]any) error {
	dns := object(document["dns"])
	servers, _ := dns["servers"].([]any)
	hasLocal, hasRemote := false, false
	for _, value := range servers {
		server, _ := value.(map[string]any)
		switch server["tag"] {
		case "local-dns":
			hasLocal = stringListContains(server["domains"], "geosite:cn")
		case "remote-dns":
			hasRemote = strings.HasPrefix(stringValue(server["address"]), "https://")
		}
	}
	if !hasLocal || !hasRemote {
		return fmt.Errorf("domestic local DNS or foreign DoH is missing")
	}
	return nil
}

func validateV2RaySplitRouting(document map[string]any) error {
	routing := object(document["routing"])
	rules, _ := routing["rules"].([]any)
	hasDomestic, hasForeign := false, false
	for _, value := range rules {
		rule, _ := value.(map[string]any)
		if rule["outboundTag"] == "direct" && (stringListContains(rule["domain"], "geosite:cn") || stringListContains(rule["ip"], "geoip:cn")) {
			hasDomestic = true
		}
		if rule["balancerTag"] != nil && (rule["network"] == "tcp,udp" || stringListContains(rule["inboundTag"], "remote-dns")) {
			hasForeign = true
		}
	}
	if !hasDomestic || !hasForeign {
		return fmt.Errorf("domestic direct or foreign balancer route is missing")
	}
	return nil
}

func stringValue(value any) string {
	result, _ := value.(string)
	return result
}

func mihomoServerPrefixes(document map[string]any) []string {
	values, _ := document["proxies"].([]any)
	result := []string{}
	for _, value := range values {
		proxy, _ := value.(map[string]any)
		server, _ := proxy["server"].(string)
		if address, err := netip.ParseAddr(strings.TrimSpace(server)); err == nil {
			result = append(result, netip.PrefixFrom(address, address.BitLen()).String())
		}
	}
	return result
}

func reservedPrefixes() []string {
	return []string{
		"0.0.0.0/8", "10.0.0.0/8", "100.64.0.0/10", "127.0.0.0/8",
		"169.254.0.0/16", "172.16.0.0/12", "192.0.0.0/24", "192.168.0.0/16",
		"224.0.0.0/4", "240.0.0.0/4", "::1/128", "fc00::/7", "fe80::/10", "ff00::/8",
	}
}

func uniqueStrings(values []string) []string {
	seen := map[string]bool{}
	result := make([]string, 0, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value != "" && !seen[value] {
			seen[value] = true
			result = append(result, value)
		}
	}
	sort.Strings(result)
	return result
}

func object(value any) map[string]any {
	if result, ok := value.(map[string]any); ok {
		return result
	}
	return map[string]any{}
}

func firstString(value any) string {
	switch values := value.(type) {
	case []any:
		if len(values) > 0 {
			result, _ := values[0].(string)
			return result
		}
	case []string:
		if len(values) > 0 {
			return values[0]
		}
	case string:
		return values
	}
	return ""
}

func numberAsUint32(value any) (uint32, bool) {
	switch number := value.(type) {
	case float64:
		if number >= 0 && number <= float64(^uint32(0)) && number == float64(uint32(number)) {
			return uint32(number), true
		}
	case uint32:
		return number, true
	case int:
		if number >= 0 && uint64(number) <= uint64(^uint32(0)) {
			return uint32(number), true
		}
	}
	return 0, false
}

func integer(value any) int {
	switch number := value.(type) {
	case int:
		return number
	case uint32:
		return int(number)
	case float64:
		return int(number)
	case uint64:
		return int(number)
	default:
		return 0
	}
}

func stringValues(value any) []string {
	switch values := value.(type) {
	case []string:
		return values
	case []any:
		result := make([]string, 0, len(values))
		for _, value := range values {
			if text, ok := value.(string); ok {
				result = append(result, text)
			}
		}
		return result
	case string:
		return []string{values}
	default:
		return nil
	}
}

func stringListContains(value any, expected string) bool {
	switch values := value.(type) {
	case []any:
		for _, item := range values {
			if item == expected {
				return true
			}
		}
	case []string:
		for _, item := range values {
			if item == expected {
				return true
			}
		}
	}
	return false
}
