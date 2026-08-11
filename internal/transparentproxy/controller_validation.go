package transparentproxy

import (
	"fmt"
	"strings"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func validateRuntimePlan(plan Plan, document map[string]any) error {
	if plan.Core == "clash-rs" {
		return validateClashRSRuntimePlan(plan, document)
	}
	if plan.Core == "mihomo" {
		return validateMihomoRuntimePlan(plan, document)
	}
	if plan.Core == "xray" || plan.Core == "v2ray" {
		return validateV2RayRuntimePlan(plan, document)
	}
	route := object(document["route"])
	if route["auto_detect_interface"] != true {
		return fmt.Errorf("route.auto_detect_interface is not enabled")
	}
	if plan.Mode == subscriptions.TransparentProxyTUN {
		inbound, err := findInbound(document, "tun-in", "tun")
		if err != nil {
			return err
		}
		for _, field := range []string{"auto_route", "auto_redirect", "strict_route"} {
			if inbound[field] != true {
				return fmt.Errorf("TUN inbound %s is not enabled", field)
			}
		}
		if plan.TUNInterface == "" || plan.TUNAddress == "" || inbound["stack"] != "system" {
			return fmt.Errorf("TUN interface, address, or system stack is missing")
		}
		return nil
	}
	if _, err := findInbound(document, "tproxy-in", "tproxy"); err != nil {
		return err
	}
	if _, err := findInbound(document, "dns-in", "direct"); err != nil {
		return err
	}
	if number, ok := numberAsUint32(route["default_mark"]); !ok || number != BypassMark {
		return fmt.Errorf("route.default_mark does not bypass the Sempre TProxy capture mark")
	}
	return nil
}

func validateSplitDNS(coreID string, document map[string]any) error {
	if coreID == "mihomo" || coreID == "clash-rs" {
		dns := object(document["dns"])
		if dns["respect-rules"] != true {
			return fmt.Errorf("dns.respect-rules is not enabled")
		}
		nameservers := stringValues(dns["nameserver"])
		if len(nameservers) == 0 || !strings.Contains(nameservers[0], "#") {
			return fmt.Errorf("remote DNS is not attached to a foreign selector")
		}
		policy := object(dns["nameserver-policy"])
		if len(stringValues(policy["geosite:cn"])) == 0 {
			return fmt.Errorf("geosite:cn does not use local DNS")
		}
		return nil
	}
	if coreID == "xray" || coreID == "v2ray" {
		return validateV2RaySplitDNS(document)
	}
	dns := object(document["dns"])
	if dns["final"] != "remote" {
		return fmt.Errorf("dns.final is not remote")
	}
	servers, _ := dns["servers"].([]any)
	remoteDetour := ""
	for _, value := range servers {
		server, _ := value.(map[string]any)
		if server["tag"] == "remote" {
			remoteDetour, _ = server["detour"].(string)
		}
	}
	if remoteDetour == "" || remoteDetour == "direct" {
		return fmt.Errorf("remote DNS is not attached to a foreign selector")
	}
	rules, _ := dns["rules"].([]any)
	for _, value := range rules {
		rule, _ := value.(map[string]any)
		if rule["server"] == "local" && stringListContains(rule["rule_set"], "geosite-cn") {
			return nil
		}
	}
	return fmt.Errorf("geosite-cn does not use local DNS")
}

func validateSplitRouting(coreID string, document map[string]any) error {
	if coreID == "mihomo" || coreID == "clash-rs" {
		rules := stringValues(document["rules"])
		if len(rules) == 0 || !strings.HasPrefix(rules[len(rules)-1], "MATCH,") || strings.HasSuffix(rules[len(rules)-1], ",DIRECT") {
			return fmt.Errorf("foreign route final is not a proxy selector")
		}
		for _, rule := range rules {
			if rule == "GEOIP,CN,DIRECT,no-resolve" || rule == "GEOSITE,CN,DIRECT" {
				return nil
			}
		}
		return fmt.Errorf("China routes do not use direct")
	}
	if coreID == "xray" || coreID == "v2ray" {
		return validateV2RaySplitRouting(document)
	}
	route := object(document["route"])
	final, _ := route["final"].(string)
	if final == "" || final == "direct" {
		return fmt.Errorf("foreign route final is not a proxy selector")
	}
	rules, _ := route["rules"].([]any)
	for _, value := range rules {
		rule, _ := value.(map[string]any)
		if rule["outbound"] == "direct" && stringListContains(rule["rule_set"], "geosite-cn") {
			return nil
		}
	}
	return fmt.Errorf("geosite-cn does not route direct")
}

func validateMihomoRuntimePlan(plan Plan, document map[string]any) error {
	if plan.Mode == subscriptions.TransparentProxyTUN {
		tun := object(document["tun"])
		for _, field := range []string{"enable", "auto-route", "strict-route", "auto-detect-interface"} {
			if tun[field] != true {
				return fmt.Errorf("TUN %s is not enabled", field)
			}
		}
		if plan.TUNInterface == "" || tun["stack"] != "system" {
			return fmt.Errorf("TUN interface or system stack is missing")
		}
		return nil
	}
	if integer(document["tproxy-port"]) != plan.TProxyPort {
		return fmt.Errorf("tproxy-port does not match the managed listener")
	}
	if !mihomoDNSListener(document, plan.DNSPort) {
		return fmt.Errorf("runtime configuration is missing the managed DNS TProxy listener")
	}
	if number, ok := numberAsUint32(document["routing-mark"]); !ok || number != BypassMark {
		return fmt.Errorf("routing-mark does not bypass the Sempre TProxy capture mark")
	}
	return nil
}

func validateClashRSRuntimePlan(plan Plan, document map[string]any) error {
	if plan.Mode == subscriptions.TransparentProxyTUN {
		tun := object(document["tun"])
		if tun["enable"] != true || tun["route-all"] != true || tun["dns-hijack"] != true {
			return fmt.Errorf("clash-rs TUN automatic routing or DNS capture is incomplete")
		}
		if plan.TUNInterface == "" || plan.TUNAddress == "" {
			return fmt.Errorf("clash-rs TUN interface or gateway is missing")
		}
		return nil
	}
	return validateMihomoRuntimePlan(plan, document)
}
