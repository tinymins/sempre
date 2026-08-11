package subscription

import (
	"fmt"
	"sort"
	"strconv"
	"strings"

	"gopkg.in/yaml.v3"
)

func convertRule(line string) (map[string]any, bool) {
	parts := strings.Split(line, ",")
	if len(parts) < 3 {
		return nil, false
	}
	rule := map[string]any{"outbound": normalizeOutboundName(strings.TrimSpace(parts[2]))}
	value := strings.TrimSpace(parts[1])
	switch strings.TrimSpace(parts[0]) {
	case "DOMAIN":
		rule["domain"] = value
	case "DOMAIN-SUFFIX":
		rule["domain_suffix"] = value
	case "DOMAIN-KEYWORD":
		rule["domain_keyword"] = value
	case "DOMAIN-REGEX":
		rule["domain_regex"] = value
	case "IP-CIDR":
		rule["ip_cidr"] = value
	case "SRC-IP-CIDR":
		rule["source_ip_cidr"] = value
	default:
		return nil, false
	}
	return rule, true
}

func parseRuleProvider(data []byte) ([]any, []string, error) {
	var document struct {
		Payload []string `yaml:"payload"`
	}
	lines := []string{}
	if err := yaml.Unmarshal(data, &document); err == nil {
		lines = document.Payload
	}
	if len(lines) == 0 {
		var sequence []string
		if err := yaml.Unmarshal(data, &sequence); err == nil {
			lines = sequence
		}
	}
	if len(lines) == 0 {
		for _, line := range strings.Split(string(data), "\n") {
			line = strings.TrimSpace(line)
			if line == "" || strings.HasPrefix(line, "#") || line == "payload:" {
				continue
			}
			lines = append(lines, strings.TrimSpace(strings.TrimPrefix(line, "- ")))
		}
	}
	grouped := map[string]any{}
	diagnostics := []string{}
	appendString := func(key, value string) {
		values, _ := grouped[key].([]string)
		grouped[key] = append(values, value)
	}
	appendPort := func(key, value string) bool {
		port, err := strconv.Atoi(value)
		if err != nil || port < 0 || port > 65535 {
			return false
		}
		values, _ := grouped[key].([]int)
		grouped[key] = append(values, port)
		return true
	}
	for _, raw := range lines {
		line := strings.TrimSpace(raw)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		parts := strings.SplitN(line, ",", 3)
		if len(parts) == 1 {
			appendString("domain", strings.TrimSpace(parts[0]))
			continue
		}
		kind, value := strings.TrimSpace(parts[0]), strings.TrimSpace(parts[1])
		switch kind {
		case "DOMAIN", "+", "HOST":
			appendString("domain", value)
		case "DOMAIN-SUFFIX", "HOST-SUFFIX":
			appendString("domain_suffix", value)
		case "DOMAIN-KEYWORD", "HOST-KEYWORD":
			appendString("domain_keyword", value)
		case "DOMAIN-REGEX":
			appendString("domain_regex", value)
		case "IP-CIDR", "IP-CIDR6":
			appendString("ip_cidr", value)
		case "SRC-IP-CIDR":
			appendString("source_ip_cidr", value)
		case "DST-PORT":
			if !appendPort("port", value) {
				diagnostics = append(diagnostics, "invalid provider port rule: "+line)
			}
		case "SRC-PORT":
			if !appendPort("source_port", value) {
				diagnostics = append(diagnostics, "invalid provider port rule: "+line)
			}
		case "PROCESS-NAME", "PROCESS-PATH":
			// These rules can crash sing-box mobile clients and are intentionally omitted.
		default:
			diagnostics = append(diagnostics, "unsupported provider rule: "+line)
		}
	}
	if len(grouped) == 0 {
		return nil, diagnostics, fmt.Errorf("provider has no convertible rules")
	}
	return []any{grouped}, diagnostics, nil
}
func validateSubscriptionContent(data []byte) error {
	parsed := Parse(string(data))
	if len(parsed.Nodes) == 0 {
		return fmt.Errorf("no usable proxy nodes: %s", strings.Join(parsed.Diagnostics, "; "))
	}
	return nil
}
func validateRuleProviderContent(data []byte) error {
	_, _, err := parseRuleProvider(data)
	return err
}
func officialRuleSet(tag, url string) map[string]any {
	return map[string]any{"tag": tag, "type": "remote", "format": "binary", "url": url, "download_detour": "direct"}
}
func normalizePlatform(value string) string {
	switch value {
	case "windows":
		return "windows"
	case "darwin", "macos":
		return "macos"
	default:
		return "default"
	}
}
func normalizePrefix(value string) string {
	if value == "" {
		return ""
	}
	for _, suffix := range []string{"-", " ", "丨", "|", "｜", "/", "_", "·", ")", "）", "]", "】", "}", "》", ">", "」"} {
		if strings.HasSuffix(value, suffix) {
			return value
		}
	}
	return value + "丨"
}
func sourceLabel(source Source) string {
	if source.Remark != "" {
		return source.Remark
	}
	if source.URL != "" {
		return source.URL
	}
	return source.ID
}
func redactSource(source Source) Source {
	if source.Type == SourceRaw {
		source.Content = ""
	}
	return source
}
func uniqueNodeNames(nodes []Proxy, nodeOrigins []string) ([]Proxy, map[string]string) {
	counts := map[string]int{}
	origins := map[string]string{}
	result := make([]Proxy, 0, len(nodes))
	for index, node := range nodes {
		base := node.Name
		counts[base]++
		if counts[base] > 1 {
			node.Name = fmt.Sprintf("%s (%d)", base, counts[base])
		}
		result = append(result, node)
		origins[node.Name] = nodeOrigins[index]
	}
	return result, origins
}
func appendUnique(target []string, values ...string) []string {
	seen := map[string]bool{}
	for _, value := range target {
		seen[value] = true
	}
	for _, value := range values {
		if !seen[value] {
			target = append(target, value)
			seen[value] = true
		}
	}
	return target
}
func stringListValue(value any) []string {
	switch values := value.(type) {
	case []string:
		return values
	case []any:
		result := []string{}
		for _, item := range values {
			text, ok := item.(string)
			if ok {
				result = append(result, text)
			}
		}
		return result
	default:
		return nil
	}
}
func normalizeOutboundName(value string) string {
	switch value {
	case "DIRECT", "🚀 直接连接":
		return "direct"
	case "REJECT":
		return "reject"
	default:
		return value
	}
}
func builtinOutboundName(value string) bool {
	normalized := normalizeOutboundName(value)
	return normalized == "direct" || normalized == "reject" || normalized == "dns-out"
}
func foreignOutbound(groups []ProxyGroup, fallback string) string {
	for _, group := range groups {
		if group.Name == "🔰 国外流量" {
			return group.Name
		}
	}
	return fallback
}
func defaultSelectorMember(group ProxyGroup, members []string) string {
	if configured := normalizeOutboundName(group.Default); configured != "" {
		return configured
	}
	if group.Name == "🔰 国外流量" {
		for _, member := range members {
			if !builtinOutboundName(member) {
				return member
			}
		}
	}
	return members[0]
}
func normalizeOutboundNames(values []string) []string {
	result := make([]string, 0, len(values))
	for _, value := range values {
		result = append(result, normalizeOutboundName(value))
	}
	return result
}
func deepMerge(target, source map[string]any) {
	for key, value := range source {
		if child, ok := objectValue(value); ok {
			if current, ok := objectValue(target[key]); ok {
				deepMerge(current, child)
				continue
			}
		}
		target[key] = value
	}
}
func sortedKeys(value map[string]any) []string {
	result := make([]string, 0, len(value))
	for key := range value {
		result = append(result, key)
	}
	sort.Strings(result)
	return result
}
