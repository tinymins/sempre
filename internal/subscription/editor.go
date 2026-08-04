package subscription

import (
	"encoding/json"
	"fmt"
	"strings"

	"gopkg.in/yaml.v3"
)

type editorRuleProvider struct {
	Name string `json:"name"`
	URL  string `json:"url"`
	Type string `json:"type,omitempty"`
}

type editorProxyGroup struct {
	Name      string   `json:"name"`
	Type      string   `json:"type"`
	Proxies   []string `json:"proxies"`
	Readonly  bool     `json:"readonly,omitempty"`
	URL       string   `json:"url,omitempty"`
	Interval  int      `json:"interval,omitempty"`
	Tolerance int      `json:"tolerance,omitempty"`
}

func editorConfigPresent(config EditorConfig) bool {
	return strings.TrimSpace(config.RuleList) != "" || strings.TrimSpace(config.Group) != "" ||
		strings.TrimSpace(config.Filter) != "" || strings.TrimSpace(config.CustomConfig) != "" ||
		strings.TrimSpace(config.DNSConfig) != "" || strings.TrimSpace(config.PrivateAccessConfig) != "" ||
		strings.TrimSpace(config.Servers) != ""
}

func editorConfigFromProfile(profile Profile) EditorConfig {
	providers := map[string][]editorRuleProvider{}
	for _, provider := range profile.RuleProviders {
		group := provider.Outbound
		providers[group] = append(providers[group], editorRuleProvider{Name: provider.Tag, URL: provider.URL, Type: provider.Behavior})
	}
	groups := make([]editorProxyGroup, 0, len(profile.Groups))
	for _, group := range profile.Groups {
		groups = append(groups, editorProxyGroup{Name: group.Name, Type: group.Type, Proxies: group.Proxies, Readonly: group.Readonly, URL: group.URL, Interval: group.Interval, Tolerance: group.Tolerance})
	}
	return EditorConfig{
		RuleList:            marshalEditorJSON(providers, "{}"),
		Group:               marshalEditorJSON(groups, "[]"),
		Filter:              marshalEditorJSON(profile.Filters, "[]"),
		CustomConfig:        marshalEditorJSON(profile.Rules, "[]"),
		DNSConfig:           marshalEditorJSON(profile.DNS, ""),
		PrivateAccessConfig: marshalEditorJSON(profile.PrivateAccess, ""),
		Servers:             "[]",
	}
}

func marshalEditorJSON(value any, empty string) string {
	if value == nil {
		return empty
	}
	encoded, err := json.MarshalIndent(value, "", "  ")
	if err != nil || string(encoded) == "null" {
		return empty
	}
	return string(encoded)
}

func ApplyEditorConfig(profile *Profile) error {
	if !editorConfigPresent(profile.Editor) {
		profile.Editor = editorConfigFromProfile(*profile)
	}
	if profile.LogLevel == "" {
		profile.LogLevel = "info"
	}
	switch profile.LogLevel {
	case "off", "error", "warn", "info", "debug":
	default:
		return fmt.Errorf("unsupported log level %q", profile.LogLevel)
	}

	var groups []editorProxyGroup
	if err := parseEditorField("group", profile.Editor.Group, &groups, []editorProxyGroup{}); err != nil {
		return err
	}
	profile.Groups = make([]ProxyGroup, 0, len(groups))
	for _, group := range groups {
		profile.Groups = append(profile.Groups, ProxyGroup{Name: group.Name, Type: group.Type, Proxies: group.Proxies, Readonly: group.Readonly, URL: group.URL, Interval: group.Interval, Tolerance: group.Tolerance})
	}

	providers := map[string][]editorRuleProvider{}
	if err := parseEditorField("rule_list", profile.Editor.RuleList, &providers, map[string][]editorRuleProvider{}); err != nil {
		return err
	}
	profile.RuleProviders = []RuleProvider{}
	for group, items := range providers {
		for _, provider := range items {
			profile.RuleProviders = append(profile.RuleProviders, RuleProvider{Tag: provider.Name, URL: provider.URL, Outbound: group, Behavior: provider.Type})
		}
	}

	if err := parseEditorField("filter", profile.Editor.Filter, &profile.Filters, []string{}); err != nil {
		return err
	}
	if err := parseEditorField("custom_config", profile.Editor.CustomConfig, &profile.Rules, []string{}); err != nil {
		return err
	}
	if err := parseEditorField("dns_config", profile.Editor.DNSConfig, &profile.DNS, map[string]any{}); err != nil {
		return err
	}
	if err := parseEditorField("private_access_config", profile.Editor.PrivateAccessConfig, &profile.PrivateAccess, map[string]any{}); err != nil {
		return err
	}
	var servers []any
	if err := parseEditorField("servers", profile.Editor.Servers, &servers, []any{}); err != nil {
		return err
	}
	return nil
}

func parseEditorField(name, input string, target, fallback any) error {
	if strings.TrimSpace(input) == "" {
		encoded, _ := json.Marshal(fallback)
		if err := json.Unmarshal(encoded, target); err != nil {
			return fmt.Errorf("initialize %s: %w", name, err)
		}
		return nil
	}
	cleaned, err := cleanJSONC(input)
	if err != nil {
		return fmt.Errorf("%s JSONC: %w", name, err)
	}
	if err := json.Unmarshal([]byte(cleaned), target); err != nil {
		return fmt.Errorf("%s JSONC: %w", name, err)
	}
	return nil
}

func cleanJSONC(input string) (string, error) {
	var output strings.Builder
	inString := false
	escaped := false
	for index := 0; index < len(input); {
		current := input[index]
		if inString {
			output.WriteByte(current)
			if escaped {
				escaped = false
			} else if current == '\\' {
				escaped = true
			} else if current == '"' {
				inString = false
			}
			index++
			continue
		}
		if current == '"' {
			inString = true
			output.WriteByte(current)
			index++
			continue
		}
		if current == '/' && index+1 < len(input) && input[index+1] == '/' {
			index += 2
			for index < len(input) && input[index] != '\n' {
				index++
			}
			continue
		}
		if current == '/' && index+1 < len(input) && input[index+1] == '*' {
			index += 2
			closed := false
			for index+1 < len(input) {
				if input[index] == '*' && input[index+1] == '/' {
					index += 2
					closed = true
					break
				}
				if input[index] == '\n' {
					output.WriteByte('\n')
				}
				index++
			}
			if !closed {
				return "", fmt.Errorf("unterminated block comment")
			}
			continue
		}
		output.WriteByte(current)
		index++
	}
	if inString {
		return "", fmt.Errorf("unterminated string")
	}

	withoutComments := output.String()
	output.Reset()
	inString, escaped = false, false
	for index := 0; index < len(withoutComments); index++ {
		current := withoutComments[index]
		if inString {
			output.WriteByte(current)
			if escaped {
				escaped = false
			} else if current == '\\' {
				escaped = true
			} else if current == '"' {
				inString = false
			}
			continue
		}
		if current == '"' {
			inString = true
			output.WriteByte(current)
			continue
		}
		if current == ',' {
			next := index + 1
			for next < len(withoutComments) && strings.ContainsRune(" \t\r\n", rune(withoutComments[next])) {
				next++
			}
			if next < len(withoutComments) && (withoutComments[next] == ']' || withoutComments[next] == '}') {
				continue
			}
		}
		output.WriteByte(current)
	}
	return output.String(), nil
}

func SystemEditorDefaults() EditorConfig {
	defaults := SystemDefaults()
	return editorConfigFromProfile(Profile{Groups: defaults.Groups, RuleProviders: defaults.RuleProviders, Filters: defaults.Filters, Rules: defaults.Rules, DNS: defaults.DNS})
}

func ManualServers(profile Profile) ([]Proxy, error) {
	var values []any
	if err := parseEditorField("servers", profile.Editor.Servers, &values, []any{}); err != nil {
		return nil, err
	}
	result := make([]Proxy, 0, len(values))
	for index, value := range values {
		var object map[string]any
		switch item := value.(type) {
		case map[string]any:
			object = item
		case string:
			if err := yaml.Unmarshal([]byte(item), &object); err != nil {
				return nil, fmt.Errorf("servers item %d: %w", index+1, err)
			}
		default:
			return nil, fmt.Errorf("servers item %d must be an object or YAML object string", index+1)
		}
		proxy, err := ProxyFromMap(object)
		if err != nil {
			return nil, fmt.Errorf("servers item %d: %w", index+1, err)
		}
		result = append(result, proxy)
	}
	return result, nil
}
