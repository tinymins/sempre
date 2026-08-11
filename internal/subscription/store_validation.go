package subscription

import (
	"fmt"
	"net"
	"net/netip"
	"sort"
	"strings"
)

func validateTransparentProxy(config TransparentProxyConfig) error {
	switch config.Mode {
	case TransparentProxyTUN, TransparentProxyTProxy, TransparentProxyEBPF, TransparentProxyDisabled:
	default:
		return fmt.Errorf("unsupported transparent proxy mode %q", config.Mode)
	}
	if strings.TrimSpace(config.TUN.InterfaceName) == "" || len(config.TUN.InterfaceName) > 15 {
		return fmt.Errorf("TUN interface name must contain 1 to 15 characters")
	}
	if config.TUN.Address != "" {
		prefix, err := netip.ParsePrefix(config.TUN.Address)
		if err != nil || !prefix.Addr().Is4() || prefix.Bits() != 30 {
			return fmt.Errorf("TUN address must be an IPv4 /30 prefix")
		}
	}
	for _, value := range config.RouteExclusions {
		if _, err := netip.ParsePrefix(value); err != nil {
			return fmt.Errorf("invalid TUN route exclusion %q", value)
		}
	}
	switch config.InterfaceMode {
	case "all", "include", "exclude":
	default:
		return fmt.Errorf("TUN interface mode must be all, include, or exclude")
	}
	interfaceNames := map[string]bool{}
	for _, name := range config.Interfaces {
		name = strings.TrimSpace(name)
		if name == "" || len(name) > 15 || interfaceNames[name] {
			return fmt.Errorf("TUN interfaces must be unique valid interface names")
		}
		interfaceNames[name] = true
	}
	if config.InterfaceMode != "all" && len(config.Interfaces) == 0 {
		return fmt.Errorf("transparent interface mode %s requires at least one interface", config.InterfaceMode)
	}
	if config.TProxy.ListenPort < 1 || config.TProxy.ListenPort > 65535 || config.TProxy.DNSListenPort < 1 || config.TProxy.DNSListenPort > 65535 {
		return fmt.Errorf("transparent proxy ports must be between 1 and 65535")
	}
	seen := map[string]bool{}
	for _, name := range config.LANInterfaces {
		name = strings.TrimSpace(name)
		if name == "" || len(name) > 15 || seen[name] {
			return fmt.Errorf("TProxy LAN interfaces must be unique valid interface names")
		}
		seen[name] = true
	}
	if config.Mode == TransparentProxyEBPF && strings.TrimSpace(config.EBPF.WANInterface) == "" {
		return fmt.Errorf("eBPF router WAN interface is required")
	}
	return nil
}

func validateLocalProxy(config LocalProxyConfig, transparent TransparentProxyConfig) error {
	if config.SOCKSPort < 1 || config.SOCKSPort > 65535 || config.HTTPPort < 1 || config.HTTPPort > 65535 {
		return fmt.Errorf("local proxy ports must be between 1 and 65535")
	}
	if config.SOCKSPort == config.HTTPPort {
		return fmt.Errorf("local SOCKS and HTTP proxy ports must be different")
	}
	if strings.TrimSpace(config.Username) == "" || strings.TrimSpace(config.Password) == "" {
		return fmt.Errorf("local proxy username and password are required")
	}
	if transparent.Mode == TransparentProxyTProxy {
		for _, port := range []int{transparent.TProxy.ListenPort, transparent.TProxy.DNSListenPort} {
			if config.SOCKSPort == port || config.HTTPPort == port {
				return fmt.Errorf("local proxy ports must not conflict with transparent proxy ports")
			}
		}
	}
	return nil
}

func validateManagementAPI(config ManagementAPIConfig) error {
	if strings.TrimSpace(config.ExternalController) == "" {
		return fmt.Errorf("external management API controller is required")
	}
	host, port, err := net.SplitHostPort(config.ExternalController)
	if err != nil || strings.TrimSpace(host) == "" || strings.TrimSpace(port) == "" {
		return fmt.Errorf("external management API controller must use host:port syntax")
	}
	if strings.TrimSpace(config.Secret) == "" {
		return fmt.Errorf("external management API secret is required")
	}
	return nil
}

func configuredMember(values []string, expected string) bool {
	for _, value := range values {
		if value == expected {
			return true
		}
	}
	return false
}

func numberValue(value any) (int, bool) {
	switch typed := value.(type) {
	case float64:
		return int(typed), typed == float64(int(typed))
	case int:
		return typed, true
	default:
		return 0, false
	}
}

func ValidateCatalog(catalog Catalog) error {
	return validateCatalog(catalog)
}

func FindProfile(catalog *Catalog, id string) (*Profile, error) {
	for index := range catalog.Profiles {
		if catalog.Profiles[index].ID == id {
			return &catalog.Profiles[index], nil
		}
	}
	return nil, fmt.Errorf("subscription profile %q was not found", id)
}

func SortProfiles(profiles []Profile) {
	sort.SliceStable(profiles, func(i, j int) bool { return profiles[i].Name < profiles[j].Name })
}
