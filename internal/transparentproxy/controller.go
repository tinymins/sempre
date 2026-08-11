package transparentproxy

import (
	"context"
	"errors"
	"fmt"
	"os"
	"time"

	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
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
	SystemDNSHosts   []string
	TUNInterface     string
	TUNAddress       string
	RouteExclusions  []string
	TProxyPort       int
	DNSPort          int
	CaptureHost      bool
	LANInterfaces    []string
	ExcludedPrefixes []string
	FakeIPPrefixes   []string
	FakeIPConflicts  []string
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
	systemDNS, systemDNSPort, systemDNSHosts := systemDNSIntent(profile.DNS)
	plan.SystemDNS = systemDNS
	plan.SystemDNSPort = systemDNSPort
	plan.SystemDNSHosts = systemDNSHosts
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
		if err := validateSystemDNSInbounds(document, plan.SystemDNSPort, plan.SystemDNSHosts); err != nil {
			return Plan{}, err
		}
	}
	fakeIPPrefixes := fakeIPPrefixesForCore(coreID, document)
	switch transparent.Mode {
	case subscriptions.TransparentProxyTUN:
		switch coreID {
		case "sing-box":
			plan, err = prepareTUN(plan, transparent, inventory, document, fakeIPPrefixes)
		case "mihomo":
			plan, err = prepareMihomoTUN(plan, transparent, inventory, document, fakeIPPrefixes)
		case "clash-rs":
			plan, err = prepareClashRSTUN(plan, transparent, inventory, document, fakeIPPrefixes)
		case "xray":
			plan, err = prepareXrayTUN(plan, transparent, inventory, document, fakeIPPrefixes)
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
		if err := waitForSystemDNS(ctx, plan.SystemDNSHosts, plan.SystemDNSPort, listenerReadinessTimeout); err != nil {
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
	diagnostics = append(diagnostics, fakeIPDiagnostics(plan)...)
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
		plan.FakeIPPrefixes = fakeIPPrefixesForCore(coreID, document)
		plan.RouteExclusions = runtimeRouteExclusions(coreID, document)
		plan.FakeIPConflicts = fakeIPRouteConflicts(plan.FakeIPPrefixes, inventory)
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
