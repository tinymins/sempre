package gateway

import (
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/netip"
	"os"
	"slices"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	"github.com/tinymins/sempre/internal/transparentproxy"
)

const SchemaVersion = 1

const (
	TopologyLocalPVE  = "local-pve"
	TopologyRemotePVE = "remote-pve"

	DNSStrategyLocalFirst = "local-first-classify"
	DNSStrategyRulesFirst = "rules-first"
)

type Config struct {
	Schema   int        `json:"schema"`
	Topology string     `json:"topology"`
	LAN      LANConfig  `json:"lan"`
	DHCP     DHCPConfig `json:"dhcp"`
	DNS      DNSConfig  `json:"dns"`
	PVE      PVEConfig  `json:"pve"`
}

type LANConfig struct {
	Interface    string `json:"interface"`
	GatewayCIDR  string `json:"gateway_cidr"`
	WANInterface string `json:"wan_interface"`
	NATEnabled   bool   `json:"nat_enabled"`
}

type DHCPConfig struct {
	Enabled      bool              `json:"enabled"`
	RangeStart   string            `json:"range_start"`
	RangeEnd     string            `json:"range_end"`
	LeaseTime    string            `json:"lease_time"`
	Domain       string            `json:"domain,omitempty"`
	Reservations []DHCPReservation `json:"reservations"`
}

type DHCPReservation struct {
	MAC      string `json:"mac"`
	IP       string `json:"ip"`
	Hostname string `json:"hostname,omitempty"`
}

type DNSConfig struct {
	Enabled         bool         `json:"enabled"`
	ListenHosts     []string     `json:"listen_hosts"`
	ListenPort      int          `json:"listen_port"`
	LocalUpstreams  []string     `json:"local_upstreams"`
	RemoteUpstream  string       `json:"remote_upstream"`
	Strategy        string       `json:"strategy"`
	RejectHTTPS     bool         `json:"reject_https"`
	RuleSets        []DNSRuleSet `json:"rule_sets"`
	DomesticCIDRs   []string     `json:"domestic_cidrs"`
	CacheTTLSeconds int          `json:"cache_ttl_seconds"`
}

type DNSRuleSet struct {
	ID       string   `json:"id"`
	Name     string   `json:"name"`
	Enabled  bool     `json:"enabled"`
	Type     string   `json:"type"`
	URL      string   `json:"url,omitempty"`
	Rules    []string `json:"rules,omitempty"`
	Upstream string   `json:"upstream"`
}

type PVEConfig struct {
	Host            string `json:"host,omitempty"`
	Port            int    `json:"port,omitempty"`
	User            string `json:"user,omitempty"`
	KeyPath         string `json:"key_path,omitempty"`
	Fingerprint     string `json:"fingerprint,omitempty"`
	ApplyPersistent bool   `json:"apply_persistent"`
}

type Status struct {
	Config            Config                     `json:"config"`
	Runtime           RuntimeStatus              `json:"runtime"`
	Inventory         transparentproxy.Inventory `json:"inventory"`
	ValidationErrors  []string                   `json:"validation_errors"`
	TransparentProxy  any                        `json:"transparent_proxy,omitempty"`
	HostPlanAvailable bool                       `json:"host_plan_available"`
}

type RuntimeStatus struct {
	DNSRunning  bool        `json:"dns_running"`
	DHCPRunning bool        `json:"dhcp_running"`
	StartedAt   *time.Time  `json:"started_at"`
	DHCPLeases  []LeaseView `json:"dhcp_leases"`
	LastError   string      `json:"last_error,omitempty"`
}

type LeaseView struct {
	MAC       string     `json:"mac"`
	IP        string     `json:"ip"`
	Hostname  string     `json:"hostname,omitempty"`
	ExpiresAt *time.Time `json:"expires_at,omitempty"`
	Reserved  bool       `json:"reserved"`
}

type Store struct {
	path string
}

func NewStore(paths layout.Layout) *Store {
	return &Store{path: paths.GatewayConfig}
}

func DefaultConfig() Config {
	return Config{
		Schema:   SchemaVersion,
		Topology: TopologyLocalPVE,
		LAN: LANConfig{
			Interface:   "",
			GatewayCIDR: "10.10.10.1/24",
			NATEnabled:  false,
		},
		DHCP: DHCPConfig{
			Enabled:      false,
			RangeStart:   "10.10.10.100",
			RangeEnd:     "10.10.10.200",
			LeaseTime:    "12h",
			Reservations: []DHCPReservation{},
		},
		DNS: DNSConfig{
			Enabled:         false,
			ListenHosts:     []string{"10.10.10.1"},
			ListenPort:      53,
			LocalUpstreams:  []string{"223.5.5.5:53", "223.6.6.6:53"},
			RemoteUpstream:  "127.0.0.1:1053",
			Strategy:        DNSStrategyLocalFirst,
			RejectHTTPS:     true,
			RuleSets:        []DNSRuleSet{},
			DomesticCIDRs:   defaultDomesticCIDRs(),
			CacheTTLSeconds: 300,
		},
		PVE: PVEConfig{Port: 22, User: "root"},
	}
}

func (store *Store) Initialize() error {
	_, err := store.Read()
	return err
}

func (store *Store) Read() (Config, error) {
	data, err := os.ReadFile(store.path)
	if errors.Is(err, os.ErrNotExist) {
		config := DefaultConfig()
		return config, writeConfig(store.path, config)
	}
	if err != nil {
		return Config{}, fmt.Errorf("read gateway configuration: %w", err)
	}
	var config Config
	if err := json.Unmarshal(data, &config); err != nil {
		return Config{}, fmt.Errorf("decode gateway configuration: %w", err)
	}
	config.Normalize()
	if err := config.Validate(); err != nil {
		return Config{}, err
	}
	return config, nil
}

func (store *Store) Update(config Config) (Config, error) {
	config.Normalize()
	if err := config.Validate(); err != nil {
		return Config{}, err
	}
	return config, writeConfig(store.path, config)
}

func writeConfig(path string, config Config) error {
	data, err := json.MarshalIndent(config, "", "  ")
	if err != nil {
		return err
	}
	return state.WriteAtomic(path, append(data, '\n'), 0o600)
}

func (config *Config) Normalize() {
	defaults := DefaultConfig()
	config.Schema = SchemaVersion
	if config.Topology == "" {
		config.Topology = defaults.Topology
	}
	if config.LAN.GatewayCIDR == "" {
		config.LAN.GatewayCIDR = defaults.LAN.GatewayCIDR
	}
	if config.DHCP.RangeStart == "" {
		config.DHCP.RangeStart = defaults.DHCP.RangeStart
	}
	if config.DHCP.RangeEnd == "" {
		config.DHCP.RangeEnd = defaults.DHCP.RangeEnd
	}
	if config.DHCP.LeaseTime == "" {
		config.DHCP.LeaseTime = defaults.DHCP.LeaseTime
	}
	if config.DHCP.Reservations == nil {
		config.DHCP.Reservations = []DHCPReservation{}
	}
	if config.DNS.ListenPort == 0 {
		config.DNS.ListenPort = defaults.DNS.ListenPort
	}
	if len(config.DNS.ListenHosts) == 0 {
		config.DNS.ListenHosts = defaults.DNS.ListenHosts
	}
	if len(config.DNS.LocalUpstreams) == 0 {
		config.DNS.LocalUpstreams = defaults.DNS.LocalUpstreams
	}
	if config.DNS.RemoteUpstream == "" {
		config.DNS.RemoteUpstream = defaults.DNS.RemoteUpstream
	}
	if config.DNS.Strategy == "" {
		config.DNS.Strategy = defaults.DNS.Strategy
	}
	if config.DNS.DomesticCIDRs == nil {
		config.DNS.DomesticCIDRs = defaults.DNS.DomesticCIDRs
	}
	if config.DNS.RuleSets == nil {
		config.DNS.RuleSets = []DNSRuleSet{}
	}
	if config.DNS.CacheTTLSeconds == 0 {
		config.DNS.CacheTTLSeconds = defaults.DNS.CacheTTLSeconds
	}
	if config.PVE.Port == 0 {
		config.PVE.Port = defaults.PVE.Port
	}
	if config.PVE.User == "" {
		config.PVE.User = defaults.PVE.User
	}
}

func (config Config) Validate() error {
	return errors.Join(validateConfig(config)...)
}

func validateConfig(config Config) []error {
	failures := []error{}
	if config.Schema != SchemaVersion {
		failures = append(failures, fmt.Errorf("unsupported gateway schema %d", config.Schema))
	}
	if config.Topology != TopologyLocalPVE && config.Topology != TopologyRemotePVE {
		failures = append(failures, fmt.Errorf("invalid gateway topology %q", config.Topology))
	}
	gateway, err := netip.ParsePrefix(config.LAN.GatewayCIDR)
	if err != nil || !gateway.Addr().Is4() {
		failures = append(failures, fmt.Errorf("LAN gateway CIDR must be an IPv4 prefix"))
	}
	start, startErr := netip.ParseAddr(config.DHCP.RangeStart)
	end, endErr := netip.ParseAddr(config.DHCP.RangeEnd)
	if startErr != nil || endErr != nil || !start.Is4() || !end.Is4() {
		failures = append(failures, fmt.Errorf("DHCP range must contain IPv4 addresses"))
	} else if err == nil {
		if !gateway.Contains(start) || !gateway.Contains(end) || start.Compare(end) > 0 {
			failures = append(failures, fmt.Errorf("DHCP range must be inside LAN gateway CIDR and ordered"))
		}
	}
	if _, err := time.ParseDuration(config.DHCP.LeaseTime); err != nil {
		failures = append(failures, fmt.Errorf("invalid DHCP lease time: %w", err))
	}
	for _, reservation := range config.DHCP.Reservations {
		if _, err := net.ParseMAC(reservation.MAC); err != nil {
			failures = append(failures, fmt.Errorf("invalid DHCP reservation MAC %q", reservation.MAC))
		}
		ip, err := netip.ParseAddr(reservation.IP)
		if err != nil || !ip.Is4() {
			failures = append(failures, fmt.Errorf("invalid DHCP reservation IP %q", reservation.IP))
		}
	}
	if config.DNS.ListenPort < 1 || config.DNS.ListenPort > 65535 {
		failures = append(failures, fmt.Errorf("DNS listen port must be between 1 and 65535"))
	}
	for _, host := range config.DNS.ListenHosts {
		if host != "0.0.0.0" {
			if ip, err := netip.ParseAddr(host); err != nil || !ip.Is4() {
				failures = append(failures, fmt.Errorf("DNS listen host %q must be an IPv4 address", host))
			}
		}
	}
	for _, upstream := range append(append([]string{}, config.DNS.LocalUpstreams...), config.DNS.RemoteUpstream) {
		if err := validateUpstream(upstream); err != nil {
			failures = append(failures, err)
		}
	}
	if !slices.Contains([]string{DNSStrategyLocalFirst, DNSStrategyRulesFirst}, config.DNS.Strategy) {
		failures = append(failures, fmt.Errorf("invalid DNS strategy %q", config.DNS.Strategy))
	}
	for _, value := range config.DNS.DomesticCIDRs {
		if prefix, err := netip.ParsePrefix(value); err != nil || !prefix.Addr().Is4() {
			failures = append(failures, fmt.Errorf("domestic CIDR %q must be an IPv4 prefix", value))
		}
	}
	for _, ruleSet := range config.DNS.RuleSets {
		if strings.TrimSpace(ruleSet.ID) == "" || strings.TrimSpace(ruleSet.Name) == "" {
			failures = append(failures, fmt.Errorf("DNS rule sets require ID and name"))
		}
		if ruleSet.Upstream != "" && ruleSet.Upstream != "local" && ruleSet.Upstream != "remote" {
			if err := validateUpstream(ruleSet.Upstream); err != nil {
				failures = append(failures, err)
			}
		}
		if ruleSet.Type != "" && ruleSet.Type != "inline" && ruleSet.Type != "url" {
			failures = append(failures, fmt.Errorf("unsupported DNS rule set type %q", ruleSet.Type))
		}
	}
	if config.PVE.Port < 0 || config.PVE.Port > 65535 {
		failures = append(failures, fmt.Errorf("PVE SSH port must be between 1 and 65535"))
	}
	return failures
}

func validateUpstream(value string) error {
	if strings.TrimSpace(value) == "" {
		return fmt.Errorf("DNS upstream cannot be empty")
	}
	host, port, err := net.SplitHostPort(value)
	if err != nil || host == "" || port == "" {
		return fmt.Errorf("DNS upstream %q must be host:port", value)
	}
	return nil
}

func ValidationMessages(config Config) []string {
	config.Normalize()
	failures := validateConfig(config)
	result := make([]string, 0, len(failures))
	for _, failure := range failures {
		result = append(result, failure.Error())
	}
	return result
}

func defaultDomesticCIDRs() []string {
	return []string{
		"10.0.0.0/8",
		"100.64.0.0/10",
		"127.0.0.0/8",
		"172.16.0.0/12",
		"192.168.0.0/16",
	}
}
