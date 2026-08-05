//go:build linux

package transparentproxy

import (
	"context"
	"encoding/binary"
	"errors"
	"fmt"
	"net"
	"net/netip"
	"os"
	"sort"
	"strconv"
	"strings"
	"syscall"

	"github.com/google/nftables"
	"github.com/google/nftables/expr"
	"github.com/google/nftables/userdata"
	"github.com/vishvananda/netlink"
	"golang.org/x/sys/unix"
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

func addNFTables(plan Plan) error {
	connection := &nftables.Conn{}
	for _, family := range []nftables.TableFamily{nftables.TableFamilyIPv4, nftables.TableFamilyIPv6} {
		table := connection.AddTable(&nftables.Table{Family: family, Name: nftTableName})
		owner := connection.AddChain(&nftables.Chain{Name: nftOwnerChain, Table: table})
		connection.AddRule(&nftables.Rule{
			Table: table, Chain: owner, Exprs: []expr.Any{&expr.Counter{}}, UserData: encodeRuleLabel(nftOwnerLabel),
		})
		prerouting := connection.AddChain(&nftables.Chain{
			Name:     "prerouting",
			Table:    table,
			Type:     nftables.ChainTypeFilter,
			Hooknum:  nftables.ChainHookPrerouting,
			Priority: nftables.ChainPriorityMangle,
		})
		addExclusionRules(connection, table, prerouting, family, plan.ExcludedPrefixes)
		for _, protocol := range []byte{unix.IPPROTO_TCP, unix.IPPROTO_UDP} {
			connection.AddRule(&nftables.Rule{
				Table: table, Chain: prerouting,
				Exprs:    markedCaptureExpressions(family, protocol, 53, plan.DNSPort),
				UserData: captureRuleLabel("dns", "host", protocol, ""),
			})
			connection.AddRule(&nftables.Rule{
				Table: table, Chain: prerouting,
				Exprs:    markedCaptureExpressions(family, protocol, 0, plan.TProxyPort),
				UserData: captureRuleLabel("proxy", "host", protocol, ""),
			})
			for _, interfaceName := range plan.LANInterfaces {
				connection.AddRule(&nftables.Rule{
					Table: table, Chain: prerouting,
					Exprs:    lanCaptureExpressions(family, interfaceName, protocol, 53, plan.DNSPort),
					UserData: captureRuleLabel("dns", "lan", protocol, interfaceName),
				})
				connection.AddRule(&nftables.Rule{
					Table: table, Chain: prerouting,
					Exprs:    lanCaptureExpressions(family, interfaceName, protocol, 0, plan.TProxyPort),
					UserData: captureRuleLabel("proxy", "lan", protocol, interfaceName),
				})
			}
		}
		if plan.CaptureHost {
			output := connection.AddChain(&nftables.Chain{
				Name:     "output",
				Table:    table,
				Type:     nftables.ChainTypeRoute,
				Hooknum:  nftables.ChainHookOutput,
				Priority: nftables.ChainPriorityMangle,
			})
			connection.AddRule(&nftables.Rule{Table: table, Chain: output, Exprs: markReturnExpressions(BypassMark)})
			addExclusionRules(connection, table, output, family, plan.ExcludedPrefixes)
			for _, protocol := range []byte{unix.IPPROTO_TCP, unix.IPPROTO_UDP} {
				connection.AddRule(&nftables.Rule{
					Table: table, Chain: output, Exprs: outputMarkExpressions(protocol),
					UserData: captureRuleLabel("output", "host", protocol, ""),
				})
			}
		}
	}
	return connection.Flush()
}

func deleteNFTables() error {
	connection := &nftables.Conn{}
	tables, err := connection.ListTables()
	if err != nil {
		return err
	}
	deleted := false
	var failures []error
	for _, table := range tables {
		if table.Name == nftTableName && (table.Family == nftables.TableFamilyIPv4 || table.Family == nftables.TableFamilyIPv6) {
			owned, ownerErr := isOwnedNFTablesTable(connection, table)
			if ownerErr != nil {
				failures = append(failures, ownerErr)
				continue
			}
			if owned {
				connection.DelTable(table)
				deleted = true
			}
		}
	}
	if !deleted {
		return errors.Join(failures...)
	}
	return errors.Join(errors.Join(failures...), connection.Flush())
}

func checkNFTablesCollisions() error {
	connection := &nftables.Conn{}
	tables, err := connection.ListTables()
	if err != nil {
		return err
	}
	for _, table := range tables {
		if table.Name != nftTableName || (table.Family != nftables.TableFamilyIPv4 && table.Family != nftables.TableFamilyIPv6) {
			continue
		}
		owned, err := isOwnedNFTablesTable(connection, table)
		if err != nil {
			return err
		}
		if !owned {
			return fmt.Errorf("nftables table %s already exists and is not owned by Sempre", nftTableName)
		}
	}
	return nil
}

func isOwnedNFTablesTable(connection *nftables.Conn, table *nftables.Table) (bool, error) {
	chains, err := connection.ListChains()
	if err != nil {
		return false, err
	}
	for _, chain := range chains {
		if chain.Table == nil || chain.Table.Name != table.Name || chain.Table.Family != table.Family || chain.Name != nftOwnerChain {
			continue
		}
		rules, err := connection.GetRules(table, chain)
		if err != nil {
			return false, err
		}
		for _, rule := range rules {
			if ruleLabel(rule.UserData) == nftOwnerLabel {
				return true, nil
			}
		}
	}
	return false, nil
}

func addExclusionRules(
	connection *nftables.Conn,
	table *nftables.Table,
	chain *nftables.Chain,
	family nftables.TableFamily,
	prefixes []string,
) {
	for _, value := range prefixes {
		prefix, err := netip.ParsePrefix(value)
		if err != nil || (prefix.Addr().Is4() != (family == nftables.TableFamilyIPv4)) {
			continue
		}
		connection.AddRule(&nftables.Rule{Table: table, Chain: chain, Exprs: prefixReturnExpressions(prefix.Masked())})
	}
}

func prefixReturnExpressions(prefix netip.Prefix) []expr.Any {
	length := 16
	offset := uint32(24)
	if prefix.Addr().Is4() {
		length = 4
		offset = 16
	}
	mask := net.CIDRMask(prefix.Bits(), prefix.Addr().BitLen())
	return []expr.Any{
		&expr.Payload{DestRegister: 1, Base: expr.PayloadBaseNetworkHeader, Offset: offset, Len: uint32(length)},
		&expr.Bitwise{SourceRegister: 1, DestRegister: 1, Len: uint32(length), Mask: mask, Xor: make([]byte, length)},
		&expr.Cmp{Op: expr.CmpOpEq, Register: 1, Data: prefix.Addr().AsSlice()},
		&expr.Verdict{Kind: expr.VerdictReturn},
	}
}

func markedCaptureExpressions(family nftables.TableFamily, protocol byte, destinationPort, proxyPort int) []expr.Any {
	result := markMatchExpressions(RouteMark)
	result = append(result, protocolMatchExpressions(protocol)...)
	if destinationPort > 0 {
		result = append(result, portMatchExpressions(destinationPort)...)
	}
	return append(result, tproxyExpressions(family, proxyPort)...)
}

func lanCaptureExpressions(family nftables.TableFamily, interfaceName string, protocol byte, destinationPort, proxyPort int) []expr.Any {
	result := []expr.Any{
		&expr.Meta{Key: expr.MetaKeyIIFNAME, Register: 1},
		&expr.Cmp{Op: expr.CmpOpEq, Register: 1, Data: interfaceBytes(interfaceName)},
	}
	result = append(result, protocolMatchExpressions(protocol)...)
	if destinationPort > 0 {
		result = append(result, portMatchExpressions(destinationPort)...)
	}
	return append(result, tproxyExpressions(family, proxyPort)...)
}

func outputMarkExpressions(protocol byte) []expr.Any {
	result := protocolMatchExpressions(protocol)
	result = append(result,
		&expr.Immediate{Register: 1, Data: uint32Bytes(RouteMark)},
		&expr.Meta{Key: expr.MetaKeyMARK, SourceRegister: true, Register: 1},
		&expr.Counter{},
		&expr.Verdict{Kind: expr.VerdictAccept},
	)
	return result
}

func tproxyExpressions(family nftables.TableFamily, port int) []expr.Any {
	return []expr.Any{
		&expr.Immediate{Register: 1, Data: uint32Bytes(RouteMark)},
		&expr.Meta{Key: expr.MetaKeyMARK, SourceRegister: true, Register: 1},
		&expr.Immediate{Register: 1, Data: uint16Bytes(uint16(port))},
		&expr.Counter{},
		&expr.TProxy{Family: byte(family), TableFamily: byte(family), RegPort: 1},
		&expr.Verdict{Kind: expr.VerdictAccept},
	}
}

type trafficObservation struct {
	host uint64
	lan  uint64
	dns  uint64
}

func observedTrafficDiagnostics(plan Plan) []Diagnostic {
	observation, err := readTrafficObservation()
	if err != nil {
		return []Diagnostic{{Name: "Sempre TProxy traffic counters", Err: err, Warning: true}}
	}
	diagnostics := []Diagnostic{}
	if plan.CaptureHost {
		diagnostics = append(diagnostics, Diagnostic{
			Name: "Sempre TProxy observed host traffic", Err: packetsObserved(observation.host), Warning: true,
		})
	}
	if len(plan.LANInterfaces) > 0 {
		diagnostics = append(diagnostics, Diagnostic{
			Name: "Sempre TProxy observed LAN traffic", Err: packetsObserved(observation.lan), Warning: true,
		})
	}
	diagnostics = append(diagnostics, Diagnostic{
		Name: "Sempre TProxy observed DNS traffic", Err: packetsObserved(observation.dns), Warning: true,
	})
	return diagnostics
}

func readTrafficObservation() (trafficObservation, error) {
	connection := &nftables.Conn{}
	chains, err := connection.ListChains()
	if err != nil {
		return trafficObservation{}, err
	}
	observation := trafficObservation{}
	for _, chain := range chains {
		if chain.Table == nil || chain.Table.Name != nftTableName || chain.Name != "prerouting" {
			continue
		}
		rules, err := connection.GetRules(chain.Table, chain)
		if err != nil {
			return trafficObservation{}, err
		}
		for _, rule := range rules {
			label := ruleLabel(rule.UserData)
			packets := uint64(0)
			for _, expression := range rule.Exprs {
				if counter, ok := expression.(*expr.Counter); ok {
					packets += counter.Packets
				}
			}
			if strings.HasPrefix(label, "sempre:dns:") {
				observation.dns += packets
			}
			if strings.Contains(label, ":host:") {
				observation.host += packets
			}
			if strings.Contains(label, ":lan:") {
				observation.lan += packets
			}
		}
	}
	return observation, nil
}

func packetsObserved(packets uint64) error {
	if packets == 0 {
		return fmt.Errorf("no packets have reached this capture path since it was installed")
	}
	return nil
}

func captureRuleLabel(kind, source string, protocol byte, interfaceName string) []byte {
	protocolName := "udp"
	if protocol == unix.IPPROTO_TCP {
		protocolName = "tcp"
	}
	return encodeRuleLabel(strings.Join([]string{"sempre", kind, source, protocolName, interfaceName}, ":"))
}

func encodeRuleLabel(label string) []byte {
	return userdata.AppendString(nil, userdata.TypeComment, label)
}

func ruleLabel(value []byte) string {
	for len(value) >= 2 {
		length := int(value[1])
		if length > len(value)-2 {
			return ""
		}
		if userdata.Type(value[0]) == userdata.TypeComment {
			return strings.TrimSuffix(string(value[2:2+length]), "\x00")
		}
		value = value[2+length:]
	}
	return ""
}

func markReturnExpressions(mark uint32) []expr.Any {
	result := markMatchExpressions(mark)
	return append(result, &expr.Verdict{Kind: expr.VerdictReturn})
}

func markMatchExpressions(mark uint32) []expr.Any {
	return []expr.Any{
		&expr.Meta{Key: expr.MetaKeyMARK, Register: 1},
		&expr.Cmp{Op: expr.CmpOpEq, Register: 1, Data: uint32Bytes(mark)},
	}
}

func protocolMatchExpressions(protocol byte) []expr.Any {
	return []expr.Any{
		&expr.Meta{Key: expr.MetaKeyL4PROTO, Register: 1},
		&expr.Cmp{Op: expr.CmpOpEq, Register: 1, Data: []byte{protocol}},
	}
}

func portMatchExpressions(port int) []expr.Any {
	return []expr.Any{
		&expr.Payload{DestRegister: 1, Base: expr.PayloadBaseTransportHeader, Offset: 2, Len: 2},
		&expr.Cmp{Op: expr.CmpOpEq, Register: 1, Data: uint16Bytes(uint16(port))},
	}
}

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
