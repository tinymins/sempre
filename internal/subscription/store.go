package subscription

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/netip"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
)

type Store struct {
	paths layout.Layout
	mu    sync.Mutex
}

type legacyCatalogConfiguration struct {
	Profiles []legacyProfileConfiguration `json:"profiles"`
}

type legacyProfileConfiguration struct {
	CustomConfig map[string]any       `json:"custom_config"`
	ClashAPI     *ManagementAPIConfig `json:"clash_api"`
}

func NewStore(paths layout.Layout) *Store {
	return &Store{paths: paths}
}

func (store *Store) Initialize(legacyURL string) error {
	store.mu.Lock()
	defer store.mu.Unlock()
	if err := os.MkdirAll(store.paths.SubscriptionBlobs, 0o700); err != nil {
		return err
	}
	if err := os.MkdirAll(store.paths.SubscriptionCache, 0o700); err != nil {
		return err
	}
	if _, err := os.Stat(store.paths.SubscriptionStore); err == nil {
		_, err = store.readUnlocked()
		return err
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	return store.writeUnlocked(NewCatalog(legacyURL))
}

func (store *Store) Read() (Catalog, error) {
	store.mu.Lock()
	defer store.mu.Unlock()
	return store.readUnlocked()
}

func (store *Store) Update(change func(*Catalog) error) error {
	store.mu.Lock()
	defer store.mu.Unlock()
	catalog, err := store.readUnlocked()
	if err != nil {
		return err
	}
	encoded, err := json.Marshal(catalog)
	if err != nil {
		return err
	}
	var candidate Catalog
	if err := json.Unmarshal(encoded, &candidate); err != nil {
		return err
	}
	if err := change(&candidate); err != nil {
		return err
	}
	return store.writeUnlocked(candidate)
}

func (store *Store) SaveBlob(data []byte) (string, error) {
	if int64(len(data)) > MaxSourceSize {
		return "", fmt.Errorf("subscription response exceeds %d bytes", MaxSourceSize)
	}
	sum := sha256.Sum256(data)
	hash := hex.EncodeToString(sum[:])
	path := filepath.Join(store.paths.SubscriptionBlobs, hash)
	if _, err := os.Stat(path); err == nil {
		return hash, nil
	} else if !errors.Is(err, os.ErrNotExist) {
		return "", err
	}
	if err := state.WriteAtomic(path, data, 0o600); err != nil {
		return "", err
	}
	return hash, nil
}

func (store *Store) ReadBlob(hash string) ([]byte, error) {
	if len(hash) != 64 {
		return nil, fmt.Errorf("invalid content hash")
	}
	data, err := os.ReadFile(filepath.Join(store.paths.SubscriptionBlobs, strings.ToLower(hash)))
	if err != nil {
		return nil, fmt.Errorf("read subscription snapshot: %w", err)
	}
	return data, nil
}

func (store *Store) CachePath(key string) string {
	sum := sha256.Sum256([]byte(key))
	return filepath.Join(store.paths.SubscriptionCache, hex.EncodeToString(sum[:])+".json")
}

func (store *Store) ClearCache() error {
	entries, err := os.ReadDir(store.paths.SubscriptionCache)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	for _, entry := range entries {
		if entry.Type().IsRegular() {
			if err := os.Remove(filepath.Join(store.paths.SubscriptionCache, entry.Name())); err != nil {
				return err
			}
		}
	}
	return nil
}

func (store *Store) readUnlocked() (Catalog, error) {
	data, err := os.ReadFile(store.paths.SubscriptionStore)
	if err != nil {
		return Catalog{}, fmt.Errorf("read subscription catalog: %w", err)
	}
	var catalog Catalog
	if err := json.Unmarshal(data, &catalog); err != nil {
		return Catalog{}, fmt.Errorf("decode subscription catalog: %w", err)
	}
	if catalog.Schema > 0 && catalog.Schema < CatalogSchema {
		previousSchema := catalog.Schema
		legacy := legacyCatalogConfiguration{}
		if previousSchema < 5 {
			if err := json.Unmarshal(data, &legacy); err != nil {
				return Catalog{}, fmt.Errorf("decode legacy subscription configuration: %w", err)
			}
		}
		catalog.Schema = CatalogSchema
		for index := range catalog.Profiles {
			if previousSchema < 4 {
				migrateLinuxRuntimeConfig(&catalog.Profiles[index])
			}
			if previousSchema < 5 {
				configuration := legacyProfileConfiguration{}
				if index < len(legacy.Profiles) {
					configuration = legacy.Profiles[index]
				}
				migrateCoreConfiguration(&catalog.Profiles[index], configuration)
			}
			normalizeProfile(&catalog.Profiles[index])
		}
	}
	if err := validateCatalog(catalog); err != nil {
		return Catalog{}, fmt.Errorf("validate subscription catalog: %w", err)
	}
	return catalog, nil
}

func (store *Store) writeUnlocked(catalog Catalog) error {
	catalog.Schema = CatalogSchema
	catalog.UpdatedAt = time.Now().UTC()
	if catalog.CustomNodes == nil {
		catalog.CustomNodes = []CustomNode{}
	}
	for index := range catalog.Profiles {
		normalizeProfile(&catalog.Profiles[index])
	}
	if err := validateCatalog(catalog); err != nil {
		return err
	}
	data, err := json.MarshalIndent(catalog, "", "  ")
	if err != nil {
		return err
	}
	return state.WriteAtomic(store.paths.SubscriptionStore, append(data, '\n'), 0o600)
}

func normalizeProfile(profile *Profile) {
	migrateLegacyDNSFields(profile)
	if profile.Revision == 0 {
		profile.Revision = 1
	}
	if profile.LogLevel == "" {
		profile.LogLevel = "info"
	}
	if profile.Sources == nil {
		profile.Sources = []Source{}
	}
	if profile.CustomNodeIDs == nil {
		profile.CustomNodeIDs = []string{}
	}
	if profile.Groups == nil {
		profile.Groups = []ProxyGroup{}
	}
	if profile.Rules == nil {
		profile.Rules = []string{}
	}
	if profile.RuleProviders == nil {
		profile.RuleProviders = []RuleProvider{}
	}
	if profile.Filters == nil {
		profile.Filters = []string{}
	}
	if profile.LastCompilerWarnings == nil {
		profile.LastCompilerWarnings = []string{}
	}
	if profile.TransparentProxy.Mode == "" {
		profile.TransparentProxy = defaultTransparentProxyConfig()
	}
	if profile.TransparentProxy.TUN.InterfaceName == "" {
		profile.TransparentProxy.TUN.InterfaceName = "sempre-tun"
	}
	if profile.TransparentProxy.TUN.RouteExcludeAddress == nil {
		profile.TransparentProxy.TUN.RouteExcludeAddress = []string{}
	}
	if profile.TransparentProxy.TUN.InterfaceMode == "" {
		profile.TransparentProxy.TUN.InterfaceMode = "all"
	}
	if profile.TransparentProxy.TUN.Interfaces == nil {
		profile.TransparentProxy.TUN.Interfaces = []string{}
	}
	if profile.TransparentProxy.TProxy.ListenPort == 0 {
		profile.TransparentProxy.TProxy.ListenPort = 7893
	}
	if profile.TransparentProxy.TProxy.DNSListenPort == 0 {
		profile.TransparentProxy.TProxy.DNSListenPort = 1053
	}
	if profile.TransparentProxy.TProxy.LANInterfaces == nil {
		profile.TransparentProxy.TProxy.LANInterfaces = []string{}
	}
	if profile.CoreOverrides == nil {
		profile.CoreOverrides = map[string]map[string]any{}
	}
	if profile.ManagementAPI.AllowOrigins == nil {
		profile.ManagementAPI.AllowOrigins = []string{}
	}
	for index := range profile.Sources {
		source := &profile.Sources[index]
		if source.UserAgent == "" {
			source.UserAgent = DefaultUserAgent
		}
		if source.FetchMode == "" {
			source.FetchMode = FetchAuto
		}
	}
	if !editorConfigPresent(profile.Editor) {
		profile.Editor = editorConfigFromProfile(*profile)
	}
	if strings.TrimSpace(profile.Editor.Servers) == "" {
		profile.Editor.Servers = "[]"
	}
}

func validateCatalog(catalog Catalog) error {
	if catalog.Schema != CatalogSchema {
		return fmt.Errorf("unsupported catalog schema %d", catalog.Schema)
	}
	if len(catalog.Profiles) == 0 {
		return fmt.Errorf("at least one subscription profile is required")
	}
	profileIDs := map[string]bool{}
	names := map[string]bool{}
	customIDs := map[string]bool{}
	for _, node := range catalog.CustomNodes {
		if node.ID == "" || node.Name == "" || len(node.Proxy) == 0 {
			return fmt.Errorf("custom nodes require an ID, name, and proxy")
		}
		if customIDs[node.ID] {
			return fmt.Errorf("duplicate custom node ID %q", node.ID)
		}
		customIDs[node.ID] = true
	}
	for index, profile := range catalog.Profiles {
		if profile.ID == "" {
			return fmt.Errorf("profile ID is required")
		}
		if profileIDs[profile.ID] {
			return fmt.Errorf("duplicate profile ID %q", profile.ID)
		}
		profileIDs[profile.ID] = true
		if profile.Revision == 0 {
			return fmt.Errorf("profile %q has no revision", profile.Name)
		}
		name := strings.ToLower(strings.TrimSpace(profile.Name))
		if index > 0 && name == "" {
			return fmt.Errorf("profile name is required")
		}
		if names[name] {
			return fmt.Errorf("profile name %q is already used", profile.Name)
		}
		names[name] = true
		sourceIDs := map[string]bool{}
		for _, source := range profile.Sources {
			if err := ValidateSource(source); err != nil {
				return fmt.Errorf("profile %q: %w", profile.Name, err)
			}
			if sourceIDs[source.ID] {
				return fmt.Errorf("duplicate source ID %q", source.ID)
			}
			sourceIDs[source.ID] = true
		}
		for _, id := range profile.CustomNodeIDs {
			if !customIDs[id] {
				return fmt.Errorf("profile %q references missing custom node %q", profile.Name, id)
			}
		}
		groupNames := map[string]bool{}
		for _, group := range profile.Groups {
			name := strings.TrimSpace(group.Name)
			if name == "" {
				return fmt.Errorf("profile %q has a proxy group without a name", profile.Name)
			}
			if groupNames[name] {
				return fmt.Errorf("profile %q has duplicate proxy group %q", profile.Name, name)
			}
			groupNames[name] = true
			switch group.Type {
			case "select", "url-test":
			default:
				return fmt.Errorf("profile %q group %q has unsupported type %q", profile.Name, name, group.Type)
			}
			if group.Default != "" && group.Readonly && !group.IncludeAll && len(group.Proxies) > 0 && !configuredMember(group.Proxies, group.Default) {
				return fmt.Errorf("profile %q group %q default %q is not a configured member", profile.Name, name, group.Default)
			}
		}
		if err := validateTransparentProxy(profile.TransparentProxy); err != nil {
			return fmt.Errorf("profile %q: %w", profile.Name, err)
		}
		if err := validateManagementAPI(profile.ManagementAPI); err != nil {
			return fmt.Errorf("profile %q: %w", profile.Name, err)
		}
		providerTags := map[string]bool{}
		for _, provider := range profile.RuleProviders {
			if strings.TrimSpace(provider.Tag) == "" {
				return fmt.Errorf("profile %q has a rule provider without a tag", profile.Name)
			}
			if providerTags[provider.Tag] {
				return fmt.Errorf("profile %q has duplicate rule provider tag %q", profile.Name, provider.Tag)
			}
			providerTags[provider.Tag] = true
			if err := ValidateSource(Source{ID: provider.Tag, Type: SourceURL, URL: provider.URL, FetchMode: FetchAuto}); err != nil {
				return fmt.Errorf("profile %q rule provider %q: %w", profile.Name, provider.Tag, err)
			}
		}
	}
	return nil
}

func migrateLinuxRuntimeConfig(profile *Profile) {
	profile.TransparentProxy = defaultTransparentProxyConfig()
	shared := profile.DNS
	if nested, ok := shared["shared"].(map[string]any); ok {
		shared = nested
	}
	if value, ok := numberValue(shared["tproxyPort"]); ok && value > 0 {
		profile.TransparentProxy.TProxy.ListenPort = value
	}
	if value, ok := numberValue(shared["dnsListenPort"]); ok && value > 0 {
		profile.TransparentProxy.TProxy.DNSListenPort = value
	}
}

func migrateCoreConfiguration(profile *Profile, legacy legacyProfileConfiguration) {
	if profile.TransparentProxy.TUN.InterfaceName == "sing-box" {
		profile.TransparentProxy.TUN.InterfaceName = "sempre-tun"
	}
	if profile.CoreOverrides == nil {
		profile.CoreOverrides = map[string]map[string]any{}
	}
	if len(legacy.CustomConfig) > 0 && len(profile.CoreOverrides["sing-box"]) == 0 {
		profile.CoreOverrides["sing-box"] = cloneMap(legacy.CustomConfig)
	}
	if legacy.ClashAPI != nil && !profile.ManagementAPI.Enabled && profile.ManagementAPI.ExternalController == "" && profile.ManagementAPI.Secret == "" && profile.ManagementAPI.ExternalUI == "" {
		profile.ManagementAPI = *legacy.ClashAPI
	}
	migrateLegacyDNSFields(profile)
}

func migrateLegacyDNSFields(profile *Profile) {
	if profile.DNS == nil {
		return
	}
	shared := profile.DNS
	if nested, ok := objectValue(profile.DNS["shared"]); ok {
		shared = nested
	}
	if value, ok := numberValue(shared["tproxyPort"]); ok && value > 0 && profile.TransparentProxy.TProxy.ListenPort == 7893 {
		profile.TransparentProxy.TProxy.ListenPort = value
	}
	if value, ok := numberValue(shared["dnsListenPort"]); ok && value > 0 && profile.TransparentProxy.TProxy.DNSListenPort == 1053 {
		profile.TransparentProxy.TProxy.DNSListenPort = value
	}
	if secret := stringValue(shared["clashApiSecret"]); secret != "" && profile.ManagementAPI.Secret == "" {
		profile.ManagementAPI.Secret = secret
	}
	if ui := stringValue(shared["clashApiUiPath"]); ui != "" && profile.ManagementAPI.ExternalUI == "" {
		profile.ManagementAPI.ExternalUI = ui
	}
	if port, ok := numberValue(shared["clashApiPort"]); ok && port > 0 && profile.ManagementAPI.Enabled && profile.ManagementAPI.ExternalController == "" {
		profile.ManagementAPI.ExternalController = net.JoinHostPort("127.0.0.1", fmt.Sprint(port))
	}
	for _, key := range []string{"tproxyPort", "dnsListenPort", "clashApiPort", "clashApiSecret", "clashApiUiPath"} {
		delete(shared, key)
	}
	if overrides, ok := objectValue(profile.DNS["overrides"]); ok {
		migrated := cloneMap(overrides)
		modes, _ := objectValue(profile.DNS["modes"])
		modes = cloneMap(modes)
		if modes == nil {
			modes = map[string]any{}
		}
		if value, exists := overrides["sing_box_v11"]; exists {
			migrated["sing_box_v11"] = value
		} else if value, exists := overrides["singbox"]; exists {
			migrated["sing_box_v11"] = value
		}
		delete(migrated, "singbox")
		if value, exists := overrides["sing_box_v12"]; exists {
			migrated["sing_box_v12"] = value
		} else if value, exists := overrides["singboxV12"]; exists {
			migrated["sing_box_v12"] = value
		}
		delete(migrated, "singboxV12")
		if value, exists := overrides["mihomo"]; exists {
			migrated["mihomo"] = value
		} else if value, exists := overrides["clashMeta"]; exists {
			migrated["mihomo"] = value
		} else if value, exists := overrides["clash"]; exists {
			migrated["mihomo"] = value
		}
		delete(migrated, "clashMeta")
		delete(migrated, "clash")
		for _, key := range []string{"sing_box_v11", "sing_box_v12", "mihomo"} {
			if _, exists := migrated[key]; exists {
				if _, configured := modes[key]; !configured {
					modes[key] = "native"
				}
			}
		}
		if len(migrated) > 0 {
			profile.DNS["overrides"] = migrated
			profile.DNS["modes"] = modes
		} else {
			delete(profile.DNS, "overrides")
		}
	}
	if strings.TrimSpace(profile.Editor.DNSConfig) != "" {
		profile.Editor.DNSConfig = marshalEditorJSON(profile.DNS, "")
	}
}

func validateTransparentProxy(config TransparentProxyConfig) error {
	switch config.Mode {
	case TransparentProxyTUN, TransparentProxyTProxy, TransparentProxyDisabled:
	default:
		return fmt.Errorf("unsupported transparent proxy mode %q", config.Mode)
	}
	if strings.TrimSpace(config.TUN.InterfaceName) == "" || len(config.TUN.InterfaceName) > 15 {
		return fmt.Errorf("TUN interface name must contain 1 to 15 characters")
	}
	if config.TUN.Address != "" {
		prefix, err := netip.ParsePrefix(config.TUN.Address)
		if err != nil || !prefix.Addr().Is4() || prefix.Bits() != 30 {
			return fmt.Errorf("TUN address must be an IPv4 /30 prefix")
		}
	}
	for _, value := range config.TUN.RouteExcludeAddress {
		if _, err := netip.ParsePrefix(value); err != nil {
			return fmt.Errorf("invalid TUN route exclusion %q", value)
		}
	}
	switch config.TUN.InterfaceMode {
	case "all", "include", "exclude":
	default:
		return fmt.Errorf("TUN interface mode must be all, include, or exclude")
	}
	interfaceNames := map[string]bool{}
	for _, name := range config.TUN.Interfaces {
		name = strings.TrimSpace(name)
		if name == "" || len(name) > 15 || interfaceNames[name] {
			return fmt.Errorf("TUN interfaces must be unique valid interface names")
		}
		interfaceNames[name] = true
	}
	if config.TUN.InterfaceMode != "all" && len(config.TUN.Interfaces) == 0 {
		return fmt.Errorf("TUN interface mode %s requires at least one interface", config.TUN.InterfaceMode)
	}
	if config.TProxy.ListenPort < 1 || config.TProxy.ListenPort > 65535 || config.TProxy.DNSListenPort < 1 || config.TProxy.DNSListenPort > 65535 {
		return fmt.Errorf("transparent proxy ports must be between 1 and 65535")
	}
	seen := map[string]bool{}
	for _, name := range config.TProxy.LANInterfaces {
		name = strings.TrimSpace(name)
		if name == "" || len(name) > 15 || seen[name] {
			return fmt.Errorf("TProxy LAN interfaces must be unique valid interface names")
		}
		seen[name] = true
	}
	return nil
}

func validateManagementAPI(config ManagementAPIConfig) error {
	if !config.Enabled {
		return nil
	}
	if strings.TrimSpace(config.ExternalController) == "" {
		return fmt.Errorf("external management API controller is required when enabled")
	}
	host, port, err := net.SplitHostPort(config.ExternalController)
	if err != nil || strings.TrimSpace(host) == "" || strings.TrimSpace(port) == "" {
		return fmt.Errorf("external management API controller must use host:port syntax")
	}
	if strings.TrimSpace(config.Secret) == "" {
		return fmt.Errorf("external management API secret is required when enabled")
	}
	return nil
}

func configuredMember(values []string, expected string) bool {
	for _, value := range values {
		if value == expected {
			return true
		}
	}
	return false
}

func numberValue(value any) (int, bool) {
	switch typed := value.(type) {
	case float64:
		return int(typed), typed == float64(int(typed))
	case int:
		return typed, true
	default:
		return 0, false
	}
}

func ValidateCatalog(catalog Catalog) error {
	return validateCatalog(catalog)
}

func FindProfile(catalog *Catalog, id string) (*Profile, error) {
	for index := range catalog.Profiles {
		if catalog.Profiles[index].ID == id {
			return &catalog.Profiles[index], nil
		}
	}
	return nil, fmt.Errorf("subscription profile %q was not found", id)
}

func SortProfiles(profiles []Profile) {
	sort.SliceStable(profiles, func(i, j int) bool { return profiles[i].Name < profiles[j].Name })
}
