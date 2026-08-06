package app

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/netip"
	"regexp"
	"strings"
	"time"
)

const networkTestTimeout = 8 * time.Second

var ipv4Pattern = regexp.MustCompile(`\b(?:\d{1,3}\.){3}\d{1,3}\b`)

type NetworkTestReport struct {
	CheckedAt time.Time           `json:"checked_at"`
	Results   []NetworkTestResult `json:"results"`
}

type NetworkTestResult struct {
	ID         string `json:"id"`
	Name       string `json:"name"`
	Region     string `json:"region"`
	Category   string `json:"category"`
	URL        string `json:"url"`
	OK         bool   `json:"ok"`
	LatencyMS  int64  `json:"latency_ms"`
	HTTPStatus int    `json:"http_status,omitempty"`
	IP         string `json:"ip,omitempty"`
	Detail     string `json:"detail,omitempty"`
}

type networkTestProbe struct {
	ID       string
	Name     string
	Region   string
	Category string
	URL      string
	Success  func(int) bool
	ParseIP  func([]byte) (string, error)
}

var defaultNetworkTestProbes = []networkTestProbe{
	{ID: "baidu", Name: "Baidu", Region: "domestic", Category: "reachability", URL: "https://www.baidu.com/", Success: status2xx3xx},
	{ID: "google", Name: "Google", Region: "foreign", Category: "reachability", URL: "https://www.google.com/generate_204", Success: statusIs(204)},
	{ID: "domestic-ip", Name: "Domestic IP", Region: "domestic", Category: "ip", URL: "https://ip.3322.net", Success: status2xx3xx, ParseIP: parseTextIP},
	{ID: "foreign-ip", Name: "Foreign IP", Region: "foreign", Category: "ip", URL: "https://api64.ipify.org?format=json", Success: status2xx3xx, ParseIP: parseJSONIP},
	{ID: "openai", Name: "OpenAI", Region: "foreign", Category: "reachability", URL: "https://api.openai.com/v1/models", Success: statusIs(401)},
	{ID: "youtube", Name: "YouTube", Region: "foreign", Category: "reachability", URL: "https://www.youtube.com/generate_204", Success: statusIs(204)},
	{ID: "github", Name: "GitHub", Region: "foreign", Category: "reachability", URL: "https://api.github.com/rate_limit", Success: statusIs(200)},
}

func (manager *Manager) NetworkTest(ctx context.Context) NetworkTestReport {
	return runNetworkTest(ctx, defaultNetworkTestProbes)
}

func runNetworkTest(ctx context.Context, probes []networkTestProbe) NetworkTestReport {
	results := make([]NetworkTestResult, len(probes))
	output := make(chan struct {
		index  int
		result NetworkTestResult
	}, len(probes))
	client := &http.Client{Transport: &http.Transport{Proxy: nil}}
	for index, probe := range probes {
		go func() {
			output <- struct {
				index  int
				result NetworkTestResult
			}{index: index, result: runNetworkTestProbe(ctx, client, probe)}
		}()
	}
	for range probes {
		item := <-output
		results[item.index] = item.result
	}
	return NetworkTestReport{CheckedAt: time.Now().UTC(), Results: results}
}

func runNetworkTestProbe(ctx context.Context, client *http.Client, probe networkTestProbe) NetworkTestResult {
	result := NetworkTestResult{
		ID: probe.ID, Name: probe.Name, Region: probe.Region,
		Category: probe.Category, URL: probe.URL,
	}
	probeCtx, cancel := context.WithTimeout(ctx, networkTestTimeout)
	defer cancel()
	request, err := http.NewRequestWithContext(probeCtx, http.MethodGet, probe.URL, nil)
	if err != nil {
		result.Detail = err.Error()
		return result
	}
	started := time.Now()
	response, err := client.Do(request)
	if err != nil {
		result.LatencyMS = time.Since(started).Milliseconds()
		result.Detail = err.Error()
		return result
	}
	defer response.Body.Close()
	body, readErr := io.ReadAll(io.LimitReader(response.Body, 1<<20))
	result.LatencyMS = time.Since(started).Milliseconds()
	result.HTTPStatus = response.StatusCode
	if readErr != nil {
		result.Detail = readErr.Error()
		return result
	}
	if !probe.Success(response.StatusCode) {
		result.Detail = fmt.Sprintf("HTTP %d", response.StatusCode)
		return result
	}
	if probe.ParseIP != nil {
		ip, parseErr := probe.ParseIP(body)
		if parseErr != nil {
			result.Detail = parseErr.Error()
			return result
		}
		result.IP = ip
	}
	result.OK = true
	return result
}

func status2xx3xx(status int) bool {
	return status >= 200 && status < 400
}

func statusIs(want int) func(int) bool {
	return func(status int) bool { return status == want }
}

func parseJSONIP(data []byte) (string, error) {
	var payload struct {
		IP string `json:"ip"`
	}
	if err := json.Unmarshal(data, &payload); err != nil {
		return "", err
	}
	return normalizeIP(payload.IP)
}

func parseTextIP(data []byte) (string, error) {
	for _, value := range ipv4Pattern.FindAllString(string(data), -1) {
		if ip, err := normalizeIP(value); err == nil {
			return ip, nil
		}
	}
	return "", fmt.Errorf("response did not contain an IP address")
}

func normalizeIP(value string) (string, error) {
	address, err := netip.ParseAddr(strings.TrimSpace(value))
	if err != nil {
		parsed := net.ParseIP(strings.TrimSpace(value))
		if parsed == nil {
			return "", err
		}
		return parsed.String(), nil
	}
	return address.String(), nil
}
