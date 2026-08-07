package gateway

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

func resolveDNSRuleSets(ctx context.Context, config DNSConfig) (DNSConfig, error) {
	client := &http.Client{Timeout: 15 * time.Second}
	for index := range config.RuleSets {
		set := &config.RuleSets[index]
		if !set.Enabled || set.Type != "url" || strings.TrimSpace(set.URL) == "" {
			continue
		}
		parsed, err := url.Parse(set.URL)
		if err != nil || parsed.Hostname() == "" || (parsed.Scheme != "http" && parsed.Scheme != "https") {
			return DNSConfig{}, fmt.Errorf("invalid DNS rule set URL %q", set.URL)
		}
		request, err := http.NewRequestWithContext(ctx, http.MethodGet, set.URL, nil)
		if err != nil {
			return DNSConfig{}, err
		}
		response, err := client.Do(request)
		if err != nil {
			return DNSConfig{}, fmt.Errorf("fetch DNS rule set %q: %w", set.Name, err)
		}
		data, readErr := io.ReadAll(io.LimitReader(response.Body, 4<<20))
		closeErr := response.Body.Close()
		if readErr != nil {
			return DNSConfig{}, fmt.Errorf("read DNS rule set %q: %w", set.Name, readErr)
		}
		if closeErr != nil {
			return DNSConfig{}, closeErr
		}
		if response.StatusCode < 200 || response.StatusCode >= 300 {
			return DNSConfig{}, fmt.Errorf("fetch DNS rule set %q: HTTP %d", set.Name, response.StatusCode)
		}
		set.Rules = parseRuleSetLines(string(data))
	}
	return config, nil
}

func parseRuleSetLines(data string) []string {
	rules := []string{}
	for _, line := range strings.Split(data, "\n") {
		line = strings.TrimSpace(line)
		line = strings.TrimPrefix(line, "- ")
		line = strings.Trim(line, `"'`)
		if line == "" || strings.HasPrefix(line, "#") || strings.HasSuffix(line, ":") {
			continue
		}
		if strings.HasPrefix(strings.ToLower(line), "payload:") {
			continue
		}
		rules = append(rules, line)
	}
	return rules
}
