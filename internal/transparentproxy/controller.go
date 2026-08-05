package transparentproxy

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/netip"
	"os"
	"sort"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

const (
	RouteMark    uint32 = 0x53500001
	BypassMark   uint32 = 0x53500002
	RouteTable          = 20240
	RulePriority        = 20240
)

type Interface struct {
	Name      string   `json:"name"`
	Index     int      `json:"index"`
	Kind      string   `json:"kind"`
	Up        bool     `json:"up"`
	Default   bool     `json:"default_route"`
	Addresses []string `json:"addresses"`
}

type Inventory struct {
	Interfaces               []Interface `json:"interfaces"`
	DefaultInterface         string      `json:"default_interface,omitempty"`
	RecommendedLANInterfaces []string    `json:"recommended_lan_interfaces"`
	LocalPrefixes            []string    `json:"local_prefixes"`
	VPNPrefixes              []string    `json:"vpn_prefixes"`
	OccupiedPrefixes         []string    `json:"occupied_prefixes"`
}

type Plan struct {
	Mode             string
	Config           string
	TUNInterface     string
	TUNAddress       string
	RouteExclusions  []string
	TProxyPort       int
	DNSPort          int
	CaptureHost      bool
	LANInterfaces    []string
	ExcludedPrefixes []string
}

func (plan Plan) Enabled() bool {
	return plan.Mode == subscriptions.TransparentProxyTUN || plan.Mode == subscriptions.TransparentProxyTProxy
}

type systemBackend interface {
	Supported() bool
	Inventory(context.Context) (Inventory, error)
	RequirePrivileges() error
	IPv4Forwarding() (bool, error)
	ApplyTProxy(context.Context, Plan) error
	VerifyTProxy(context.Context, Plan) error
	VerifyTUN(context.Context, Plan) error
	Cleanup(context.Context) error
}

type Controller struct {
	backend systemBackend
}

func New() *Controller {
	return &Controller{backend: newSystemBackend()}
}

func (controller *Controller) Inventory(ctx context.Context) (Inventory, error) {
	if !controller.backend.Supported() {
		return Inventory{}, nil
	}
	return controller.backend.Inventory(ctx)
}

func (controller *Controller) Prepare(
	ctx context.Context,
	coreID string,
	profile subscriptions.Profile,
	configPath string,
) (Plan, error) {
	plan := Plan{Mode: subscriptions.TransparentProxyDisabled, Config: configPath}
	if coreID != "sing-box" || !controller.backend.Supported() {
		return plan, nil
	}
	transparent := profile.TransparentProxy
	plan.Mode = transparent.Mode
	if !plan.Enabled() {
		return plan, nil
	}
	if err := controller.backend.RequirePrivileges(); err != nil {
		return Plan{}, err
	}
	inventory, err := controller.backend.Inventory(ctx)
	if err != nil {
		return Plan{}, fmt.Errorf("inspect Linux routes: %w", err)
	}
	data, err := os.ReadFile(configPath)
	if err != nil {
		return Plan{}, fmt.Errorf("read runtime configuration: %w", err)
	}
	var document map[string]any
	if err := json.Unmarshal(data, &document); err != nil {
		return Plan{}, fmt.Errorf("decode runtime configuration: %w", err)
	}
	switch transparent.Mode {
	case subscriptions.TransparentProxyTUN:
		plan, err = prepareTUN(plan, transparent.TUN, inventory, document)
	case subscriptions.TransparentProxyTProxy:
		plan, err = prepareTProxy(plan, transparent.TProxy, inventory, document)
	default:
		err = fmt.Errorf("unsupported Linux transparent proxy mode %q", transparent.Mode)
	}
	if err != nil {
		return Plan{}, err
	}
	if len(plan.LANInterfaces) > 0 {
		enabled, forwardingErr := controller.backend.IPv4Forwarding()
		if forwardingErr != nil {
			return Plan{}, fmt.Errorf("check net.ipv4.ip_forward: %w", forwardingErr)
		}
		if !enabled {
			return Plan{}, fmt.Errorf("net.ipv4.ip_forward is disabled; enable forwarding before using Sempre as a LAN gateway")
		}
	}
	encoded, err := json.MarshalIndent(document, "", "  ")
	if err != nil {
		return Plan{}, err
	}
	if err := state.WriteAtomic(configPath, append(encoded, '\n'), 0o600); err != nil {
		return Plan{}, fmt.Errorf("write resolved Linux runtime configuration: %w", err)
	}
	return plan, nil
}

func (controller *Controller) Apply(ctx context.Context, plan Plan) error {
	if !plan.Enabled() {
		return nil
	}
	deadline := time.Now().Add(8 * time.Second)
	for {
		var err error
		if plan.Mode == subscriptions.TransparentProxyTUN {
			err = controller.backend.VerifyTUN(ctx, plan)
		} else {
			err = listenersReady(plan)
		}
		if err == nil {
			break
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("transparent proxy did not become ready: %w", err)
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(100 * time.Millisecond):
		}
	}
	if plan.Mode == subscriptions.TransparentProxyTUN {
		return nil
	}
	if err := controller.backend.ApplyTProxy(ctx, plan); err != nil {
		_ = controller.backend.Cleanup(ctx)
		return err
	}
	if err := controller.backend.VerifyTProxy(ctx, plan); err != nil {
		_ = controller.backend.Cleanup(ctx)
		return fmt.Errorf("verify Linux TProxy data plane: %w", err)
	}
	return nil
}

func (controller *Controller) Verify(ctx context.Context, plan Plan) error {
	if !plan.Enabled() {
		return nil
	}
	if plan.Mode == subscriptions.TransparentProxyTUN {
		return controller.backend.VerifyTUN(ctx, plan)
	}
	return controller.backend.VerifyTProxy(ctx, plan)
}

func (controller *Controller) Cleanup(ctx context.Context) error {
	if !controller.backend.Supported() {
		return nil
	}
	if err := controller.backend.RequirePrivileges(); err != nil {
		return nil
	}
	return controller.backend.Cleanup(ctx)
}

func prepareTUN(
	plan Plan,
	config subscriptions.TUNConfig,
	inventory Inventory,
	document map[string]any,
) (Plan, error) {
	address, err := resolveTUNAddress(config.Address, inventory.OccupiedPrefixes)
	if err != nil {
		return Plan{}, err
	}
	exclusions := append([]string{}, config.RouteExcludeAddress...)
	if config.AutoExcludeLocalRoutes {
		exclusions = append(exclusions, inventory.LocalPrefixes...)
	}
	if config.AutoExcludeVPNRoutes {
		exclusions = append(exclusions, inventory.VPNPrefixes...)
	}
	exclusions = normalizedPrefixes(exclusions)
	inbound, err := findInbound(document, "tun-in", "tun")
	if err != nil {
		return Plan{}, err
	}
	inbound["interface_name"] = config.InterfaceName
	inbound["address"] = []string{address}
	inbound["auto_route"] = true
	inbound["auto_redirect"] = true
	inbound["strict_route"] = true
	inbound["stack"] = "system"
	if len(exclusions) > 0 {
		inbound["route_exclude_address"] = exclusions
	} else {
		delete(inbound, "route_exclude_address")
	}
	route := object(document["route"])
	route["auto_detect_interface"] = true
	document["route"] = route
	plan.TUNInterface = config.InterfaceName
	plan.TUNAddress = address
	plan.RouteExclusions = exclusions
	plan.LANInterfaces = append([]string{}, inventory.RecommendedLANInterfaces...)
	return plan, nil
}

func prepareTProxy(
	plan Plan,
	config subscriptions.TProxyConfig,
	inventory Inventory,
	document map[string]any,
) (Plan, error) {
	if _, err := findInbound(document, "tproxy-in", "tproxy"); err != nil {
		return Plan{}, err
	}
	if _, err := findInbound(document, "dns-in", "direct"); err != nil {
		return Plan{}, err
	}
	interfaces := append([]string{}, config.LANInterfaces...)
	if len(interfaces) == 0 {
		interfaces = append(interfaces, inventory.RecommendedLANInterfaces...)
	}
	interfaces = uniqueStrings(interfaces)
	available := map[string]bool{}
	for _, current := range inventory.Interfaces {
		available[current.Name] = true
	}
	for _, name := range interfaces {
		if !available[name] {
			return Plan{}, fmt.Errorf("configured LAN interface %q does not exist", name)
		}
	}
	if len(interfaces) == 0 && !config.CaptureHost {
		return Plan{}, fmt.Errorf("TProxy mode needs a LAN interface or capture_host enabled")
	}
	route := object(document["route"])
	route["default_mark"] = BypassMark
	route["auto_detect_interface"] = true
	document["route"] = route
	plan.TProxyPort = config.ListenPort
	plan.DNSPort = config.DNSListenPort
	plan.CaptureHost = config.CaptureHost
	plan.LANInterfaces = interfaces
	plan.ExcludedPrefixes = normalizedPrefixes(append(reservedPrefixes(), inventory.LocalPrefixes...))
	plan.ExcludedPrefixes = normalizedPrefixes(append(plan.ExcludedPrefixes, outboundServerPrefixes(document)...))
	return plan, nil
}

func findInbound(document map[string]any, tag, inboundType string) (map[string]any, error) {
	values, ok := document["inbounds"].([]any)
	if !ok {
		return nil, fmt.Errorf("runtime configuration has no inbounds")
	}
	for _, value := range values {
		inbound, ok := value.(map[string]any)
		if ok && inbound["tag"] == tag && inbound["type"] == inboundType {
			return inbound, nil
		}
	}
	return nil, fmt.Errorf("runtime configuration is missing %s %s inbound", tag, inboundType)
}

func listenersReady(plan Plan) error {
	for _, port := range []int{plan.TProxyPort, plan.DNSPort} {
		connection, err := net.DialTimeout("tcp", net.JoinHostPort("127.0.0.1", fmt.Sprint(port)), 100*time.Millisecond)
		if err != nil {
			return fmt.Errorf("TCP port %d is not listening: %w", port, err)
		}
		_ = connection.Close()
	}
	return nil
}

func resolveTUNAddress(explicit string, occupied []string) (string, error) {
	occupiedPrefixes := parsePrefixes(occupied)
	if explicit != "" {
		prefix, err := netip.ParsePrefix(explicit)
		if err != nil || !prefix.Addr().Is4() || prefix.Bits() != 30 {
			return "", fmt.Errorf("TUN address %q must be an IPv4 /30 prefix", explicit)
		}
		if prefixOverlapsAny(prefix.Masked(), occupiedPrefixes) {
			return "", fmt.Errorf("TUN address %s conflicts with an existing address or route", explicit)
		}
		return explicit, nil
	}
	for _, base := range []string{"172.19.0.0/16", "172.20.0.0/14", "172.24.0.0/13", "198.18.0.0/15"} {
		pool := netip.MustParsePrefix(base)
		address := pool.Masked().Addr()
		for pool.Contains(address) {
			candidate := netip.PrefixFrom(address, 30)
			if !prefixOverlapsAny(candidate, occupiedPrefixes) {
				return candidate.Addr().Next().String() + "/30", nil
			}
			for range 4 {
				address = address.Next()
			}
		}
	}
	return "", fmt.Errorf("no non-conflicting IPv4 /30 is available for the sing-box TUN")
}

func prefixOverlapsAny(candidate netip.Prefix, prefixes []netip.Prefix) bool {
	for _, current := range prefixes {
		if candidate.Addr().BitLen() == current.Addr().BitLen() &&
			(candidate.Contains(current.Addr()) || current.Contains(candidate.Addr())) {
			return true
		}
	}
	return false
}

func parsePrefixes(values []string) []netip.Prefix {
	result := make([]netip.Prefix, 0, len(values))
	for _, value := range values {
		if prefix, err := netip.ParsePrefix(value); err == nil {
			result = append(result, prefix.Masked())
		}
	}
	return result
}

func normalizedPrefixes(values []string) []string {
	prefixes := parsePrefixes(values)
	sort.Slice(prefixes, func(left, right int) bool {
		if prefixes[left].Addr().BitLen() != prefixes[right].Addr().BitLen() {
			return prefixes[left].Addr().BitLen() < prefixes[right].Addr().BitLen()
		}
		if prefixes[left].Addr() != prefixes[right].Addr() {
			return prefixes[left].Addr().Less(prefixes[right].Addr())
		}
		return prefixes[left].Bits() < prefixes[right].Bits()
	})
	result := make([]string, 0, len(prefixes))
	for _, prefix := range prefixes {
		if prefix.Bits() == 0 {
			continue
		}
		covered := false
		for _, existing := range parsePrefixes(result) {
			if existing.Addr().BitLen() == prefix.Addr().BitLen() && existing.Contains(prefix.Addr()) {
				covered = true
				break
			}
		}
		if !covered {
			result = append(result, prefix.String())
		}
	}
	return result
}

func outboundServerPrefixes(document map[string]any) []string {
	values, _ := document["outbounds"].([]any)
	result := []string{}
	for _, value := range values {
		outbound, _ := value.(map[string]any)
		server, _ := outbound["server"].(string)
		if address, err := netip.ParseAddr(strings.TrimSpace(server)); err == nil {
			result = append(result, netip.PrefixFrom(address, address.BitLen()).String())
		}
	}
	return result
}

func reservedPrefixes() []string {
	return []string{
		"0.0.0.0/8", "10.0.0.0/8", "100.64.0.0/10", "127.0.0.0/8",
		"169.254.0.0/16", "172.16.0.0/12", "192.0.0.0/24", "192.168.0.0/16",
		"224.0.0.0/4", "240.0.0.0/4", "::1/128", "fc00::/7", "fe80::/10", "ff00::/8",
	}
}

func uniqueStrings(values []string) []string {
	seen := map[string]bool{}
	result := make([]string, 0, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value != "" && !seen[value] {
			seen[value] = true
			result = append(result, value)
		}
	}
	sort.Strings(result)
	return result
}

func object(value any) map[string]any {
	if result, ok := value.(map[string]any); ok {
		return result
	}
	return map[string]any{}
}
