package gateway

import (
	"strings"
	"testing"
)

func TestDefaultConfigValidAndDisabled(t *testing.T) {
	config := DefaultConfig()
	if err := config.Validate(); err != nil {
		t.Fatalf("default config validation = %v", err)
	}
	if config.DHCP.Enabled || config.DNS.Enabled || config.LAN.NATEnabled {
		t.Fatalf("gateway services should default disabled: %#v", config)
	}
}

func TestValidateRejectsDHCPRangeOutsideLAN(t *testing.T) {
	config := DefaultConfig()
	config.LAN.GatewayCIDR = "10.10.10.1/24"
	config.DHCP.RangeStart = "192.168.1.10"
	messages := ValidationMessages(config)
	if len(messages) == 0 || !strings.Contains(strings.Join(messages, "\n"), "DHCP range") {
		t.Fatalf("validation messages = %#v", messages)
	}
}

func TestBuildHostPlanIncludesGatewayAndNAT(t *testing.T) {
	config := DefaultConfig()
	config.LAN.Interface = "vmbr1"
	config.LAN.WANInterface = "vmbr0"
	config.LAN.NATEnabled = true
	plan, err := BuildHostPlan(config)
	if err != nil {
		t.Fatalf("host plan error = %v", err)
	}
	joined := strings.Join(plan.Commands, "\n")
	for _, want := range []string{"ip addr replace 10.10.10.1/24 dev vmbr1", "net.ipv4.ip_forward=1", "masquerade"} {
		if !strings.Contains(joined, want) {
			t.Fatalf("plan commands missing %q: %s", want, joined)
		}
	}
}

func TestParseRuleSetLinesSupportsClashPayload(t *testing.T) {
	rules := parseRuleSetLines("payload:\n  - DOMAIN-SUFFIX,example.cn\n  - 'DOMAIN,api.example.cn'\n# comment\n")
	if len(rules) != 2 || rules[0] != "DOMAIN-SUFFIX,example.cn" || rules[1] != "DOMAIN,api.example.cn" {
		t.Fatalf("rules = %#v", rules)
	}
}
