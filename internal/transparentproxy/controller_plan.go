package transparentproxy

import (
	"fmt"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func prepareTUN(
	plan Plan,
	config subscriptions.TransparentProxyConfig,
	inventory Inventory,
	document map[string]any,
	fakeIPPrefixes []string,
) (Plan, error) {
	address, err := resolveTUNAddress(config.TUN.Address, inventory.OccupiedPrefixes)
	if err != nil {
		return Plan{}, err
	}
	exclusions := tunRouteExclusions(config, inventory, fakeIPPrefixes)
	inbound, err := findInbound(document, "tun-in", "tun")
	if err != nil {
		return Plan{}, err
	}
	inbound["interface_name"] = config.TUN.InterfaceName
	inbound["address"] = []string{address}
	inbound["auto_route"] = true
	inbound["auto_redirect"] = true
	inbound["strict_route"] = true
	inbound["stack"] = "system"
	delete(inbound, "include_interface")
	delete(inbound, "exclude_interface")
	if config.InterfaceMode == "include" {
		inbound["include_interface"] = config.Interfaces
	} else if config.InterfaceMode == "exclude" {
		inbound["exclude_interface"] = config.Interfaces
	}
	if len(exclusions) > 0 {
		inbound["route_exclude_address"] = exclusions
	} else {
		delete(inbound, "route_exclude_address")
	}
	route := object(document["route"])
	route["auto_detect_interface"] = true
	document["route"] = route
	plan.TUNInterface = config.TUN.InterfaceName
	plan.TUNAddress = address
	plan.RouteExclusions = exclusions
	plan.FakeIPPrefixes = fakeIPPrefixes
	plan.FakeIPConflicts = fakeIPRouteConflicts(fakeIPPrefixes, inventory)
	plan.LANInterfaces = append([]string{}, inventory.RecommendedLANInterfaces...)
	return plan, nil
}

func prepareTProxy(
	plan Plan,
	config subscriptions.TransparentProxyConfig,
	inventory Inventory,
	document map[string]any,
) (Plan, error) {
	if _, err := findInbound(document, "tproxy-in", "tproxy"); err != nil {
		return Plan{}, err
	}
	if _, err := findInbound(document, "dns-in", "direct"); err != nil {
		return Plan{}, err
	}
	interfaces, err := resolveLANInterfaces(config.LANInterfaces, inventory)
	if err != nil {
		return Plan{}, err
	}
	if len(interfaces) == 0 && !config.CaptureHost {
		return Plan{}, fmt.Errorf("TProxy mode needs a LAN interface or capture_host enabled")
	}
	route := object(document["route"])
	route["default_mark"] = BypassMark
	route["auto_detect_interface"] = true
	document["route"] = route
	plan.TProxyPort = config.TProxy.ListenPort
	plan.DNSPort = config.TProxy.DNSListenPort
	plan.CaptureHost = config.CaptureHost
	plan.LANInterfaces = interfaces
	plan.ExcludedPrefixes = normalizedPrefixes(append(reservedPrefixes(), inventory.LocalPrefixes...))
	plan.ExcludedPrefixes = normalizedPrefixes(append(plan.ExcludedPrefixes, outboundServerPrefixes(document)...))
	return plan, nil
}

func prepareMihomoTUN(
	plan Plan,
	config subscriptions.TransparentProxyConfig,
	inventory Inventory,
	document map[string]any,
	fakeIPPrefixes []string,
) (Plan, error) {
	exclusions := tunRouteExclusions(config, inventory, fakeIPPrefixes)
	tun := object(document["tun"])
	tun["enable"] = true
	tun["device"] = config.TUN.InterfaceName
	tun["stack"] = "system"
	tun["auto-route"] = true
	tun["auto-redirect"] = true
	tun["strict-route"] = true
	tun["auto-detect-interface"] = true
	tun["dns-hijack"] = []string{"any:53", "tcp://any:53"}
	if len(exclusions) > 0 {
		tun["route-exclude-address"] = exclusions
	} else {
		delete(tun, "route-exclude-address")
	}
	delete(tun, "include-interface")
	delete(tun, "exclude-interface")
	if config.InterfaceMode == "include" {
		tun["include-interface"] = config.Interfaces
	} else if config.InterfaceMode == "exclude" {
		tun["exclude-interface"] = config.Interfaces
	}
	document["tun"] = tun
	plan.TUNInterface = config.TUN.InterfaceName
	plan.RouteExclusions = exclusions
	plan.FakeIPPrefixes = fakeIPPrefixes
	plan.FakeIPConflicts = fakeIPRouteConflicts(fakeIPPrefixes, inventory)
	plan.LANInterfaces = append([]string{}, inventory.RecommendedLANInterfaces...)
	return plan, nil
}

func prepareClashRSTUN(
	plan Plan,
	config subscriptions.TransparentProxyConfig,
	inventory Inventory,
	document map[string]any,
	fakeIPPrefixes []string,
) (Plan, error) {
	address, err := resolveTUNAddress(config.TUN.Address, inventory.OccupiedPrefixes)
	if err != nil {
		return Plan{}, err
	}
	exclusions := tunRouteExclusions(config, inventory, fakeIPPrefixes)
	tun := object(document["tun"])
	tun["enable"] = true
	tun["device"] = config.TUN.InterfaceName
	tun["gateway"] = address
	tun["route-all"] = true
	tun["dns-hijack"] = true
	document["tun"] = tun
	plan.TUNInterface = config.TUN.InterfaceName
	plan.TUNAddress = address
	plan.RouteExclusions = exclusions
	plan.FakeIPPrefixes = fakeIPPrefixes
	plan.FakeIPConflicts = fakeIPRouteConflicts(fakeIPPrefixes, inventory)
	plan.LANInterfaces = append([]string{}, inventory.RecommendedLANInterfaces...)
	return plan, nil
}

func prepareMihomoTProxy(
	plan Plan,
	config subscriptions.TransparentProxyConfig,
	inventory Inventory,
	document map[string]any,
) (Plan, error) {
	if integer(document["tproxy-port"]) != config.TProxy.ListenPort {
		return Plan{}, fmt.Errorf("runtime configuration is missing tproxy-port %d", config.TProxy.ListenPort)
	}
	if !mihomoDNSListener(document, config.TProxy.DNSListenPort) {
		return Plan{}, fmt.Errorf("runtime configuration is missing DNS TProxy listener %d", config.TProxy.DNSListenPort)
	}
	interfaces, err := resolveLANInterfaces(config.LANInterfaces, inventory)
	if err != nil {
		return Plan{}, err
	}
	if len(interfaces) == 0 && !config.CaptureHost {
		return Plan{}, fmt.Errorf("TProxy mode needs a LAN interface or capture_host enabled")
	}
	document["routing-mark"] = BypassMark
	plan.TProxyPort = config.TProxy.ListenPort
	plan.DNSPort = config.TProxy.DNSListenPort
	plan.CaptureHost = config.CaptureHost
	plan.LANInterfaces = interfaces
	plan.ExcludedPrefixes = normalizedPrefixes(append(reservedPrefixes(), inventory.LocalPrefixes...))
	plan.ExcludedPrefixes = normalizedPrefixes(append(plan.ExcludedPrefixes, mihomoServerPrefixes(document)...))
	return plan, nil
}

func prepareXrayTUN(
	plan Plan,
	config subscriptions.TransparentProxyConfig,
	inventory Inventory,
	document map[string]any,
	fakeIPPrefixes []string,
) (Plan, error) {
	address, err := resolveTUNAddress(config.TUN.Address, inventory.OccupiedPrefixes)
	if err != nil {
		return Plan{}, err
	}
	inbound, err := findProtocolInbound(document, "tun-in", "tun")
	if err != nil {
		return Plan{}, err
	}
	settings := object(inbound["settings"])
	settings["name"] = config.TUN.InterfaceName
	settings["gateway"] = []string{address}
	settings["autoSystemRoutingTable"] = []string{"0.0.0.0/0", "::/0"}
	settings["autoOutboundsInterface"] = "auto"
	inbound["settings"] = settings
	exclusions := tunRouteExclusions(config, inventory, fakeIPPrefixes)
	if len(exclusions) > 0 {
		routing := object(document["routing"])
		rules, _ := routing["rules"].([]any)
		rules = append([]any{map[string]any{"type": "field", "ip": exclusions, "outboundTag": "direct"}}, rules...)
		routing["rules"] = rules
		document["routing"] = routing
	}
	plan.TUNInterface = config.TUN.InterfaceName
	plan.TUNAddress = address
	plan.RouteExclusions = exclusions
	plan.FakeIPPrefixes = fakeIPPrefixes
	plan.FakeIPConflicts = fakeIPRouteConflicts(fakeIPPrefixes, inventory)
	plan.LANInterfaces = append([]string{}, inventory.RecommendedLANInterfaces...)
	return plan, nil
}

func prepareV2RayTProxy(
	plan Plan,
	config subscriptions.TransparentProxyConfig,
	inventory Inventory,
	document map[string]any,
) (Plan, error) {
	if _, err := findProtocolInbound(document, "tproxy-in", "dokodemo-door"); err != nil {
		return Plan{}, err
	}
	if _, err := findProtocolInbound(document, "dns-in", "dokodemo-door"); err != nil {
		return Plan{}, err
	}
	interfaces, err := resolveLANInterfaces(config.LANInterfaces, inventory)
	if err != nil {
		return Plan{}, err
	}
	if len(interfaces) == 0 && !config.CaptureHost {
		return Plan{}, fmt.Errorf("TProxy mode needs a LAN interface or capture_host enabled")
	}
	values, _ := document["outbounds"].([]any)
	for _, value := range values {
		outbound, _ := value.(map[string]any)
		stream := object(outbound["streamSettings"])
		sockopt := object(stream["sockopt"])
		sockopt["mark"] = BypassMark
		stream["sockopt"] = sockopt
		outbound["streamSettings"] = stream
	}
	plan.TProxyPort = config.TProxy.ListenPort
	plan.DNSPort = config.TProxy.DNSListenPort
	plan.CaptureHost = config.CaptureHost
	plan.LANInterfaces = interfaces
	plan.ExcludedPrefixes = normalizedPrefixes(append(reservedPrefixes(), inventory.LocalPrefixes...))
	plan.ExcludedPrefixes = normalizedPrefixes(append(plan.ExcludedPrefixes, v2RayServerPrefixes(document)...))
	return plan, nil
}
