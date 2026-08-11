//go:build linux

package transparentproxy

import (
	"errors"
	"fmt"
	"net"
	"net/netip"

	"github.com/google/nftables"
	"github.com/google/nftables/expr"
	"golang.org/x/sys/unix"
)

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
		for _, protocol := range []byte{unix.IPPROTO_TCP, unix.IPPROTO_UDP} {
			connection.AddRule(&nftables.Rule{
				Table: table, Chain: prerouting,
				Exprs:    markedCaptureExpressions(family, protocol, 53, plan.DNSPort),
				UserData: captureRuleLabel("dns", "host", protocol, ""),
			})
			for _, interfaceName := range plan.LANInterfaces {
				connection.AddRule(&nftables.Rule{
					Table: table, Chain: prerouting,
					Exprs:    lanCaptureExpressions(family, interfaceName, protocol, 53, plan.DNSPort),
					UserData: captureRuleLabel("dns", "lan", protocol, interfaceName),
				})
			}
		}
		addExclusionRules(connection, table, prerouting, family, plan.ExcludedPrefixes)
		for _, protocol := range []byte{unix.IPPROTO_TCP, unix.IPPROTO_UDP} {
			connection.AddRule(&nftables.Rule{
				Table: table, Chain: prerouting,
				Exprs:    markedCaptureExpressions(family, protocol, 0, plan.TProxyPort),
				UserData: captureRuleLabel("proxy", "host", protocol, ""),
			})
			for _, interfaceName := range plan.LANInterfaces {
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
			for _, protocol := range []byte{unix.IPPROTO_TCP, unix.IPPROTO_UDP} {
				connection.AddRule(&nftables.Rule{
					Table: table, Chain: output, Exprs: outputMarkExpressions(protocol, 53),
					UserData: captureRuleLabel("output-dns", "host", protocol, ""),
				})
			}
			addExclusionRules(connection, table, output, family, plan.ExcludedPrefixes)
			for _, protocol := range []byte{unix.IPPROTO_TCP, unix.IPPROTO_UDP} {
				connection.AddRule(&nftables.Rule{
					Table: table, Chain: output, Exprs: outputMarkExpressions(protocol, 0),
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

func outputMarkExpressions(protocol byte, destinationPort int) []expr.Any {
	result := protocolMatchExpressions(protocol)
	if destinationPort > 0 {
		result = append(result, portMatchExpressions(destinationPort)...)
	}
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
