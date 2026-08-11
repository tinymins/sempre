package subscription

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
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
	CustomConfig     map[string]any               `json:"custom_config"`
	ClashAPI         *ManagementAPIConfig         `json:"clash_api"`
	TransparentProxy legacyTransparentProxyConfig `json:"transparent_proxy"`
}

type legacyTransparentProxyConfig struct {
	Mode string `json:"mode"`
	TUN  struct {
		InterfaceName          string   `json:"interface_name"`
		Address                string   `json:"address,omitempty"`
		RouteExcludeAddress    []string `json:"route_exclude_address"`
		InterfaceMode          string   `json:"interface_mode"`
		Interfaces             []string `json:"interfaces"`
		AutoExcludeLocalRoutes bool     `json:"auto_exclude_local_routes"`
		AutoExcludeVPNRoutes   bool     `json:"auto_exclude_vpn_routes"`
	} `json:"tun"`
	TProxy struct {
		ListenPort    int      `json:"listen_port"`
		DNSListenPort int      `json:"dns_listen_port"`
		CaptureHost   bool     `json:"capture_host"`
		LANInterfaces []string `json:"lan_interfaces"`
	} `json:"tproxy"`
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
		if previousSchema < 6 {
			if err := json.Unmarshal(data, &legacy); err != nil {
				return Catalog{}, fmt.Errorf("decode legacy subscription configuration: %w", err)
			}
		}
		catalog.Schema = CatalogSchema
		for index := range catalog.Profiles {
			configuration := legacyProfileConfiguration{}
			if index < len(legacy.Profiles) {
				configuration = legacy.Profiles[index]
			}
			if previousSchema < 4 {
				migrateLinuxRuntimeConfig(&catalog.Profiles[index])
			}
			if previousSchema < 6 {
				if previousSchema >= 4 {
					migrateRuntimeIntent(&catalog.Profiles[index], configuration.TransparentProxy)
				} else {
					catalog.Profiles[index].LocalProxy = defaultLocalProxyConfig()
				}
			}
			if previousSchema < 5 {
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
