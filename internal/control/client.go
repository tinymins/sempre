package control

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/coder/websocket"
)

type CapabilitySet map[string]bool

type Overview struct {
	Core        string         `json:"core"`
	Version     string         `json:"version"`
	Mode        string         `json:"mode,omitempty"`
	Connections int            `json:"connections"`
	Download    int64          `json:"download"`
	Upload      int64          `json:"upload"`
	Extra       map[string]any `json:"extra,omitempty"`
}

type Proxy struct {
	Name    string    `json:"name"`
	Type    string    `json:"type"`
	Now     string    `json:"now,omitempty"`
	All     []string  `json:"all,omitempty"`
	UDP     bool      `json:"udp,omitempty"`
	History []Latency `json:"history,omitempty"`
}

type Latency struct {
	Time  time.Time `json:"time"`
	Delay int       `json:"delay"`
}

type ProxyProvider struct {
	Name        string    `json:"name"`
	Type        string    `json:"type,omitempty"`
	VehicleType string    `json:"vehicle_type,omitempty"`
	UpdatedAt   time.Time `json:"updated_at,omitempty"`
	Proxies     []Proxy   `json:"proxies"`
}

type Rule struct {
	Type    string `json:"type"`
	Payload string `json:"payload"`
	Proxy   string `json:"proxy"`
	Size    int    `json:"size,omitempty"`
}

type ConnectionSnapshot struct {
	DownloadTotal int64        `json:"download_total"`
	UploadTotal   int64        `json:"upload_total"`
	Connections   []Connection `json:"connections"`
}

type Connection struct {
	ID          string             `json:"id"`
	Metadata    ConnectionMetadata `json:"metadata"`
	Chains      []string           `json:"chains"`
	Rule        string             `json:"rule,omitempty"`
	RulePayload string             `json:"rule_payload,omitempty"`
	Download    int64              `json:"download"`
	Upload      int64              `json:"upload"`
	Start       time.Time          `json:"start,omitempty"`
}

type ConnectionMetadata struct {
	Network         string `json:"network,omitempty"`
	Type            string `json:"type,omitempty"`
	SourceIP        string `json:"source_ip,omitempty"`
	DestinationIP   string `json:"destination_ip,omitempty"`
	SourcePort      string `json:"source_port,omitempty"`
	DestinationPort string `json:"destination_port,omitempty"`
	Host            string `json:"host,omitempty"`
	DNSMode         string `json:"dns_mode,omitempty"`
	Process         string `json:"process,omitempty"`
	ProcessPath     string `json:"process_path,omitempty"`
	InboundUser     string `json:"inbound_user,omitempty"`
}

type HTTPError struct {
	Status int
	Body   string
}

func (err *HTTPError) Error() string {
	return fmt.Sprintf("core API returned HTTP %d: %s", err.Status, err.Body)
}

type Client struct {
	core    string
	baseURL string
	secret  string
	http    *http.Client
}

func New(core, baseURL, secret string) *Client {
	return &Client{
		core:    core,
		baseURL: strings.TrimRight(baseURL, "/"),
		secret:  secret,
		http:    &http.Client{Timeout: 30 * time.Second},
	}
}

func (client *Client) Ready(ctx context.Context) error {
	deadline := time.NewTimer(10 * time.Second)
	defer deadline.Stop()
	ticker := time.NewTicker(100 * time.Millisecond)
	defer ticker.Stop()
	var last error
	for {
		if _, err := client.Version(ctx); err == nil {
			return nil
		} else {
			last = err
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-deadline.C:
			return fmt.Errorf("core control API did not become ready: %w", last)
		case <-ticker.C:
		}
	}
}

func (client *Client) Version(ctx context.Context) (string, error) {
	var response struct {
		Version string `json:"version"`
		Meta    bool   `json:"meta"`
	}
	if err := client.get(ctx, "/version", &response); err != nil {
		return "", err
	}
	return response.Version, nil
}

func (client *Client) Capabilities(ctx context.Context) CapabilitySet {
	capabilities := CapabilitySet{
		"overview":         true,
		"runtime-config":   true,
		"proxies":          true,
		"latency":          true,
		"rules":            true,
		"connections":      true,
		"connection-close": true,
		"traffic":          true,
		"memory":           true,
		"logs":             true,
		"dns-query":        true,
		"reload":           true,
	}
	probes := []struct {
		path string
		keys []string
	}{
		{"/proxies", []string{"proxies", "latency"}},
		{"/providers/proxies", []string{"providers", "provider-update"}},
		{"/rules", []string{"rules"}},
		{"/providers/rules", []string{"rule-providers"}},
		{"/connections", []string{"connections", "connection-close"}},
	}
	for _, probe := range probes {
		probeCtx, cancel := context.WithTimeout(ctx, 2*time.Second)
		var value any
		err := client.get(probeCtx, probe.path, &value)
		cancel()
		if err != nil {
			for _, key := range probe.keys {
				delete(capabilities, key)
			}
		}
	}
	return capabilities
}

func (client *Client) Overview(ctx context.Context) (Overview, error) {
	version, err := client.Version(ctx)
	if err != nil {
		return Overview{}, err
	}
	config, _ := client.Config(ctx)
	connections, _ := client.Connections(ctx)
	mode, _ := config["mode"].(string)
	return Overview{
		Core:        client.core,
		Version:     version,
		Mode:        mode,
		Connections: len(connections.Connections),
		Download:    connections.DownloadTotal,
		Upload:      connections.UploadTotal,
	}, nil
}

func (client *Client) Config(ctx context.Context) (map[string]any, error) {
	var response map[string]any
	err := client.get(ctx, "/configs", &response)
	return response, err
}

func (client *Client) PatchConfig(ctx context.Context, patch map[string]any) error {
	return client.send(ctx, http.MethodPatch, "/configs", patch, nil)
}

func (client *Client) Reload(ctx context.Context) error {
	return client.send(ctx, http.MethodPut, "/configs?force=true", map[string]any{}, nil)
}

func (client *Client) Proxies(ctx context.Context) ([]Proxy, error) {
	var response struct {
		Proxies proxyList `json:"proxies"`
	}
	response.Proxies = make(proxyList, 0)
	if err := client.get(ctx, "/proxies", &response); err != nil {
		return nil, err
	}
	return response.Proxies, nil
}

func (client *Client) SelectProxy(ctx context.Context, group, proxy string) error {
	return client.send(ctx, http.MethodPut, "/proxies/"+url.PathEscape(group), map[string]string{"name": proxy}, nil)
}

func (client *Client) ProxyDelay(ctx context.Context, name, testURL string, timeout int) (int, error) {
	query := url.Values{"url": []string{testURL}, "timeout": []string{strconv.Itoa(timeout)}}
	var response struct {
		Delay int `json:"delay"`
	}
	err := client.get(ctx, "/proxies/"+url.PathEscape(name)+"/delay?"+query.Encode(), &response)
	return response.Delay, err
}

func (client *Client) Providers(ctx context.Context) ([]ProxyProvider, error) {
	var response struct {
		Providers map[string]providerPayload `json:"providers"`
	}
	if err := client.get(ctx, "/providers/proxies", &response); err != nil {
		return nil, err
	}
	result := make([]ProxyProvider, 0, len(response.Providers))
	for name, item := range response.Providers {
		provider := ProxyProvider{Name: name, Type: item.Type, VehicleType: item.VehicleType, UpdatedAt: item.UpdatedAt}
		for _, proxy := range item.Proxies {
			provider.Proxies = append(provider.Proxies, proxy.normalized(proxy.Name))
		}
		result = append(result, provider)
	}
	sort.Slice(result, func(i, j int) bool { return strings.ToLower(result[i].Name) < strings.ToLower(result[j].Name) })
	return result, nil
}

func (client *Client) UpdateProvider(ctx context.Context, name string) error {
	return client.send(ctx, http.MethodPut, "/providers/proxies/"+url.PathEscape(name), nil, nil)
}

func (client *Client) HealthcheckProvider(ctx context.Context, name string) error {
	return client.send(ctx, http.MethodGet, "/providers/proxies/"+url.PathEscape(name)+"/healthcheck", nil, nil)
}

func (client *Client) Rules(ctx context.Context) ([]Rule, error) {
	var response struct {
		Rules []Rule `json:"rules"`
	}
	err := client.get(ctx, "/rules", &response)
	return response.Rules, err
}

func (client *Client) RuleProviders(ctx context.Context) (map[string]any, error) {
	var response map[string]any
	err := client.get(ctx, "/providers/rules", &response)
	return response, err
}

func (client *Client) UpdateRuleProvider(ctx context.Context, name string) error {
	return client.send(ctx, http.MethodPut, "/providers/rules/"+url.PathEscape(name), nil, nil)
}

func (client *Client) Connections(ctx context.Context) (ConnectionSnapshot, error) {
	var raw struct {
		DownloadTotal int64 `json:"downloadTotal"`
		UploadTotal   int64 `json:"uploadTotal"`
		Connections   []struct {
			ID       string `json:"id"`
			Metadata struct {
				Network         string `json:"network"`
				Type            string `json:"type"`
				SourceIP        string `json:"sourceIP"`
				DestinationIP   string `json:"destinationIP"`
				SourcePort      string `json:"sourcePort"`
				DestinationPort string `json:"destinationPort"`
				Host            string `json:"host"`
				DNSMode         string `json:"dnsMode"`
				Process         string `json:"process"`
				ProcessPath     string `json:"processPath"`
				InboundUser     string `json:"inboundUser"`
			} `json:"metadata"`
			Chains      []string  `json:"chains"`
			Rule        string    `json:"rule"`
			RulePayload string    `json:"rulePayload"`
			Download    int64     `json:"download"`
			Upload      int64     `json:"upload"`
			Start       time.Time `json:"start"`
		} `json:"connections"`
	}
	if err := client.get(ctx, "/connections", &raw); err != nil {
		return ConnectionSnapshot{}, err
	}
	result := ConnectionSnapshot{DownloadTotal: raw.DownloadTotal, UploadTotal: raw.UploadTotal, Connections: []Connection{}}
	for _, item := range raw.Connections {
		result.Connections = append(result.Connections, Connection{
			ID: item.ID,
			Metadata: ConnectionMetadata{
				Network: item.Metadata.Network, Type: item.Metadata.Type, SourceIP: item.Metadata.SourceIP,
				DestinationIP: item.Metadata.DestinationIP, SourcePort: item.Metadata.SourcePort,
				DestinationPort: item.Metadata.DestinationPort, Host: item.Metadata.Host,
				DNSMode: item.Metadata.DNSMode, Process: item.Metadata.Process,
				ProcessPath: item.Metadata.ProcessPath, InboundUser: item.Metadata.InboundUser,
			},
			Chains: item.Chains, Rule: item.Rule, RulePayload: item.RulePayload,
			Download: item.Download, Upload: item.Upload, Start: item.Start,
		})
	}
	return result, nil
}

func (client *Client) CloseConnection(ctx context.Context, id string) error {
	path := "/connections"
	if id != "" {
		path += "/" + url.PathEscape(id)
	}
	return client.send(ctx, http.MethodDelete, path, nil, nil)
}

func (client *Client) DNSQuery(ctx context.Context, name, recordType string) (any, error) {
	query := url.Values{"name": []string{name}, "type": []string{recordType}}
	var response any
	err := client.get(ctx, "/dns/query?"+query.Encode(), &response)
	return response, err
}

func (client *Client) FlushFakeIP(ctx context.Context) error {
	return client.send(ctx, http.MethodPost, "/cache/fakeip/flush", nil, nil)
}

func (client *Client) Stream(ctx context.Context, topic string, receive func(json.RawMessage) error) error {
	paths := map[string]string{
		"traffic":     "/traffic",
		"memory":      "/memory",
		"connections": "/connections",
		"logs":        "/logs?level=debug",
	}
	path := paths[topic]
	if path == "" {
		return fmt.Errorf("unsupported runtime event topic %q", topic)
	}
	endpoint, err := url.Parse(client.baseURL + path)
	if err != nil {
		return err
	}
	if endpoint.Scheme == "http" {
		endpoint.Scheme = "ws"
	} else {
		endpoint.Scheme = "wss"
	}
	header := http.Header{}
	if client.secret != "" {
		header.Set("Authorization", "Bearer "+client.secret)
	}
	connection, _, err := websocket.Dial(ctx, endpoint.String(), &websocket.DialOptions{HTTPHeader: header})
	if err != nil {
		return fmt.Errorf("connect core %s stream: %w", topic, err)
	}
	defer connection.Close(websocket.StatusNormalClosure, "")
	for {
		_, data, err := connection.Read(ctx)
		if err != nil {
			if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
				return nil
			}
			return err
		}
		if !json.Valid(data) {
			continue
		}
		if err := receive(json.RawMessage(data)); err != nil {
			return err
		}
	}
}

func (client *Client) get(ctx context.Context, path string, result any) error {
	return client.send(ctx, http.MethodGet, path, nil, result)
}

func (client *Client) send(ctx context.Context, method, path string, body, result any) error {
	var reader io.Reader
	if body != nil {
		data, err := json.Marshal(body)
		if err != nil {
			return err
		}
		reader = bytes.NewReader(data)
	}
	request, err := http.NewRequestWithContext(ctx, method, client.baseURL+path, reader)
	if err != nil {
		return err
	}
	request.Header.Set("Accept", "application/json")
	if body != nil {
		request.Header.Set("Content-Type", "application/json")
	}
	if client.secret != "" {
		request.Header.Set("Authorization", "Bearer "+client.secret)
	}
	response, err := client.http.Do(request)
	if err != nil {
		return fmt.Errorf("call core API: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		data, _ := io.ReadAll(io.LimitReader(response.Body, 4<<10))
		return &HTTPError{Status: response.StatusCode, Body: strings.TrimSpace(string(data))}
	}
	if result == nil || response.StatusCode == http.StatusNoContent {
		return nil
	}
	if err := json.NewDecoder(io.LimitReader(response.Body, 16<<20)).Decode(result); err != nil {
		return fmt.Errorf("decode core API response: %w", err)
	}
	return nil
}

type proxyPayload struct {
	Name    string    `json:"name"`
	Type    string    `json:"type"`
	Now     string    `json:"now"`
	All     []string  `json:"all"`
	UDP     bool      `json:"udp"`
	History []Latency `json:"history"`
}

type proxyList []Proxy

func (proxies *proxyList) UnmarshalJSON(data []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(data))
	start, err := decoder.Token()
	if err != nil {
		return err
	}
	if start != json.Delim('{') {
		return fmt.Errorf("proxy collection must be an object")
	}
	*proxies = make(proxyList, 0)
	for decoder.More() {
		nameToken, err := decoder.Token()
		if err != nil {
			return err
		}
		name, ok := nameToken.(string)
		if !ok {
			return fmt.Errorf("proxy name must be a string")
		}
		var payload proxyPayload
		if err := decoder.Decode(&payload); err != nil {
			return err
		}
		*proxies = append(*proxies, payload.normalized(name))
	}
	_, err = decoder.Token()
	return err
}

func (payload proxyPayload) normalized(name string) Proxy {
	if payload.Name != "" {
		name = payload.Name
	}
	return Proxy{Name: name, Type: payload.Type, Now: payload.Now, All: payload.All, UDP: payload.UDP, History: payload.History}
}

type providerPayload struct {
	Name        string         `json:"name"`
	Type        string         `json:"type"`
	VehicleType string         `json:"vehicleType"`
	UpdatedAt   time.Time      `json:"updatedAt"`
	Proxies     []proxyPayload `json:"proxies"`
}
