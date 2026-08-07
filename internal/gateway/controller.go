package gateway

import (
	"context"
	"errors"
	"fmt"
	"net/netip"
	"sync"
	"time"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/transparentproxy"
)

type Controller struct {
	store     *Store
	inventory func(context.Context) (transparentproxy.Inventory, error)
	mu        sync.Mutex
	dns       *DNSServer
	dhcp      *DHCPServer
	startedAt time.Time
	lastError string
	active    Config
}

func New(paths layout.Layout, inventory func(context.Context) (transparentproxy.Inventory, error)) (*Controller, error) {
	store := NewStore(paths)
	if err := store.Initialize(); err != nil {
		return nil, err
	}
	return &Controller{store: store, inventory: inventory}, nil
}

func (controller *Controller) Read() (Config, error) {
	return controller.store.Read()
}

func (controller *Controller) Update(config Config) (Config, error) {
	return controller.store.Update(config)
}

func (controller *Controller) Status(ctx context.Context, transparent any) (Status, error) {
	config, err := controller.Read()
	if err != nil {
		return Status{}, err
	}
	inventory := transparentproxy.Inventory{}
	if controller.inventory != nil {
		inventory, _ = controller.inventory(ctx)
	}
	return Status{
		Config:            config,
		Runtime:           controller.RuntimeStatus(),
		Inventory:         inventory,
		ValidationErrors:  ValidationMessages(config),
		TransparentProxy:  transparent,
		HostPlanAvailable: true,
	}, nil
}

func (controller *Controller) RuntimeStatus() RuntimeStatus {
	controller.mu.Lock()
	defer controller.mu.Unlock()
	var started *time.Time
	if !controller.startedAt.IsZero() {
		value := controller.startedAt
		started = &value
	}
	leases := []LeaseView{}
	if controller.dhcp != nil {
		leases = controller.dhcp.Leases()
	}
	return RuntimeStatus{
		DNSRunning:  controller.dns != nil && controller.dns.Running(),
		DHCPRunning: controller.dhcp != nil && controller.dhcp.Running(),
		StartedAt:   started,
		DHCPLeases:  leases,
		LastError:   controller.lastError,
	}
}

func (controller *Controller) Start(ctx context.Context, config Config) error {
	config.Normalize()
	if err := config.Validate(); err != nil {
		return err
	}
	if !config.DNS.Enabled && !config.DHCP.Enabled {
		return controller.Stop(ctx)
	}
	controller.mu.Lock()
	defer controller.mu.Unlock()
	controller.stopLocked(ctx)
	var failures []error
	if config.DNS.Enabled {
		dnsConfig, err := resolveDNSRuleSets(ctx, config.DNS)
		if err != nil {
			failures = append(failures, err)
			dnsConfig = config.DNS
		}
		dnsServer, err := NewDNSServer(dnsConfig)
		if err != nil {
			failures = append(failures, err)
		} else if err := dnsServer.Start(); err != nil {
			failures = append(failures, err)
		} else {
			controller.dns = dnsServer
		}
	}
	if config.DHCP.Enabled {
		dhcpServer, err := NewDHCPServer(config)
		if err != nil {
			failures = append(failures, err)
		} else if err := dhcpServer.Start(); err != nil {
			failures = append(failures, err)
		} else {
			controller.dhcp = dhcpServer
		}
	}
	if err := errors.Join(failures...); err != nil {
		controller.stopLocked(ctx)
		controller.lastError = err.Error()
		return err
	}
	controller.active = config
	controller.startedAt = time.Now().UTC()
	controller.lastError = ""
	return nil
}

func (controller *Controller) Stop(ctx context.Context) error {
	controller.mu.Lock()
	defer controller.mu.Unlock()
	return controller.stopLocked(ctx)
}

func (controller *Controller) stopLocked(_ context.Context) error {
	var failures []error
	if controller.dns != nil {
		failures = append(failures, controller.dns.Stop())
		controller.dns = nil
	}
	if controller.dhcp != nil {
		failures = append(failures, controller.dhcp.Stop())
		controller.dhcp = nil
	}
	controller.startedAt = time.Time{}
	controller.active = Config{}
	return errors.Join(failures...)
}

func (controller *Controller) QueryDNS(ctx context.Context, name, recordType string) (DNSDebugResult, error) {
	config, err := controller.Read()
	if err != nil {
		return DNSDebugResult{}, err
	}
	dnsConfig, err := resolveDNSRuleSets(ctx, config.DNS)
	if err != nil {
		return DNSDebugResult{}, err
	}
	server, err := NewDNSServer(dnsConfig)
	if err != nil {
		return DNSDebugResult{}, err
	}
	return server.DebugQuery(ctx, name, recordType)
}

func (controller *Controller) RevokeLease(mac string) error {
	controller.mu.Lock()
	defer controller.mu.Unlock()
	if controller.dhcp == nil {
		return fmt.Errorf("DHCP server is not running")
	}
	return controller.dhcp.Revoke(mac)
}

func gatewayAddr(config Config) (netip.Addr, error) {
	prefix, err := netip.ParsePrefix(config.LAN.GatewayCIDR)
	if err != nil {
		return netip.Addr{}, err
	}
	return prefix.Addr(), nil
}
