package transparentproxy

import (
	"fmt"
	"net/netip"
	"sort"
	"strings"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func resolveTUNAddress(explicit string, occupied []string) (string, error) {
	occupiedPrefixes := parsePrefixes(occupied)
	if explicit != "" {
		prefix, err := netip.ParsePrefix(explicit)
		if err != nil || !prefix.Addr().Is4() || prefix.Bits() != 30 {
			return "", fmt.Errorf("TUN address %q must be an IPv4 /30 prefix", explicit)
		}
		if prefixOverlapsAny(prefix.Masked(), occupiedPrefixes) {
			return "", fmt.Errorf("TUN address %s conflicts with an existing address or route", explicit)
		}
		return explicit, nil
	}
	for _, base := range []string{"172.19.0.0/16", "172.20.0.0/14", "172.24.0.0/13", "198.18.0.0/15"} {
		pool := netip.MustParsePrefix(base)
		address := pool.Masked().Addr()
		for pool.Contains(address) {
			candidate := netip.PrefixFrom(address, 30)
			if !prefixOverlapsAny(candidate, occupiedPrefixes) {
				return candidate.Addr().Next().String() + "/30", nil
			}
			for range 4 {
				address = address.Next()
			}
		}
	}
	return "", fmt.Errorf("no non-conflicting IPv4 /30 is available for the sing-box TUN")
}

func prefixOverlapsAny(candidate netip.Prefix, prefixes []netip.Prefix) bool {
	for _, current := range prefixes {
		if candidate.Addr().BitLen() == current.Addr().BitLen() &&
			(candidate.Contains(current.Addr()) || current.Contains(candidate.Addr())) {
			return true
		}
	}
	return false
}

func parsePrefixes(values []string) []netip.Prefix {
	result := make([]netip.Prefix, 0, len(values))
	for _, value := range values {
		if prefix, err := netip.ParsePrefix(value); err == nil {
			result = append(result, prefix.Masked())
		}
	}
	return result
}

func normalizedPrefixes(values []string) []string {
	prefixes := parsePrefixes(values)
	sort.Slice(prefixes, func(left, right int) bool {
		if prefixes[left].Addr().BitLen() != prefixes[right].Addr().BitLen() {
			return prefixes[left].Addr().BitLen() < prefixes[right].Addr().BitLen()
		}
		if prefixes[left].Addr() != prefixes[right].Addr() {
			return prefixes[left].Addr().Less(prefixes[right].Addr())
		}
		return prefixes[left].Bits() < prefixes[right].Bits()
	})
	result := make([]string, 0, len(prefixes))
	for _, prefix := range prefixes {
		if prefix.Bits() == 0 {
			continue
		}
		covered := false
		for _, existing := range parsePrefixes(result) {
			if existing.Addr().BitLen() == prefix.Addr().BitLen() && existing.Contains(prefix.Addr()) {
				covered = true
				break
			}
		}
		if !covered {
			result = append(result, prefix.String())
		}
	}
	return result
}

func tunRouteExclusions(config subscriptions.TransparentProxyConfig, inventory Inventory, fakeIPPrefixes []string) []string {
	exclusions := append([]string{}, config.RouteExclusions...)
	if config.AutoExcludeLocalRoutes {
		exclusions = append(exclusions, inventory.LocalPrefixes...)
	}
	if config.AutoExcludeVPNRoutes {
		exclusions = append(exclusions, inventory.VPNPrefixes...)
	}
	return filterFakeIPRouteExclusions(exclusions, fakeIPPrefixes)
}

func filterFakeIPRouteExclusions(exclusions, fakeIPPrefixes []string) []string {
	normalized := normalizedPrefixes(exclusions)
	fakeIPs := parsePrefixes(fakeIPPrefixes)
	if len(fakeIPs) == 0 {
		return normalized
	}
	result := make([]string, 0, len(normalized))
	for _, exclusion := range normalized {
		prefix, err := netip.ParsePrefix(exclusion)
		if err != nil || prefixOverlapsAny(prefix.Masked(), fakeIPs) {
			continue
		}
		result = append(result, exclusion)
	}
	return result
}

func fakeIPPrefixesForCore(coreID string, document map[string]any) []string {
	switch coreID {
	case "sing-box":
		return singBoxFakeIPPrefixes(document)
	case "mihomo", "clash-rs":
		return mihomoFakeIPPrefixes(document)
	default:
		return nil
	}
}

func singBoxFakeIPPrefixes(document map[string]any) []string {
	dns := object(document["dns"])
	prefixes := []string{}
	servers, _ := dns["servers"].([]any)
	for _, value := range servers {
		server, _ := value.(map[string]any)
		if server["type"] != "fakeip" {
			continue
		}
		prefixes = append(prefixes, stringValue(server["inet4_range"]), stringValue(server["inet6_range"]))
	}
	fakeIP := object(dns["fakeip"])
	if len(fakeIP) > 0 && fakeIP["enabled"] != false {
		prefixes = append(prefixes, stringValue(fakeIP["inet4_range"]), stringValue(fakeIP["inet6_range"]))
	}
	return normalizedPrefixes(prefixes)
}

func mihomoFakeIPPrefixes(document map[string]any) []string {
	dns := object(document["dns"])
	if dns["enhanced-mode"] != "fake-ip" {
		return nil
	}
	return normalizedPrefixes([]string{
		stringValue(dns["fake-ip-range"]),
		stringValue(dns["fake-ip-range6"]),
	})
}

func runtimeRouteExclusions(coreID string, document map[string]any) []string {
	switch coreID {
	case "sing-box":
		inbound, err := findInbound(document, "tun-in", "tun")
		if err != nil {
			return nil
		}
		return normalizedPrefixes(stringValues(inbound["route_exclude_address"]))
	case "mihomo", "clash-rs":
		tun := object(document["tun"])
		return normalizedPrefixes(stringValues(tun["route-exclude-address"]))
	case "xray":
		return xrayRuntimeRouteExclusions(document)
	default:
		return nil
	}
}

func xrayRuntimeRouteExclusions(document map[string]any) []string {
	routing := object(document["routing"])
	rules, _ := routing["rules"].([]any)
	exclusions := []string{}
	for _, value := range rules {
		rule, _ := value.(map[string]any)
		if rule["outboundTag"] == "direct" {
			exclusions = append(exclusions, stringValues(rule["ip"])...)
		}
	}
	return normalizedPrefixes(exclusions)
}

func fakeIPDiagnostics(plan Plan) []Diagnostic {
	if len(plan.FakeIPPrefixes) == 0 {
		return nil
	}
	diagnostics := []Diagnostic{}
	if conflicts := fakeIPRouteExclusionConflicts(plan.RouteExclusions, plan.FakeIPPrefixes); len(conflicts) > 0 {
		diagnostics = append(diagnostics, Diagnostic{
			Name: "Linux fake-ip route capture",
			Err:  fmt.Errorf("fake-ip ranges must not be excluded from TUN capture: %s", strings.Join(conflicts, ", ")),
		})
	}
	if len(plan.FakeIPConflicts) > 0 {
		diagnostics = append(diagnostics, Diagnostic{
			Name:    "Linux fake-ip route overlap",
			Err:     fmt.Errorf("fake-ip ranges overlap local or VPN routes; Sempre ignores matching TUN exclusions and relies on core auto-redirect/fwmark capture: %s", strings.Join(plan.FakeIPConflicts, ", ")),
			Warning: true,
		})
	}
	return diagnostics
}

func fakeIPRouteExclusionConflicts(exclusions, fakeIPPrefixes []string) []string {
	return overlappingPrefixDetails(fakeIPPrefixes, exclusions, "excluded route")
}

func fakeIPRouteConflicts(fakeIPPrefixes []string, inventory Inventory) []string {
	result := overlappingPrefixDetails(fakeIPPrefixes, inventory.LocalPrefixes, "local route")
	result = append(result, overlappingPrefixDetails(fakeIPPrefixes, inventory.VPNPrefixes, "VPN route")...)
	sort.Strings(result)
	return result
}

func overlappingPrefixDetails(leftPrefixes, rightPrefixes []string, rightLabel string) []string {
	left := parsePrefixes(leftPrefixes)
	result := []string{}
	seen := map[string]bool{}
	for _, value := range rightPrefixes {
		right, err := netip.ParsePrefix(value)
		if err != nil || !prefixOverlapsAny(right.Masked(), left) {
			continue
		}
		for _, fakeIP := range left {
			if !prefixOverlapsAny(fakeIP, []netip.Prefix{right.Masked()}) {
				continue
			}
			detail := fmt.Sprintf("%s overlaps %s %s", fakeIP, rightLabel, value)
			if !seen[detail] {
				result = append(result, detail)
				seen[detail] = true
			}
		}
	}
	sort.Strings(result)
	return result
}

func outboundServerPrefixes(document map[string]any) []string {
	values, _ := document["outbounds"].([]any)
	result := []string{}
	for _, value := range values {
		outbound, _ := value.(map[string]any)
		server, _ := outbound["server"].(string)
		if address, err := netip.ParseAddr(strings.TrimSpace(server)); err == nil {
			result = append(result, netip.PrefixFrom(address, address.BitLen()).String())
		}
	}
	return result
}

func v2RayServerPrefixes(document map[string]any) []string {
	values, _ := document["outbounds"].([]any)
	result := []string{}
	for _, value := range values {
		outbound, _ := value.(map[string]any)
		settings := object(outbound["settings"])
		if address, err := netip.ParseAddr(strings.TrimSpace(stringValue(settings["address"]))); err == nil {
			result = append(result, netip.PrefixFrom(address, address.BitLen()).String())
		}
		for _, collection := range []string{"servers", "vnext"} {
			servers, _ := settings[collection].([]any)
			for _, serverValue := range servers {
				server, _ := serverValue.(map[string]any)
				if address, err := netip.ParseAddr(strings.TrimSpace(stringValue(server["address"]))); err == nil {
					result = append(result, netip.PrefixFrom(address, address.BitLen()).String())
				}
			}
		}
	}
	return result
}
