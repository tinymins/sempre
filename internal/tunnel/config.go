package tunnel

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/url"
	"os"
	"regexp"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
)

const SchemaVersion = 1

const (
	DesiredRunning = "running"
	DesiredStopped = "stopped"
)

var idPattern = regexp.MustCompile(`^[a-z0-9][a-z0-9-]{0,62}$`)

type Config struct {
	Schema    int        `json:"schema"`
	Instances []Instance `json:"instances"`
}

type Instance struct {
	ID                        string    `json:"id"`
	Name                      string    `json:"name"`
	DesiredState              string    `json:"desired_state"`
	ServerURL                 string    `json:"server_url"`
	DNSResolvers              []string  `json:"dns_resolvers"`
	PreferIPv4                bool      `json:"prefer_ipv4"`
	WebsocketPing             string    `json:"websocket_ping"`
	ConnectionRetryMaxBackoff string    `json:"connection_retry_max_backoff"`
	UpgradePathPrefix         string    `json:"upgrade_path_prefix,omitempty"`
	Forwards                  []Forward `json:"forwards"`
}

type Forward struct {
	ID             string `json:"id"`
	Name           string `json:"name"`
	ListenPort     int    `json:"listen_port"`
	RemoteHost     string `json:"remote_host"`
	RemotePort     int    `json:"remote_port"`
	TimeoutSeconds int    `json:"timeout_seconds"`
}

type ForwardEndpoint struct {
	InstanceID   string `json:"instance_id"`
	InstanceName string `json:"instance_name"`
	ForwardID    string `json:"forward_id"`
	ForwardName  string `json:"forward_name"`
	Host         string `json:"host"`
	Port         int    `json:"port"`
}

type Store struct {
	path string
}

func NewStore(paths layout.Layout) *Store {
	return &Store{path: paths.TunnelConfig}
}

func DefaultConfig() Config {
	return Config{Schema: SchemaVersion, Instances: []Instance{}}
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
		return Config{}, fmt.Errorf("read tunnel configuration: %w", err)
	}
	var config Config
	if err := json.Unmarshal(data, &config); err != nil {
		return Config{}, fmt.Errorf("decode tunnel configuration: %w", err)
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
	config.Schema = SchemaVersion
	if config.Instances == nil {
		config.Instances = []Instance{}
	}
	for index := range config.Instances {
		instance := &config.Instances[index]
		instance.ID = strings.TrimSpace(instance.ID)
		instance.Name = strings.TrimSpace(instance.Name)
		instance.ServerURL = strings.TrimSpace(instance.ServerURL)
		instance.UpgradePathPrefix = strings.TrimSpace(instance.UpgradePathPrefix)
		if instance.DesiredState == "" {
			instance.DesiredState = DesiredStopped
		}
		if instance.WebsocketPing == "" {
			instance.WebsocketPing = "15s"
		}
		if instance.ConnectionRetryMaxBackoff == "" {
			instance.ConnectionRetryMaxBackoff = "30s"
		}
		if instance.DNSResolvers == nil {
			instance.DNSResolvers = []string{}
		}
		if instance.Forwards == nil {
			instance.Forwards = []Forward{}
		}
		for resolverIndex := range instance.DNSResolvers {
			instance.DNSResolvers[resolverIndex] = strings.TrimSpace(instance.DNSResolvers[resolverIndex])
		}
		for forwardIndex := range instance.Forwards {
			forward := &instance.Forwards[forwardIndex]
			forward.ID = strings.TrimSpace(forward.ID)
			forward.Name = strings.TrimSpace(forward.Name)
			forward.RemoteHost = strings.TrimSpace(forward.RemoteHost)
			if forward.RemoteHost == "" {
				forward.RemoteHost = "127.0.0.1"
			}
		}
	}
}

func (config Config) Validate() error {
	if config.Schema != SchemaVersion {
		return fmt.Errorf("unsupported tunnel schema %d", config.Schema)
	}
	instanceIDs := map[string]bool{}
	forwardIDs := map[string]bool{}
	listenPorts := map[int]bool{}
	var failures []error
	for _, instance := range config.Instances {
		if !idPattern.MatchString(instance.ID) {
			failures = append(failures, fmt.Errorf("invalid tunnel instance ID %q", instance.ID))
		} else if instanceIDs[instance.ID] {
			failures = append(failures, fmt.Errorf("duplicate tunnel instance ID %q", instance.ID))
		}
		instanceIDs[instance.ID] = true
		if instance.Name == "" {
			failures = append(failures, fmt.Errorf("tunnel instance %q requires a name", instance.ID))
		}
		if instance.DesiredState != DesiredRunning && instance.DesiredState != DesiredStopped {
			failures = append(failures, fmt.Errorf("tunnel instance %q has invalid desired state", instance.ID))
		}
		if err := validateServerURL(instance.ServerURL); err != nil {
			failures = append(failures, fmt.Errorf("tunnel instance %q: %w", instance.ID, err))
		}
		if err := validateDuration(instance.WebsocketPing, time.Second); err != nil {
			failures = append(failures, fmt.Errorf("tunnel instance %q websocket ping: %w", instance.ID, err))
		}
		if err := validateDuration(instance.ConnectionRetryMaxBackoff, time.Second); err != nil {
			failures = append(failures, fmt.Errorf("tunnel instance %q retry backoff: %w", instance.ID, err))
		}
		for _, resolver := range instance.DNSResolvers {
			if err := validateResolver(resolver); err != nil {
				failures = append(failures, fmt.Errorf("tunnel instance %q: %w", instance.ID, err))
			}
		}
		if instance.DesiredState == DesiredRunning && len(instance.Forwards) == 0 {
			failures = append(failures, fmt.Errorf("running tunnel instance %q requires a forward", instance.ID))
		}
		for _, forward := range instance.Forwards {
			if !idPattern.MatchString(forward.ID) {
				failures = append(failures, fmt.Errorf("invalid tunnel forward ID %q", forward.ID))
			} else if forwardIDs[forward.ID] {
				failures = append(failures, fmt.Errorf("duplicate tunnel forward ID %q", forward.ID))
			}
			forwardIDs[forward.ID] = true
			if forward.Name == "" {
				failures = append(failures, fmt.Errorf("tunnel forward %q requires a name", forward.ID))
			}
			if forward.ListenPort < 1 || forward.ListenPort > 65535 || forward.RemotePort < 1 || forward.RemotePort > 65535 {
				failures = append(failures, fmt.Errorf("tunnel forward %q ports must be between 1 and 65535", forward.ID))
			}
			if listenPorts[forward.ListenPort] {
				failures = append(failures, fmt.Errorf("tunnel listen port %d is duplicated", forward.ListenPort))
			}
			listenPorts[forward.ListenPort] = true
			if forward.RemoteHost == "" || strings.ContainsAny(forward.RemoteHost, " \t\r\n") || (strings.Contains(forward.RemoteHost, ":") && net.ParseIP(forward.RemoteHost) == nil) {
				failures = append(failures, fmt.Errorf("tunnel forward %q has invalid remote host", forward.ID))
			}
			if forward.TimeoutSeconds < 0 {
				failures = append(failures, fmt.Errorf("tunnel forward %q timeout cannot be negative", forward.ID))
			}
		}
	}
	return errors.Join(failures...)
}

func validateServerURL(value string) error {
	parsed, err := url.Parse(value)
	if err != nil || parsed.Scheme != "wss" || parsed.Hostname() == "" || parsed.User != nil {
		return fmt.Errorf("server URL must be an absolute wss:// URL without credentials")
	}
	return nil
}

func validateResolver(value string) error {
	parsed, err := url.Parse(value)
	if err != nil || parsed.Hostname() == "" || parsed.User != nil {
		return fmt.Errorf("invalid DNS resolver %q", value)
	}
	switch parsed.Scheme {
	case "dns", "dns+https", "dns+tls", "system":
		return nil
	default:
		return fmt.Errorf("unsupported DNS resolver %q", value)
	}
}

func validateDuration(value string, minimum time.Duration) error {
	duration, err := time.ParseDuration(value)
	if err != nil || duration < minimum {
		return fmt.Errorf("must be a duration of at least %s", minimum)
	}
	return nil
}

func (config Config) Forward(id string) (ForwardEndpoint, bool) {
	for _, instance := range config.Instances {
		for _, forward := range instance.Forwards {
			if forward.ID == id {
				return ForwardEndpoint{InstanceID: instance.ID, InstanceName: instance.Name, ForwardID: forward.ID, ForwardName: forward.Name, Host: "127.0.0.1", Port: forward.ListenPort}, true
			}
		}
	}
	return ForwardEndpoint{}, false
}

func (config Config) ForwardEndpoints() []ForwardEndpoint {
	result := []ForwardEndpoint{}
	for _, instance := range config.Instances {
		for _, forward := range instance.Forwards {
			result = append(result, ForwardEndpoint{InstanceID: instance.ID, InstanceName: instance.Name, ForwardID: forward.ID, ForwardName: forward.Name, Host: "127.0.0.1", Port: forward.ListenPort})
		}
	}
	return result
}

func NewID(prefix string) string {
	data := make([]byte, 6)
	if _, err := rand.Read(data); err != nil {
		panic(fmt.Sprintf("generate tunnel ID: %v", err))
	}
	return strings.TrimSuffix(prefix, "-") + "-" + hex.EncodeToString(data)
}

func BuildArgs(instance Instance) []string {
	args := []string{"--no-color", "client", "--tls-verify-certificate", "--websocket-ping-frequency", instance.WebsocketPing, "--connection-retry-max-backoff", instance.ConnectionRetryMaxBackoff}
	if instance.PreferIPv4 {
		args = append(args, "--dns-resolver-prefer-ipv4")
	}
	for _, resolver := range instance.DNSResolvers {
		args = append(args, "--dns-resolver", resolver)
	}
	if instance.UpgradePathPrefix != "" {
		args = append(args, "--http-upgrade-path-prefix", instance.UpgradePathPrefix)
	}
	for _, forward := range instance.Forwards {
		remote := net.JoinHostPort(forward.RemoteHost, fmt.Sprint(forward.RemotePort))
		value := fmt.Sprintf("udp://127.0.0.1:%d:%s?timeout_sec=%d", forward.ListenPort, remote, forward.TimeoutSeconds)
		args = append(args, "--local-to-remote", value)
	}
	return append(args, instance.ServerURL)
}
