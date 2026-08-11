package subscription

import (
	"fmt"
	"strings"
)

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
	if profile.TransparentProxy.RouteExclusions == nil {
		profile.TransparentProxy.RouteExclusions = []string{}
	}
	if profile.TransparentProxy.InterfaceMode == "" {
		profile.TransparentProxy.InterfaceMode = "all"
	}
	if profile.TransparentProxy.Interfaces == nil {
		profile.TransparentProxy.Interfaces = []string{}
	}
	if profile.TransparentProxy.TProxy.ListenPort == 0 {
		profile.TransparentProxy.TProxy.ListenPort = 7893
	}
	if profile.TransparentProxy.TProxy.DNSListenPort == 0 {
		profile.TransparentProxy.TProxy.DNSListenPort = 1053
	}
	if profile.TransparentProxy.LANInterfaces == nil {
		profile.TransparentProxy.LANInterfaces = []string{}
	}
	if profile.TransparentProxy.EBPF.WANInterface == "" {
		profile.TransparentProxy.EBPF.WANInterface = "auto"
	}
	if profile.LocalProxy == (LocalProxyConfig{}) {
		profile.LocalProxy = defaultLocalProxyConfig()
	} else {
		if profile.LocalProxy.SOCKSPort == 0 {
			profile.LocalProxy.SOCKSPort = 1080
		}
		if profile.LocalProxy.HTTPPort == 0 {
			profile.LocalProxy.HTTPPort = 1081
		}
		if profile.LocalProxy.Username == "" {
			profile.LocalProxy.Username = "sempre"
		}
		if profile.LocalProxy.Password == "" {
			profile.LocalProxy.Password = NewPassword()
		}
	}
	if profile.CoreOverrides == nil {
		profile.CoreOverrides = map[string]map[string]any{}
	}
	if profile.ManagementAPI.AllowOrigins == nil {
		profile.ManagementAPI.AllowOrigins = []string{}
	}
	if profile.ManagementAPI.ExternalController == "" {
		profile.ManagementAPI.ExternalController = "0.0.0.0:9090"
	}
	if profile.ManagementAPI.Secret == "" {
		profile.ManagementAPI.Secret = NewPassword()
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
		if err := validateLocalProxy(profile.LocalProxy, profile.TransparentProxy); err != nil {
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
