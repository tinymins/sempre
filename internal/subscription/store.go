package subscription

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
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
		catalog.Schema = CatalogSchema
		for index := range catalog.Profiles {
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
