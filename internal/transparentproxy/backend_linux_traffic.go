//go:build linux

package transparentproxy

import (
	"fmt"
	"strings"

	"github.com/google/nftables"
	"github.com/google/nftables/expr"
	"github.com/google/nftables/userdata"
	"golang.org/x/sys/unix"
)

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
