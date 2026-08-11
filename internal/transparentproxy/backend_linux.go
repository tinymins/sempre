//go:build linux

package transparentproxy

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/netip"
	"os"
	"sort"
	"strconv"
	"strings"

	"github.com/google/nftables"
	"github.com/vishvananda/netlink"
)

const (
	nftTableName  = "sempre_tproxy"
	nftOwnerChain = "sempre_owner"
	nftOwnerLabel = "sempre:tproxy:owner:v1"
)

type linuxBackend struct{}

func newSystemBackend() systemBackend {
	return linuxBackend{}
}

func (linuxBackend) Supported() bool {
	return true
}

func (linuxBackend) RequirePrivileges() error {
	if os.Geteuid() != 0 {
		return fmt.Errorf("Linux transparent proxy mode requires root privileges")
	}
	return nil
}

func (linuxBackend) Inventory(ctx context.Context) (Inventory, error) {
	if err := ctx.Err(); err != nil {
		return Inventory{}, err
	}
	links, err := netlink.LinkList()
	if err != nil {
		return Inventory{}, err
	}
	routes := []netlink.Route{}
	for _, family := range []int{netlink.FAMILY_V4, netlink.FAMILY_V6} {
		current, routeErr := netlink.RouteList(nil, family)
		if routeErr != nil {
			return Inventory{}, routeErr
		}
		routes = append(routes, current...)
	}
	defaultLinks := map[int]bool{}
	defaultInterface := ""
	for _, route := range routes {
		if route.Dst == nil && route.LinkIndex > 0 {
			defaultLinks[route.LinkIndex] = true
			if defaultInterface == "" {
				if link := linkByIndex(links, route.LinkIndex); link != nil {
					defaultInterface = link.Attrs().Name
				}
			}
		}
	}
	interfaces := make([]Interface, 0, len(links))
	linkKinds := map[int]string{}
	privateAddress := map[int]bool{}
	localPrefixes := []string{}
	occupiedPrefixes := []string{}
	for _, link := range links {
		attributes := link.Attrs()
		kind := classifyLink(link)
		linkKinds[attributes.Index] = kind
		addresses, addressErr := netlink.AddrList(link, netlink.FAMILY_ALL)
		if addressErr != nil {
			return Inventory{}, addressErr
		}
		values := make([]string, 0, len(addresses))
		for _, address := range addresses {
			if address.IPNet == nil {
				continue
			}
			prefix, parseErr := netip.ParsePrefix(address.IPNet.String())
			if parseErr != nil {
				continue
			}
			values = append(values, prefix.String())
			localPrefixes = append(localPrefixes, prefix.Masked().String())
			occupiedPrefixes = append(occupiedPrefixes, prefix.Masked().String())
			if prefix.Addr().IsPrivate() {
				privateAddress[attributes.Index] = true
			}
		}
		sort.Strings(values)
		interfaces = append(interfaces, Interface{
			Name:      attributes.Name,
			Index:     attributes.Index,
			Kind:      kind,
			Up:        attributes.Flags&net.FlagUp != 0,
			Default:   defaultLinks[attributes.Index],
			Addresses: values,
		})
	}
	vpnPrefixes := []string{}
	for _, route := range routes {
		if route.Dst == nil {
			continue
		}
		prefix, parseErr := netip.ParsePrefix(route.Dst.String())
		if parseErr != nil || prefix.Bits() == 0 {
			continue
		}
		occupiedPrefixes = append(occupiedPrefixes, prefix.Masked().String())
		if route.Gw == nil || route.Gw.IsUnspecified() {
			localPrefixes = append(localPrefixes, prefix.Masked().String())
		}
		if linkKinds[route.LinkIndex] == "vpn" {
			vpnPrefixes = append(vpnPrefixes, prefix.Masked().String())
		}
	}
	sort.Slice(interfaces, func(left, right int) bool { return interfaces[left].Name < interfaces[right].Name })
	recommended := []string{}
	for _, current := range interfaces {
		if !current.Up || current.Default || !privateAddress[current.Index] {
			continue
		}
		if current.Kind == "bridge" || current.Kind == "physical" {
			recommended = append(recommended, current.Name)
		}
	}
	sort.SliceStable(recommended, func(left, right int) bool {
		return lanInterfaceScore(recommended[left], linkKinds, links) > lanInterfaceScore(recommended[right], linkKinds, links)
	})
	return Inventory{
		Interfaces:               interfaces,
		DefaultInterface:         defaultInterface,
		RecommendedLANInterfaces: recommended,
		LocalPrefixes:            normalizedPrefixes(localPrefixes),
		VPNPrefixes:              normalizedPrefixes(vpnPrefixes),
		OccupiedPrefixes:         normalizedPrefixes(occupiedPrefixes),
	}, nil
}

func (linuxBackend) IPv4Forwarding() (bool, error) {
	data, err := os.ReadFile("/proc/sys/net/ipv4/ip_forward")
	if err != nil {
		return false, err
	}
	value, err := strconv.Atoi(strings.TrimSpace(string(data)))
	return value == 1, err
}

func (linuxBackend) VerifyTUN(_ context.Context, plan Plan) error {
	link, err := netlink.LinkByName(plan.TUNInterface)
	if err != nil {
		return fmt.Errorf("TUN interface %s is unavailable: %w", plan.TUNInterface, err)
	}
	addresses, err := netlink.AddrList(link, netlink.FAMILY_V4)
	if err != nil {
		return err
	}
	want, err := netip.ParsePrefix(plan.TUNAddress)
	if err != nil {
		return err
	}
	for _, address := range addresses {
		if address.IPNet != nil {
			current, parseErr := netip.ParsePrefix(address.IPNet.String())
			if parseErr == nil && current.Addr() == want.Addr() && current.Bits() == want.Bits() {
				return nil
			}
		}
	}
	return fmt.Errorf("TUN interface %s does not have address %s", plan.TUNInterface, plan.TUNAddress)
}

func (backend linuxBackend) ApplyTProxy(ctx context.Context, plan Plan) (result error) {
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := checkPolicyRouteCollisions(); err != nil {
		return err
	}
	if err := checkNFTablesCollisions(); err != nil {
		return err
	}
	if err := backend.Cleanup(ctx); err != nil {
		return fmt.Errorf("clean stale Sempre TProxy state: %w", err)
	}
	defer func() {
		if result != nil {
			result = errors.Join(result, backend.Cleanup(ctx))
		}
	}()
	if err := addPolicyRoutes(); err != nil {
		return fmt.Errorf("create TProxy policy routes: %w", err)
	}
	if err := addNFTables(plan); err != nil {
		return fmt.Errorf("create Sempre nftables rules: %w", err)
	}
	return nil
}

func (linuxBackend) VerifyTProxy(_ context.Context, plan Plan) error {
	return errors.Join(
		verifySempreNFTables(plan),
		listenersReady(plan),
		verifyLANInterfaces(plan),
		verifyPolicyRoutes(),
		verifyTrafficSources(plan),
	)
}

func (backend linuxBackend) Diagnostics(ctx context.Context, plan Plan) []Diagnostic {
	if plan.Mode == "tun-router" {
		return []Diagnostic{
			{Name: "Linux TUN interface and address", Err: backend.VerifyTUN(ctx, plan)},
			{Name: "sing-box TUN auto-redirect nftables rules", Err: verifySingBoxNFTables()},
			{Name: "Linux IPv4 forwarding", Err: verifyIPv4Forwarding(plan)},
		}
	}
	diagnostics := []Diagnostic{
		{Name: "Sempre TProxy nftables rules", Err: verifySempreNFTables(plan)},
		{Name: "Sempre TProxy policy routing", Err: verifyPolicyRoutes()},
		{Name: "Sempre TProxy TCP listeners", Err: listenersReady(plan)},
		{Name: "Linux LAN interfaces", Err: verifyLANInterfaces(plan)},
		{Name: "Linux IPv4 forwarding", Err: verifyIPv4Forwarding(plan)},
		{Name: "Linux transparent traffic sources", Err: verifyTrafficSources(plan)},
	}
	return append(diagnostics, observedTrafficDiagnostics(plan)...)
}

func verifySempreNFTables(plan Plan) error {
	var failures []error
	for _, family := range []nftables.TableFamily{nftables.TableFamilyIPv4, nftables.TableFamilyIPv6} {
		connection := &nftables.Conn{}
		tables, err := connection.ListTables()
		if err != nil {
			failures = append(failures, err)
			continue
		}
		var found *nftables.Table
		for _, table := range tables {
			if table.Family == family && table.Name == nftTableName {
				found = table
				break
			}
		}
		if found == nil {
			failures = append(failures, fmt.Errorf("nftables %s table is missing", nftTableName))
			continue
		}
		owned, ownerErr := isOwnedNFTablesTable(connection, found)
		if ownerErr != nil {
			failures = append(failures, ownerErr)
			continue
		}
		if !owned {
			failures = append(failures, fmt.Errorf("nftables %s table is not owned by Sempre", nftTableName))
			continue
		}
		chains, err := connection.ListChains()
		if err != nil {
			failures = append(failures, err)
			continue
		}
		required := map[string]bool{"prerouting": false}
		if plan.CaptureHost {
			required["output"] = false
		}
		for _, chain := range chains {
			if chain.Table == nil || chain.Table.Name != nftTableName || chain.Table.Family != family {
				continue
			}
			if _, ok := required[chain.Name]; !ok {
				continue
			}
			rules, ruleErr := connection.GetRules(found, chain)
			if ruleErr != nil {
				failures = append(failures, ruleErr)
				continue
			}
			if len(rules) == 0 {
				failures = append(failures, fmt.Errorf("nftables %s chain %s has no rules", nftTableName, chain.Name))
				continue
			}
			required[chain.Name] = true
		}
		for name, present := range required {
			if !present {
				failures = append(failures, fmt.Errorf("nftables %s chain %s is missing", nftTableName, name))
			}
		}
	}
	return errors.Join(failures...)
}

func verifySingBoxNFTables() error {
	connection := &nftables.Conn{}
	tables, err := connection.ListTables()
	if err != nil {
		return err
	}
	var table *nftables.Table
	for _, current := range tables {
		if current.Family == nftables.TableFamilyINet && current.Name == "sing-box" {
			table = current
			break
		}
	}
	if table == nil {
		return fmt.Errorf("nftables inet table sing-box is missing")
	}
	chains, err := connection.ListChains()
	if err != nil {
		return err
	}
	foundRules := false
	for _, chain := range chains {
		if chain.Table == nil || chain.Table.Family != table.Family || chain.Table.Name != table.Name {
			continue
		}
		rules, ruleErr := connection.GetRules(table, chain)
		if ruleErr != nil {
			return ruleErr
		}
		if len(rules) > 0 {
			foundRules = true
		}
	}
	if !foundRules {
		return fmt.Errorf("nftables inet table sing-box has no rules")
	}
	return nil
}

func verifyLANInterfaces(plan Plan) error {
	var failures []error
	for _, name := range plan.LANInterfaces {
		if _, err := netlink.LinkByName(name); err != nil {
			failures = append(failures, fmt.Errorf("LAN interface %s is unavailable: %w", name, err))
		}
	}
	return errors.Join(failures...)
}

func verifyIPv4Forwarding(plan Plan) error {
	if len(plan.LANInterfaces) == 0 {
		return nil
	}
	enabled, err := (linuxBackend{}).IPv4Forwarding()
	if err != nil {
		return err
	}
	if !enabled {
		return fmt.Errorf("net.ipv4.ip_forward is disabled")
	}
	return nil
}

func verifyTrafficSources(plan Plan) error {
	if len(plan.LANInterfaces) == 0 && !plan.CaptureHost {
		return fmt.Errorf("no traffic source is configured")
	}
	return nil
}

func (linuxBackend) Cleanup(_ context.Context) error {
	nftErr := deleteNFTables()
	policyErr := deletePolicyRoutes()
	return errors.Join(nftErr, policyErr)
}
