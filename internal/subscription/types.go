package subscription

import (
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"net/url"
	"strings"
	"time"
)

const (
	CatalogSchema       = 3
	DefaultUserAgent    = "clash.meta"
	MaxSourceSize       = int64(32 << 20)
	SourceURL           = "url"
	SourceRaw           = "raw"
	FetchAuto           = "auto"
	FetchDomesticDirect = "domestic-direct"
)

type Catalog struct {
	Schema      int          `json:"schema"`
	UpdatedAt   time.Time    `json:"updated_at"`
	Profiles    []Profile    `json:"profiles"`
	CustomNodes []CustomNode `json:"custom_nodes"`
}

type Profile struct {
	ID                    string         `json:"id"`
	Revision              uint64         `json:"revision"`
	Name                  string         `json:"name"`
	Remark                string         `json:"remark,omitempty"`
	LogLevel              string         `json:"log_level"`
	Editor                EditorConfig   `json:"editor"`
	Sources               []Source       `json:"sources"`
	CustomNodeIDs         []string       `json:"custom_node_ids"`
	Groups                []ProxyGroup   `json:"groups"`
	Rules                 []string       `json:"rules"`
	RuleProviders         []RuleProvider `json:"rule_providers"`
	Filters               []string       `json:"filters"`
	DNS                   map[string]any `json:"dns,omitempty"`
	PrivateAccess         map[string]any `json:"private_access,omitempty"`
	CustomConfig          map[string]any `json:"custom_config,omitempty"`
	UseSystemGroups       bool           `json:"use_system_groups"`
	UseSystemRules        bool           `json:"use_system_rules"`
	UseSystemFilters      bool           `json:"use_system_filters"`
	UseSystemDNS          bool           `json:"use_system_dns"`
	UseSystemCustomConfig bool           `json:"use_system_custom_config"`
	LastCheck             time.Time      `json:"last_check,omitempty"`
	LastChange            time.Time      `json:"last_change,omitempty"`
	LastResult            string         `json:"last_result,omitempty"`
	LastConfigHash        string         `json:"last_config_hash,omitempty"`
	LastRuntimeValidated  bool           `json:"last_runtime_validated"`
	LastCompilerTarget    string         `json:"last_compiler_target,omitempty"`
	LastCompilerWarnings  []string       `json:"last_compiler_warnings,omitempty"`
}

type EditorConfig struct {
	RuleList            string `json:"rule_list"`
	Group               string `json:"group"`
	Filter              string `json:"filter"`
	CustomConfig        string `json:"custom_config"`
	DNSConfig           string `json:"dns_config"`
	PrivateAccessConfig string `json:"private_access_config"`
	Servers             string `json:"servers"`
}

type Source struct {
	ID              string    `json:"id"`
	Type            string    `json:"type"`
	Enabled         bool      `json:"enabled"`
	URL             string    `json:"url,omitempty"`
	Content         string    `json:"content,omitempty"`
	Prefix          string    `json:"prefix,omitempty"`
	Remark          string    `json:"remark,omitempty"`
	UserAgent       string    `json:"user_agent,omitempty"`
	FetchMode       string    `json:"fetch_mode,omitempty"`
	CacheTTLMinutes int       `json:"cache_ttl_minutes,omitempty"`
	SnapshotHash    string    `json:"snapshot_hash,omitempty"`
	FetchedAt       time.Time `json:"fetched_at,omitempty"`
	LastStatus      string    `json:"last_status,omitempty"`
	LastError       string    `json:"last_error,omitempty"`
}

type CustomNode struct {
	ID        string         `json:"id"`
	Name      string         `json:"name"`
	Proxy     map[string]any `json:"proxy"`
	CreatedAt time.Time      `json:"created_at"`
	UpdatedAt time.Time      `json:"updated_at"`
}

type ProxyGroup struct {
	Name       string   `json:"name"`
	Type       string   `json:"type"`
	Proxies    []string `json:"proxies,omitempty"`
	IncludeAll bool     `json:"include_all,omitempty"`
	Readonly   bool     `json:"readonly,omitempty"`
	URL        string   `json:"url,omitempty"`
	Interval   int      `json:"interval,omitempty"`
	Tolerance  int      `json:"tolerance,omitempty"`
}

type RuleProvider struct {
	Tag      string `json:"tag"`
	URL      string `json:"url"`
	Outbound string `json:"outbound,omitempty"`
	Format   string `json:"format,omitempty"`
	Behavior string `json:"behavior,omitempty"`
}

type Proxy struct {
	Name   string         `json:"name" yaml:"name"`
	Type   string         `json:"type" yaml:"type"`
	Server string         `json:"server" yaml:"server"`
	Port   int            `json:"port" yaml:"port"`
	Extra  map[string]any `json:"extra,omitempty" yaml:"-"`
}

func (proxy Proxy) Map() map[string]any {
	result := make(map[string]any, len(proxy.Extra)+4)
	for key, value := range proxy.Extra {
		result[key] = value
	}
	result["name"] = proxy.Name
	result["type"] = proxy.Type
	result["server"] = proxy.Server
	result["port"] = proxy.Port
	return result
}

type ParseResult struct {
	Format                string   `json:"format"`
	DecodedText           string   `json:"decoded_text,omitempty"`
	Nodes                 []Proxy  `json:"nodes"`
	DiscardedPlaceholders []Proxy  `json:"discarded_placeholder_nodes"`
	Diagnostics           []string `json:"diagnostics"`
}

type SourceResult struct {
	Source      Source      `json:"source"`
	Parse       ParseResult `json:"parse"`
	FromCache   bool        `json:"from_cache"`
	ContentHash string      `json:"content_hash"`
	Bytes       int         `json:"bytes"`
}

type FieldDiff struct {
	Node         string                 `json:"node"`
	Consumed     []string               `json:"consumed"`
	Ignored      []string               `json:"ignored"`
	Dropped      []string               `json:"dropped"`
	Warnings     []string               `json:"warnings"`
	Outbound     map[string]any         `json:"outbound,omitempty"`
	FieldOrigins map[string]FieldOrigin `json:"field_origins,omitempty"`
}

type FieldOrigin struct {
	SourceKey   string   `json:"source_key,omitempty"`
	SourceValue any      `json:"source_value,omitempty"`
	Step        string   `json:"step"`
	Transform   string   `json:"transform"`
	Reason      string   `json:"reason,omitempty"`
	Sources     []string `json:"sources,omitempty"`
}

type RenderResult struct {
	Format           string            `json:"format"`
	Version          string            `json:"version,omitempty"`
	Platform         string            `json:"platform,omitempty"`
	Content          string            `json:"content"`
	NodeCount        int               `json:"node_count"`
	SourceResults    []SourceResult    `json:"source_results,omitempty"`
	FieldDiffs       []FieldDiff       `json:"field_diffs,omitempty"`
	NodeOrigins      map[string]string `json:"node_origins,omitempty"`
	Warnings         []string          `json:"warnings,omitempty"`
	RuntimeValidated bool              `json:"runtime_validated"`
}

func NewCatalog(legacyURL string) Catalog {
	profile := NewProfile("")
	if strings.TrimSpace(legacyURL) != "" {
		profile.Sources = append(profile.Sources, Source{
			ID: NewID(), Type: SourceURL, Enabled: true, URL: strings.TrimSpace(legacyURL),
			UserAgent: DefaultUserAgent, FetchMode: FetchAuto,
		})
	}
	return Catalog{Schema: CatalogSchema, Profiles: []Profile{profile}, CustomNodes: []CustomNode{}}
}

func NewProfile(name string) Profile {
	return Profile{
		ID: NewID(), Revision: 1, Name: strings.TrimSpace(name), LogLevel: "info", Editor: EditorConfig{Servers: "[]"}, Sources: []Source{}, CustomNodeIDs: []string{},
		Groups: []ProxyGroup{}, Rules: []string{}, RuleProviders: []RuleProvider{}, Filters: []string{},
		UseSystemGroups: true, UseSystemRules: true, UseSystemFilters: true,
		UseSystemDNS: true, UseSystemCustomConfig: true,
	}
}

func NewID() string {
	data := make([]byte, 16)
	if _, err := rand.Read(data); err != nil {
		panic(fmt.Sprintf("generate identifier: %v", err))
	}
	data[6] = (data[6] & 0x0f) | 0x40
	data[8] = (data[8] & 0x3f) | 0x80
	encoded := hex.EncodeToString(data)
	return encoded[0:8] + "-" + encoded[8:12] + "-" + encoded[12:16] + "-" + encoded[16:20] + "-" + encoded[20:]
}

func ValidateSource(source Source) error {
	if strings.TrimSpace(source.ID) == "" {
		return fmt.Errorf("source ID is required")
	}
	switch source.Type {
	case SourceURL:
		parsed, err := url.Parse(strings.TrimSpace(source.URL))
		if err != nil || parsed.Hostname() == "" || parsed.User != nil || (parsed.Scheme != "http" && parsed.Scheme != "https") {
			return fmt.Errorf("source URL must be an absolute HTTP or HTTPS URL without user information")
		}
	case SourceRaw:
		if strings.TrimSpace(source.Content) == "" {
			return fmt.Errorf("raw source content is empty")
		}
	default:
		return fmt.Errorf("unsupported source type %q", source.Type)
	}
	if source.FetchMode != "" && source.FetchMode != FetchAuto && source.FetchMode != FetchDomesticDirect {
		return fmt.Errorf("unsupported fetch mode %q", source.FetchMode)
	}
	if source.CacheTTLMinutes < 0 {
		return fmt.Errorf("cache TTL cannot be negative")
	}
	return nil
}
