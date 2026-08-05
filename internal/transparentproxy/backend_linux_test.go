//go:build linux

package transparentproxy

import (
	"encoding/binary"
	"net"
	"testing"

	"github.com/vishvananda/netlink"
	"golang.org/x/sys/unix"
)

func TestClassifyLinkSeparatesPVEContainerAndVPNInterfaces(t *testing.T) {
	tests := []struct {
		link netlink.Link
		want string
	}{
		{link: &netlink.Bridge{LinkAttrs: netlink.LinkAttrs{Name: "vmbr1"}}, want: "bridge"},
		{link: &netlink.Bridge{LinkAttrs: netlink.LinkAttrs{Name: "docker0"}}, want: "container"},
		{link: &netlink.Bridge{LinkAttrs: netlink.LinkAttrs{Name: "br-deadbeef"}}, want: "container"},
		{link: &netlink.Veth{LinkAttrs: netlink.LinkAttrs{Name: "veth1234"}}, want: "container"},
		{link: &netlink.Dummy{LinkAttrs: netlink.LinkAttrs{Name: "wg0"}}, want: "vpn"},
		{link: &netlink.Dummy{LinkAttrs: netlink.LinkAttrs{Name: "eth0"}}, want: "physical"},
	}
	for _, test := range tests {
		if got := classifyLink(test.link); got != test.want {
			t.Errorf("classifyLink(%s) = %q, want %q", test.link.Attrs().Name, got, test.want)
		}
	}
}

func TestNFTablesValueEncodingUsesFieldByteOrder(t *testing.T) {
	mark := uint32Bytes(RouteMark)
	if got := binary.NativeEndian.Uint32(mark); got != RouteMark {
		t.Fatalf("decoded mark = %#x, want %#x", got, RouteMark)
	}
	port := uint16Bytes(7893)
	if got := binary.BigEndian.Uint16(port); got != 7893 {
		t.Fatalf("decoded port = %d, want 7893", got)
	}
}

func TestKernelObjectOwnershipRequiresSempreProtocol(t *testing.T) {
	mask := uint32(0xffffffff)
	rule := *netlink.NewRule()
	rule.Priority = RulePriority
	rule.Table = RouteTable
	rule.Mark = RouteMark
	rule.Mask = &mask
	rule.Protocol = PolicyProtocol
	if !ownedPolicyRule(rule, &mask) {
		t.Fatal("Sempre policy rule was not recognized")
	}
	rule.Protocol = unix.RTPROT_STATIC
	if ownedPolicyRule(rule, &mask) {
		t.Fatal("foreign policy rule was recognized as Sempre-owned")
	}

	_, destination, _ := net.ParseCIDR("0.0.0.0/0")
	route := netlink.Route{Type: unix.RTN_LOCAL, Dst: destination, Protocol: netlink.RouteProtocol(PolicyProtocol)}
	if !ownedPolicyRoute(route, netlink.FAMILY_V4) {
		t.Fatal("Sempre policy route was not recognized")
	}
	route.Protocol = unix.RTPROT_STATIC
	if ownedPolicyRoute(route, netlink.FAMILY_V4) {
		t.Fatal("foreign policy route was recognized as Sempre-owned")
	}
}

func TestNFTablesRuleLabelsRoundTrip(t *testing.T) {
	const label = "sempre:dns:lan:tcp:vmbr1"
	if got := ruleLabel(encodeRuleLabel(label)); got != label {
		t.Fatalf("decoded label = %q, want %q", got, label)
	}
	if got := ruleLabel([]byte{byte(0), 10, 'x'}); got != "" {
		t.Fatalf("malformed label decoded as %q", got)
	}
}
