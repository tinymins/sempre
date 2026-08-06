package transparentproxy

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/netip"
	"os"
	"sort"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"gopkg.in/yaml.v3"
)

const (
	RouteMark      uint32 = 0x53500001
	BypassMark     uint32 = 0x53500002
	RouteTable            = 20240
	RulePriority          = 20240
	PolicyProtocol uint8  = 0xfd
)

var (
	listenerReadinessTimeout = 8 * time.Second
	tunReadinessTimeout      = 20 * time.Second
	readinessPollInterval    = 100 * time.Millisecond
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
	Supported                bool        `json:"supported"`
	Interfaces               []Interface `json:"interfaces"`
	DefaultInterface         string      `json:"default_interface,omitempty"`
	RecommendedLANInterfaces []string    `json:"recommended_lan_interfaces"`
	LocalPrefixes            []string    `json:"local_prefixes"`
	VPNPrefixes              []string    `json:"vpn_prefixes"`
	OccupiedPrefixes         []string    `json:"occupied_prefixes"`
}

type Diagnostic struct {
	Name    string
	Err     error
	Warning bool
}

type Plan struct {
	Core             string
	Mode             string
	Config           string
	SystemDNS        bool
	SystemDNSPort    int
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
	Diagnostics(context.Context, Plan) []Diagnostic
	Cleanup(context.Context) error
}

type Controller struct {
	backend   systemBackend
	systemDNS *systemDNSManager
}

type Option func(*Controller)

func WithSystemDNS(allowed bool, stateDir, resolvConf string) Option {
	return func(controller *Controller) {
		controller.systemDNS = &systemDNSManager{allowed: allowed, stateDir: stateDir, resolvConf: resolvConf}
	}
}

func New(options ...Option) *Controller {
	controller := &Controller{backend: newSystemBackend()}
	for _, option := range options {
		option(controller)
	}
	return controller
}

func (controller *Controller) Inventory(ctx context.Context) (Inventory, error) {
	if !controller.backend.Supported() {
		return Inventory{
			Interfaces:               []Interface{},
			RecommendedLANInterfaces: []string{},
			LocalPrefixes:            []string{},
			VPNPrefixes:              []string{},
			OccupiedPrefixes:         []string{},
		}, nil
	}
	inventory, err := controller.backend.Inventory(ctx)
	inventory.Supported = true
	return inventory, err
}

func (controller *Controller) Prepare(
	ctx context.Context,
	coreID string,
	profile subscriptions.Profile,
	configPath string,
) (Plan, error) {
	plan := Plan{Mode: subscriptions.TransparentProxyDisabled, Config: configPath}
	systemDNS, systemDNSPort := systemDNSIntent(profile.DNS)
	plan.SystemDNS = systemDNS
	plan.SystemDNSPort = systemDNSPort
	if systemDNS && (coreID != "sing-box" || controller.systemDNS == nil || !controller.systemDNS.allowed) {
		return Plan{}, fmt.Errorf("system DNS takeover is only available for Linux system sing-box runtime")
	}
	if !supportedCore(coreID) || !controller.backend.Supported() {
		return plan, nil
	}
	plan.Core = coreID
	transparent := profile.TransparentProxy
	plan.Mode = transparent.Mode
	if !coreSupportsMode(coreID, plan.Mode) {
		plan.Mode = subscriptions.TransparentProxyDisabled
	}
	if !plan.Enabled() && !plan.SystemDNS {
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
	document, err := decodeRuntimeDocument(coreID, data)
	if err != nil {
		return Plan{}, fmt.Errorf("decode runtime configuration: %w", err)
	}
	if plan.SystemDNS {
		if err := validateSystemDNSInbound(document, plan.SystemDNSPort); err != nil {
			return Plan{}, err
		}
	}
	switch transparent.Mode {
	case subscriptions.TransparentProxyTUN:
		switch coreID {
		case "sing-box":
			plan, err = prepareTUN(plan, transparent, inventory, document)
		case "mihomo":
			plan, err = prepareMihomoTUN(plan, transparent, inventory, document)
		case "clash-rs":
			plan, err = prepareClashRSTUN(plan, transparent, inventory, document)
		case "xray":
			plan, err = prepareXrayTUN(plan, transparent, inventory, document)
		default:
			err = fmt.Errorf("%s does not support tun-router mode", coreID)
		}
	case subscriptions.TransparentProxyTProxy:
		switch coreID {
		case "sing-box":
			plan, err = prepareTProxy(plan, transparent, inventory, document)
		case "mihomo", "clash-rs":
			plan, err = prepareMihomoTProxy(plan, transparent, inventory, document)
		case "xray", "v2ray":
			plan, err = prepareV2RayTProxy(plan, transparent, inventory, document)
		}
	default:
		if plan.Enabled() {
			err = fmt.Errorf("unsupported Linux transparent proxy mode %q", transparent.Mode)
		}
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
	encoded, err := encodeRuntimeDocument(coreID, document)
	if err != nil {
		return Plan{}, err
	}
	if err := state.WriteAtomic(configPath, encoded, 0o600); err != nil {
		return Plan{}, fmt.Errorf("write resolved Linux runtime configuration: %w", err)
	}
	return plan, nil
}

func (controller *Controller) Apply(ctx context.Context, plan Plan) error {
	if !plan.Enabled() && !plan.SystemDNS {
		return nil
	}
	if plan.Enabled() {
		timeout := listenerReadinessTimeout
		if plan.Mode == subscriptions.TransparentProxyTUN {
			timeout = tunReadinessTimeout
		}
		deadline := time.Now().Add(timeout)
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
				if plan.Mode == subscriptions.TransparentProxyTUN {
					return fmt.Errorf("timed out waiting for TUN interface %s to become ready after %s: %w", plan.TUNInterface, timeout, err)
				}
				return fmt.Errorf("transparent proxy did not become ready: %w", err)
			}
			select {
			case <-ctx.Done():
				return ctx.Err()
			case <-time.After(readinessPollInterval):
			}
		}
	}
	if plan.Mode == subscriptions.TransparentProxyTUN {
		// sing-box owns TUN routing in this mode.
	} else if plan.Enabled() {
		if err := controller.backend.ApplyTProxy(ctx, plan); err != nil {
			_ = controller.Cleanup(ctx)
			return err
		}
		if err := controller.backend.VerifyTProxy(ctx, plan); err != nil {
			_ = controller.Cleanup(ctx)
			return fmt.Errorf("verify Linux TProxy data plane: %w", err)
		}
	}
	if plan.SystemDNS {
		if err := waitForTCP(ctx, plan.SystemDNSPort, listenerReadinessTimeout); err != nil {
			_ = controller.Cleanup(ctx)
			return fmt.Errorf("system DNS listener did not become ready: %w", err)
		}
		if err := controller.systemDNS.Apply(); err != nil {
			_ = controller.Cleanup(ctx)
			return err
		}
	}
	return nil
}

func (controller *Controller) Verify(ctx context.Context, plan Plan) error {
	if !plan.Enabled() && !plan.SystemDNS {
		return nil
	}
	var failures []error
	if plan.Mode == subscriptions.TransparentProxyTUN {
		failures = append(failures, controller.backend.VerifyTUN(ctx, plan))
	} else if plan.Enabled() {
		failures = append(failures, controller.backend.VerifyTProxy(ctx, plan))
	}
	if plan.SystemDNS {
		failures = append(failures, controller.systemDNS.Verify())
	}
	return errors.Join(failures...)
}

func (controller *Controller) Cleanup(ctx context.Context) error {
	if !controller.backend.Supported() {
		return nil
	}
	if err := controller.backend.RequirePrivileges(); err != nil {
		return nil
	}
	var failures []error
	if controller.systemDNS != nil {
		failures = append(failures, controller.systemDNS.Restore())
	}
	failures = append(failures, controller.backend.Cleanup(ctx))
	return errors.Join(failures...)
}

func (controller *Controller) Diagnostics(
	ctx context.Context,
	coreID string,
	profile subscriptions.Profile,
	configPath string,
) []Diagnostic {
	if !supportedCore(coreID) || !controller.backend.Supported() || profile.TransparentProxy.Mode == subscriptions.TransparentProxyDisabled {
		return nil
	}
	plan, document, err := controller.runtimePlan(ctx, coreID, profile, configPath)
	if err != nil {
		return []Diagnostic{{Name: "Linux transparent runtime configuration", Err: err}}
	}
	diagnostics := []Diagnostic{
		{Name: "Linux transparent runtime configuration", Err: validateRuntimePlan(plan, document)},
		{Name: "Linux split DNS configuration", Err: validateSplitDNS(plan.Core, document)},
		{Name: "Linux domestic and foreign routing", Err: validateSplitRouting(plan.Core, document)},
	}
	return append(diagnostics, controller.backend.Diagnostics(ctx, plan)...)
}

func (controller *Controller) runtimePlan(
	ctx context.Context,
	coreID string,
	profile subscriptions.Profile,
	configPath string,
) (Plan, map[string]any, error) {
	data, err := os.ReadFile(configPath)
	if err != nil {
		return Plan{}, nil, fmt.Errorf("read runtime configuration: %w", err)
	}
	document, err := decodeRuntimeDocument(coreID, data)
	if err != nil {
		return Plan{}, nil, fmt.Errorf("decode runtime configuration: %w", err)
	}
	plan := Plan{Core: coreID, Mode: profile.TransparentProxy.Mode, Config: configPath}
	inventory, err := controller.backend.Inventory(ctx)
	if err != nil {
		return Plan{}, nil, fmt.Errorf("inspect Linux routes: %w", err)
	}
	if plan.Mode == subscriptions.TransparentProxyTUN {
		switch coreID {
		case "sing-box":
			inbound, findErr := findInbound(document, "tun-in", "tun")
			if findErr != nil {
				return Plan{}, nil, findErr
			}
			plan.TUNInterface, _ = inbound["interface_name"].(string)
			plan.TUNAddress = firstString(inbound["address"])
		case "mihomo":
			tun := object(document["tun"])
			plan.TUNInterface, _ = tun["device"].(string)
		case "clash-rs":
			tun := object(document["tun"])
			plan.TUNInterface, _ = tun["device"].(string)
			plan.TUNAddress, _ = tun["gateway"].(string)
		case "xray":
			inbound, findErr := findProtocolInbound(document, "tun-in", "tun")
			if findErr != nil {
				return Plan{}, nil, findErr
			}
			settings := object(inbound["settings"])
			plan.TUNInterface, _ = settings["name"].(string)
			plan.TUNAddress = firstString(settings["gateway"])
		}
		plan.LANInterfaces = append([]string{}, inventory.RecommendedLANInterfaces...)
	} else {
		config := profile.TransparentProxy
		plan.TProxyPort = config.TProxy.ListenPort
		plan.DNSPort = config.TProxy.DNSListenPort
		plan.CaptureHost = config.CaptureHost
		plan.LANInterfaces = uniqueStrings(config.LANInterfaces)
		if len(plan.LANInterfaces) == 0 {
			plan.LANInterfaces = append([]string{}, inventory.RecommendedLANInterfaces...)
		}
	}
	return plan, document, nil
}

func validateRuntimePlan(plan Plan, document map[string]any) error {
	if plan.Core == "clash-rs" {
		return validateClashRSRuntimePlan(plan, document)
	}
	if plan.Core == "mihomo" {
		return validateMihomoRuntimePlan(plan, document)
	}
	if plan.Core == "xray" || plan.Core == "v2ray" {
		return validateV2RayRuntimePlan(plan, document)
	}
	route := object(document["route"])
	if route["auto_detect_interface"] != true {
		return fmt.Errorf("route.auto_detect_interface is not enabled")
	}
	if plan.Mode == subscriptions.TransparentProxyTUN {
		inbound, err := findInbound(document, "tun-in", "tun")
		if err != nil {
			return err
		}
		for _, field := range []string{"auto_route", "auto_redirect", "strict_route"} {
			if inbound[field] != true {
				return fmt.Errorf("TUN inbound %s is not enabled", field)
			}
		}
		if plan.TUNInterface == "" || plan.TUNAddress == "" || inbound["stack"] != "system" {
			return fmt.Errorf("TUN interface, address, or system stack is missing")
		}
		return nil
	}
	if _, err := findInbound(document, "tproxy-in", "tproxy"); err != nil {
		return err
	}
	if _, err := findInbound(document, "dns-in", "direct"); err != nil {
		return err
	}
	if number, ok := numberAsUint32(route["default_mark"]); !ok || number != BypassMark {
		return fmt.Errorf("route.default_mark does not bypass the Sempre TProxy capture mark")
	}
	return nil
}

func validateSplitDNS(coreID string, document map[string]any) error {
	if coreID == "mihomo" || coreID == "clash-rs" {
		dns := object(document["dns"])
		if dns["respect-rules"] != true {
			return fmt.Errorf("dns.respect-rules is not enabled")
		}
		nameservers := stringValues(dns["nameserver"])
		if len(nameservers) == 0 || !strings.Contains(nameservers[0], "#") {
			return fmt.Errorf("remote DNS is not attached to a foreign selector")
		}
		policy := object(dns["nameserver-policy"])
		if len(stringValues(policy["geosite:cn"])) == 0 {
			return fmt.Errorf("geosite:cn does not use local DNS")
		}
		return nil
	}
	if coreID == "xray" || coreID == "v2ray" {
		return validateV2RaySplitDNS(document)
	}
	dns := object(document["dns"])
	if dns["final"] != "remote" {
		return fmt.Errorf("dns.final is not remote")
	}
	servers, _ := dns["servers"].([]any)
	remoteDetour := ""
	for _, value := range servers {
		server, _ := value.(map[string]any)
		if server["tag"] == "remote" {
			remoteDetour, _ = server["detour"].(string)
		}
	}
	if remoteDetour == "" || remoteDetour == "direct" {
		return fmt.Errorf("remote DNS is not attached to a foreign selector")
	}
	rules, _ := dns["rules"].([]any)
	for _, value := range rules {
		rule, _ := value.(map[string]any)
		if rule["server"] == "local" && stringListContains(rule["rule_set"], "geosite-cn") {
			return nil
		}
	}
	return fmt.Errorf("geosite-cn does not use local DNS")
}

func validateSplitRouting(coreID string, document map[string]any) error {
	if coreID == "mihomo" || coreID == "clash-rs" {
		rules := stringValues(document["rules"])
		if len(rules) == 0 || !strings.HasPrefix(rules[len(rules)-1], "MATCH,") || strings.HasSuffix(rules[len(rules)-1], ",DIRECT") {
			return fmt.Errorf("foreign route final is not a proxy selector")
		}
		for _, rule := range rules {
			if rule == "GEOIP,CN,DIRECT,no-resolve" || rule == "GEOSITE,CN,DIRECT" {
				return nil
			}
		}
		return fmt.Errorf("China routes do not use direct")
	}
	if coreID == "xray" || coreID == "v2ray" {
		return validateV2RaySplitRouting(document)
	}
	route := object(document["route"])
	final, _ := route["final"].(string)
	if final == "" || final == "direct" {
		return fmt.Errorf("foreign route final is not a proxy selector")
	}
	rules, _ := route["rules"].([]any)
	for _, value := range rules {
		rule, _ := value.(map[string]any)
		if rule["outbound"] == "direct" && stringListContains(rule["rule_set"], "geosite-cn") {
			return nil
		}
	}
	return fmt.Errorf("geosite-cn does not route direct")
}

func validateMihomoRuntimePlan(plan Plan, document map[string]any) error {
	if plan.Mode == subscriptions.TransparentProxyTUN {
		tun := object(document["tun"])
		for _, field := range []string{"enable", "auto-route", "strict-route", "auto-detect-interface"} {
			if tun[field] != true {
				return fmt.Errorf("TUN %s is not enabled", field)
			}
		}
		if plan.TUNInterface == "" || tun["stack"] != "system" {
			return fmt.Errorf("TUN interface or system stack is missing")
		}
		return nil
	}
	if integer(document["tproxy-port"]) != plan.TProxyPort {
		return fmt.Errorf("tproxy-port does not match the managed listener")
	}
	if !mihomoDNSListener(document, plan.DNSPort) {
		return fmt.Errorf("runtime configuration is missing the managed DNS TProxy listener")
	}
	if number, ok := numberAsUint32(document["routing-mark"]); !ok || number != BypassMark {
		return fmt.Errorf("routing-mark does not bypass the Sempre TProxy capture mark")
	}
	return nil
}

func validateClashRSRuntimePlan(plan Plan, document map[string]any) error {
	if plan.Mode == subscriptions.TransparentProxyTUN {
		tun := object(document["tun"])
		if tun["enable"] != true || tun["route-all"] != true || tun["dns-hijack"] != true {
			return fmt.Errorf("clash-rs TUN automatic routing or DNS capture is incomplete")
		}
		if plan.TUNInterface == "" || plan.TUNAddress == "" {
			return fmt.Errorf("clash-rs TUN interface or gateway is missing")
		}
		return nil
	}
	return validateMihomoRuntimePlan(plan, document)
}

func prepareTUN(
	plan Plan,
	config subscriptions.TransparentProxyConfig,
	inventory Inventory,
	document map[string]any,
) (Plan, error) {
	address, err := resolveTUNAddress(config.TUN.Address, inventory.OccupiedPrefixes)
	if err != nil {
		return Plan{}, err
	}
	exclusions := append([]string{}, config.RouteExclusions...)
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
) (Plan, error) {
	exclusions := append([]string{}, config.RouteExclusions...)
	if config.AutoExcludeLocalRoutes {
		exclusions = append(exclusions, inventory.LocalPrefixes...)
	}
	if config.AutoExcludeVPNRoutes {
		exclusions = append(exclusions, inventory.VPNPrefixes...)
	}
	exclusions = normalizedPrefixes(exclusions)
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
	plan.LANInterfaces = append([]string{}, inventory.RecommendedLANInterfaces...)
	return plan, nil
}

func prepareClashRSTUN(
	plan Plan,
	config subscriptions.TransparentProxyConfig,
	inventory Inventory,
	document map[string]any,
) (Plan, error) {
	address, err := resolveTUNAddress(config.TUN.Address, inventory.OccupiedPrefixes)
	if err != nil {
		return Plan{}, err
	}
	exclusions := append([]string{}, config.RouteExclusions...)
	if config.AutoExcludeLocalRoutes {
		exclusions = append(exclusions, inventory.LocalPrefixes...)
	}
	if config.AutoExcludeVPNRoutes {
		exclusions = append(exclusions, inventory.VPNPrefixes...)
	}
	tun := object(document["tun"])
	tun["enable"] = true
	tun["device"] = config.TUN.InterfaceName
	tun["gateway"] = address
	tun["route-all"] = true
	tun["dns-hijack"] = true
	document["tun"] = tun
	plan.TUNInterface = config.TUN.InterfaceName
	plan.TUNAddress = address
	plan.RouteExclusions = normalizedPrefixes(exclusions)
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
	exclusions := append([]string{}, config.RouteExclusions...)
	if config.AutoExcludeLocalRoutes {
		exclusions = append(exclusions, inventory.LocalPrefixes...)
	}
	if config.AutoExcludeVPNRoutes {
		exclusions = append(exclusions, inventory.VPNPrefixes...)
	}
	exclusions = normalizedPrefixes(exclusions)
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

func resolveLANInterfaces(configured []string, inventory Inventory) ([]string, error) {
	interfaces := append([]string{}, configured...)
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
			return nil, fmt.Errorf("configured LAN interface %q does not exist", name)
		}
	}
	return interfaces, nil
}

func systemDNSIntent(config map[string]any) (bool, int) {
	shared := config
	if nested := object(config["shared"]); len(nested) > 0 {
		shared = nested
	}
	enabled, _ := shared["systemDnsTakeoverEnabled"].(bool)
	port := integer(shared["systemDnsListenPort"])
	if port == 0 {
		port = 53
	}
	return enabled, port
}

func validateSystemDNSInbound(document map[string]any, port int) error {
	inbound, err := findInbound(document, "system-dns-in", "direct")
	if err != nil {
		return err
	}
	if inbound["listen"] != "127.0.0.1" || integer(inbound["listen_port"]) != port {
		return fmt.Errorf("system DNS inbound must listen on 127.0.0.1:%d", port)
	}
	return nil
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

func findProtocolInbound(document map[string]any, tag, protocol string) (map[string]any, error) {
	values, ok := document["inbounds"].([]any)
	if !ok {
		return nil, fmt.Errorf("runtime configuration has no inbounds")
	}
	for _, value := range values {
		inbound, ok := value.(map[string]any)
		if ok && inbound["tag"] == tag && inbound["protocol"] == protocol {
			return inbound, nil
		}
	}
	return nil, fmt.Errorf("runtime configuration is missing %s %s inbound", tag, protocol)
}

func supportedCore(coreID string) bool {
	return coreID == "sing-box" || coreID == "mihomo" || coreID == "xray" || coreID == "v2ray" || coreID == "clash-rs"
}

func coreSupportsMode(coreID, mode string) bool {
	switch mode {
	case subscriptions.TransparentProxyDisabled:
		return true
	case subscriptions.TransparentProxyTUN:
		return coreID == "sing-box" || coreID == "mihomo" || coreID == "clash-rs" || coreID == "xray"
	case subscriptions.TransparentProxyTProxy:
		return supportedCore(coreID)
	default:
		return false
	}
}

func decodeRuntimeDocument(coreID string, data []byte) (map[string]any, error) {
	document := map[string]any{}
	if coreID == "mihomo" || coreID == "clash-rs" {
		return document, yaml.Unmarshal(data, &document)
	}
	return document, json.Unmarshal(data, &document)
}

func encodeRuntimeDocument(coreID string, document map[string]any) ([]byte, error) {
	if coreID == "mihomo" || coreID == "clash-rs" {
		return yaml.Marshal(document)
	}
	encoded, err := json.MarshalIndent(document, "", "  ")
	if err != nil {
		return nil, err
	}
	return append(encoded, '\n'), nil
}

func mihomoDNSListener(document map[string]any, port int) bool {
	values, _ := document["listeners"].([]any)
	for _, value := range values {
		listener, _ := value.(map[string]any)
		if listener["type"] == "tproxy" && integer(listener["port"]) == port {
			return true
		}
	}
	return false
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

func waitForTCP(ctx context.Context, port int, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for {
		connection, err := net.DialTimeout("tcp", net.JoinHostPort("127.0.0.1", fmt.Sprint(port)), 100*time.Millisecond)
		if err == nil {
			_ = connection.Close()
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("TCP port %d is not listening after %s: %w", port, timeout, err)
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(readinessPollInterval):
		}
	}
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

func v2RayServerPrefixes(document map[string]any) []string {
	values, _ := document["outbounds"].([]any)
	result := []string{}
	for _, value := range values {
		outbound, _ := value.(map[string]any)
		settings := object(outbound["settings"])
		if address, err := netip.ParseAddr(strings.TrimSpace(stringValue(settings["address"]))); err == nil {
			result = append(result, netip.PrefixFrom(address, address.BitLen()).String())
		}
		for _, collection := range []string{"servers", "vnext"} {
			servers, _ := settings[collection].([]any)
			for _, serverValue := range servers {
				server, _ := serverValue.(map[string]any)
				if address, err := netip.ParseAddr(strings.TrimSpace(stringValue(server["address"]))); err == nil {
					result = append(result, netip.PrefixFrom(address, address.BitLen()).String())
				}
			}
		}
	}
	return result
}

func validateV2RayRuntimePlan(plan Plan, document map[string]any) error {
	if plan.Mode == subscriptions.TransparentProxyTUN {
		inbound, err := findProtocolInbound(document, "tun-in", "tun")
		if err != nil {
			return err
		}
		settings := object(inbound["settings"])
		if settings["name"] != plan.TUNInterface || firstString(settings["gateway"]) == "" || firstString(settings["autoSystemRoutingTable"]) == "" || settings["autoOutboundsInterface"] != "auto" {
			return fmt.Errorf("Xray TUN automatic routing is incomplete")
		}
		return nil
	}
	if _, err := findProtocolInbound(document, "tproxy-in", "dokodemo-door"); err != nil {
		return err
	}
	if _, err := findProtocolInbound(document, "dns-in", "dokodemo-door"); err != nil {
		return err
	}
	values, _ := document["outbounds"].([]any)
	for _, value := range values {
		outbound, _ := value.(map[string]any)
		if number, ok := numberAsUint32(object(object(outbound["streamSettings"])["sockopt"])["mark"]); !ok || number != BypassMark {
			return fmt.Errorf("outbound %v does not use the Sempre bypass mark", outbound["tag"])
		}
	}
	return nil
}

func validateV2RaySplitDNS(document map[string]any) error {
	dns := object(document["dns"])
	servers, _ := dns["servers"].([]any)
	hasLocal, hasRemote := false, false
	for _, value := range servers {
		server, _ := value.(map[string]any)
		switch server["tag"] {
		case "local-dns":
			hasLocal = stringListContains(server["domains"], "geosite:cn")
		case "remote-dns":
			hasRemote = strings.HasPrefix(stringValue(server["address"]), "https://")
		}
	}
	if !hasLocal || !hasRemote {
		return fmt.Errorf("domestic local DNS or foreign DoH is missing")
	}
	return nil
}

func validateV2RaySplitRouting(document map[string]any) error {
	routing := object(document["routing"])
	rules, _ := routing["rules"].([]any)
	hasDomestic, hasForeign := false, false
	for _, value := range rules {
		rule, _ := value.(map[string]any)
		if rule["outboundTag"] == "direct" && (stringListContains(rule["domain"], "geosite:cn") || stringListContains(rule["ip"], "geoip:cn")) {
			hasDomestic = true
		}
		if rule["balancerTag"] != nil && (rule["network"] == "tcp,udp" || stringListContains(rule["inboundTag"], "remote-dns")) {
			hasForeign = true
		}
	}
	if !hasDomestic || !hasForeign {
		return fmt.Errorf("domestic direct or foreign balancer route is missing")
	}
	return nil
}

func stringValue(value any) string {
	result, _ := value.(string)
	return result
}

func mihomoServerPrefixes(document map[string]any) []string {
	values, _ := document["proxies"].([]any)
	result := []string{}
	for _, value := range values {
		proxy, _ := value.(map[string]any)
		server, _ := proxy["server"].(string)
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

func firstString(value any) string {
	switch values := value.(type) {
	case []any:
		if len(values) > 0 {
			result, _ := values[0].(string)
			return result
		}
	case []string:
		if len(values) > 0 {
			return values[0]
		}
	case string:
		return values
	}
	return ""
}

func numberAsUint32(value any) (uint32, bool) {
	switch number := value.(type) {
	case float64:
		if number >= 0 && number <= float64(^uint32(0)) && number == float64(uint32(number)) {
			return uint32(number), true
		}
	case uint32:
		return number, true
	case int:
		if number >= 0 && uint64(number) <= uint64(^uint32(0)) {
			return uint32(number), true
		}
	}
	return 0, false
}

func integer(value any) int {
	switch number := value.(type) {
	case int:
		return number
	case uint32:
		return int(number)
	case float64:
		return int(number)
	case uint64:
		return int(number)
	default:
		return 0
	}
}

func stringValues(value any) []string {
	switch values := value.(type) {
	case []string:
		return values
	case []any:
		result := make([]string, 0, len(values))
		for _, value := range values {
			if text, ok := value.(string); ok {
				result = append(result, text)
			}
		}
		return result
	case string:
		return []string{values}
	default:
		return nil
	}
}

func stringListContains(value any, expected string) bool {
	switch values := value.(type) {
	case []any:
		for _, item := range values {
			if item == expected {
				return true
			}
		}
	case []string:
		for _, item := range values {
			if item == expected {
				return true
			}
		}
	}
	return false
}
