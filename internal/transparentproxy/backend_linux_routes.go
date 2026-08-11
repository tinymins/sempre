//go:build linux

package transparentproxy

import (
	"encoding/binary"
	"errors"
	"fmt"
	"net"
	"strings"
	"syscall"

	"github.com/vishvananda/netlink"
	"golang.org/x/sys/unix"
)

func addPolicyRoutes() error {
	loopback, err := netlink.LinkByName("lo")
	if err != nil {
		return err
	}
	mask := uint32(0xffffffff)
	for _, family := range []int{netlink.FAMILY_V4, netlink.FAMILY_V6} {
		destination := defaultNetwork(family)
		route := netlink.Route{
			LinkIndex: loopback.Attrs().Index,
			Scope:     netlink.SCOPE_HOST,
			Dst:       destination,
			Family:    family,
			Table:     RouteTable,
			Type:      unix.RTN_LOCAL,
			Protocol:  netlink.RouteProtocol(PolicyProtocol),
		}
		if err := netlink.RouteAdd(&route); err != nil {
			return err
		}
		rule := netlink.NewRule()
		rule.Family = family
		rule.Priority = RulePriority
		rule.Table = RouteTable
		rule.Mark = RouteMark
		rule.Mask = &mask
		rule.Protocol = PolicyProtocol
		if err := netlink.RuleAdd(rule); err != nil {
			return err
		}
	}
	return nil
}

func deletePolicyRoutes() error {
	var failures []error
	mask := uint32(0xffffffff)
	for _, family := range []int{netlink.FAMILY_V4, netlink.FAMILY_V6} {
		rules, err := netlink.RuleList(family)
		if err != nil {
			failures = append(failures, err)
		} else {
			for _, rule := range rules {
				if ownedPolicyRule(rule, &mask) {
					current := rule
					if deleteErr := netlink.RuleDel(&current); deleteErr != nil && !isMissingKernelObject(deleteErr) {
						failures = append(failures, deleteErr)
					}
				}
			}
		}
		routes, err := netlink.RouteListFiltered(family, &netlink.Route{Table: RouteTable}, netlink.RT_FILTER_TABLE)
		if err != nil {
			failures = append(failures, err)
		} else {
			for _, route := range routes {
				if ownedPolicyRoute(route, family) {
					current := route
					if deleteErr := netlink.RouteDel(&current); deleteErr != nil && !isMissingKernelObject(deleteErr) {
						failures = append(failures, deleteErr)
					}
				}
			}
		}
	}
	return errors.Join(failures...)
}

func verifyPolicyRoutes() error {
	mask := uint32(0xffffffff)
	for _, family := range []int{netlink.FAMILY_V4, netlink.FAMILY_V6} {
		rules, err := netlink.RuleList(family)
		if err != nil {
			return err
		}
		foundRule := false
		for _, rule := range rules {
			if ownedPolicyRule(rule, &mask) {
				foundRule = true
			}
		}
		if !foundRule {
			return fmt.Errorf("policy rule for mark %#x and table %d is missing", RouteMark, RouteTable)
		}
		routes, err := netlink.RouteListFiltered(family, &netlink.Route{Table: RouteTable}, netlink.RT_FILTER_TABLE)
		if err != nil {
			return err
		}
		foundRoute := false
		for _, route := range routes {
			if ownedPolicyRoute(route, family) {
				foundRoute = true
			}
		}
		if !foundRoute {
			return fmt.Errorf("local default route in table %d is missing", RouteTable)
		}
	}
	return nil
}

func checkPolicyRouteCollisions() error {
	mask := uint32(0xffffffff)
	for _, family := range []int{netlink.FAMILY_V4, netlink.FAMILY_V6} {
		rules, err := netlink.RuleList(family)
		if err != nil {
			return err
		}
		for _, rule := range rules {
			if rule.Priority == RulePriority && !ownedPolicyRule(rule, &mask) {
				return fmt.Errorf("ip rule priority %d is already used by another rule", RulePriority)
			}
			if rule.Table == RouteTable && !ownedPolicyRule(rule, &mask) {
				return fmt.Errorf("route table %d is already used by another policy rule", RouteTable)
			}
		}
		routes, err := netlink.RouteListFiltered(family, &netlink.Route{Table: RouteTable}, netlink.RT_FILTER_TABLE)
		if err != nil {
			return err
		}
		for _, route := range routes {
			if !ownedPolicyRoute(route, family) {
				return fmt.Errorf("route table %d contains routes not owned by Sempre", RouteTable)
			}
		}
	}
	return nil
}

func ownedPolicyRule(rule netlink.Rule, mask *uint32) bool {
	return rule.Priority == RulePriority && rule.Table == RouteTable && rule.Mark == RouteMark &&
		masksEqual(rule.Mask, mask) && rule.Protocol == PolicyProtocol
}

func ownedPolicyRoute(route netlink.Route, family int) bool {
	return route.Type == unix.RTN_LOCAL && route.Dst != nil && route.Dst.String() == defaultNetwork(family).String() &&
		route.Protocol == netlink.RouteProtocol(PolicyProtocol)
}

func defaultNetwork(family int) *net.IPNet {
	if family == netlink.FAMILY_V6 {
		_, network, _ := net.ParseCIDR("::/0")
		return network
	}
	_, network, _ := net.ParseCIDR("0.0.0.0/0")
	return network
}

func classifyLink(link netlink.Link) string {
	name := strings.ToLower(link.Attrs().Name)
	kind := strings.ToLower(link.Type())
	if kind == "veth" || hasAnyPrefix(name, "docker", "podman", "cni", "lxc", "veth", "virbr", "br-") {
		return "container"
	}
	if kind == "bridge" || strings.HasPrefix(name, "vmbr") {
		return "bridge"
	}
	if kind == "tun" || kind == "wireguard" || hasAnyPrefix(name, "tun", "tap", "wg", "tailscale", "zt", "vpn") {
		return "vpn"
	}
	if link.Attrs().Flags&net.FlagLoopback != 0 {
		return "loopback"
	}
	return "physical"
}

func lanInterfaceScore(name string, kinds map[int]string, links []netlink.Link) int {
	score := 0
	if strings.HasPrefix(strings.ToLower(name), "vmbr") {
		score += 20
	}
	for _, link := range links {
		if link.Attrs().Name == name && kinds[link.Attrs().Index] == "bridge" {
			score += 10
		}
	}
	return score
}

func linkByIndex(links []netlink.Link, index int) netlink.Link {
	for _, link := range links {
		if link.Attrs().Index == index {
			return link
		}
	}
	return nil
}

func hasAnyPrefix(value string, prefixes ...string) bool {
	for _, prefix := range prefixes {
		if strings.HasPrefix(value, prefix) {
			return true
		}
	}
	return false
}

func masksEqual(left, right *uint32) bool {
	if left == nil || right == nil {
		return left == right
	}
	return *left == *right
}

func isMissingKernelObject(err error) bool {
	return errors.Is(err, syscall.ENOENT) || errors.Is(err, syscall.ESRCH)
}

func interfaceBytes(name string) []byte {
	result := make([]byte, unix.IFNAMSIZ)
	copy(result, name)
	return result
}

func uint16Bytes(value uint16) []byte {
	result := make([]byte, 2)
	binary.BigEndian.PutUint16(result, value)
	return result
}

func uint32Bytes(value uint32) []byte {
	result := make([]byte, 4)
	binary.NativeEndian.PutUint32(result, value)
	return result
}
