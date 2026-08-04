package subscription

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net/url"
	"strconv"
	"strings"

	"gopkg.in/yaml.v3"
)

func Parse(text string) ParseResult {
	trimmed := strings.TrimSpace(text)
	if trimmed == "" {
		return ParseResult{Format: "unknown", Nodes: []Proxy{}, DiscardedPlaceholders: []Proxy{}, Diagnostics: []string{"Response body is empty"}}
	}
	yamlHint := strings.HasPrefix(trimmed, "proxies:") || strings.HasPrefix(trimmed, "port:") || strings.HasPrefix(trimmed, "#") || strings.Contains(trimmed, "\nproxies:")
	result := ParseResult{Nodes: []Proxy{}, DiscardedPlaceholders: []Proxy{}, Diagnostics: []string{}}
	if decoded, ok := decodeSubscription(trimmed); ok && containsProxyURI(string(decoded)) && !yamlHint {
		result.Format = "base64"
		result.DecodedText = string(decoded)
		for index, line := range strings.Split(string(decoded), "\n") {
			line = strings.TrimSpace(line)
			if line == "" {
				continue
			}
			proxy, err := ParseURI(line)
			if err != nil {
				scheme, _, _ := strings.Cut(line, "://")
				if scheme == "" {
					scheme = "unknown"
				}
				result.Diagnostics = append(result.Diagnostics, fmt.Sprintf("Line %d uses an unsupported or invalid proxy URI (%s)", index+1, scheme))
				continue
			}
			result.Nodes = append(result.Nodes, proxy)
		}
	} else {
		result.Format = "yaml"
		var document struct {
			Proxies []map[string]any `yaml:"proxies"`
		}
		if err := yaml.Unmarshal([]byte(text), &document); err != nil {
			if !yamlHint {
				result.Format = "unknown"
			}
			result.Diagnostics = append(result.Diagnostics, "YAML parse failed: "+err.Error())
			return result
		}
		for index, value := range document.Proxies {
			proxy, err := ProxyFromMap(value)
			if err != nil {
				result.Diagnostics = append(result.Diagnostics, fmt.Sprintf("Proxy %d is invalid: %v", index+1, err))
				continue
			}
			result.Nodes = append(result.Nodes, proxy)
		}
		if len(result.Nodes) == 0 {
			result.Diagnostics = append(result.Diagnostics, "YAML response contains no proxy nodes")
		}
	}
	usable := result.Nodes[:0]
	for _, proxy := range result.Nodes {
		if (proxy.Server == "127.0.0.1" || proxy.Server == "::1" || proxy.Server == "localhost") && proxy.Port <= 1 {
			result.DiscardedPlaceholders = append(result.DiscardedPlaceholders, proxy)
		} else {
			usable = append(usable, proxy)
		}
	}
	result.Nodes = usable
	if count := len(result.DiscardedPlaceholders); count > 0 {
		result.Diagnostics = append(result.Diagnostics, fmt.Sprintf("Discarded %d placeholder node(s) using a loopback address and port 0 or 1", count))
	}
	return result
}

func ProxyFromMap(value map[string]any) (Proxy, error) {
	proxy := Proxy{Extra: map[string]any{}}
	for key, item := range value {
		switch key {
		case "name":
			proxy.Name, _ = item.(string)
		case "type":
			proxy.Type, _ = item.(string)
		case "server":
			proxy.Server, _ = item.(string)
		case "port":
			proxy.Port = integer(item)
		default:
			proxy.Extra[key] = item
		}
	}
	proxy.Type = strings.ToLower(proxy.Type)
	if proxy.Name == "" || proxy.Type == "" || proxy.Server == "" || proxy.Port < 0 || proxy.Port > 65535 {
		return Proxy{}, fmt.Errorf("name, type, server, and a valid port are required")
	}
	return proxy, nil
}

func ParseURI(value string) (Proxy, error) {
	switch {
	case strings.HasPrefix(value, "vless://"):
		return parseVLESSURI(value)
	case strings.HasPrefix(value, "trojan://"):
		return parseTrojanURI(value)
	case strings.HasPrefix(value, "hysteria2://"), strings.HasPrefix(value, "hy2://"):
		return parseHysteria2URI(value)
	case strings.HasPrefix(value, "anytls://"):
		return parseAnyTLSURI(value)
	case strings.HasPrefix(value, "vmess://"):
		return parseVMessURI(value)
	case strings.HasPrefix(value, "ss://"):
		return parseSSURI(value)
	default:
		return Proxy{}, fmt.Errorf("unsupported proxy URI")
	}
}

func parseProxyURL(value, proxyType string) (*url.URL, string, int, error) {
	parsed, err := url.Parse(value)
	if err != nil || parsed.Hostname() == "" || parsed.Port() == "" {
		return nil, "", 0, fmt.Errorf("invalid %s URI", proxyType)
	}
	port, err := strconv.Atoi(parsed.Port())
	if err != nil || port < 0 || port > 65535 {
		return nil, "", 0, fmt.Errorf("invalid port")
	}
	name, _ := url.PathUnescape(parsed.Fragment)
	if name == "" {
		name = fmt.Sprintf("%s:%d", parsed.Hostname(), port)
	}
	return parsed, name, port, nil
}

func parseVLESSURI(value string) (Proxy, error) {
	parsed, name, port, err := parseProxyURL(value, "vless")
	if err != nil {
		return Proxy{}, err
	}
	uuid := ""
	if parsed.User != nil {
		uuid = parsed.User.Username()
	}
	extra := map[string]any{"uuid": uuid, "udp": true}
	query := parsed.Query()
	network := valueOr(query.Get("type"), "tcp")
	if network != "tcp" {
		extra["network"] = network
	}
	if network == "ws" {
		options := map[string]any{"path": valueOr(query.Get("path"), "/")}
		if host := query.Get("host"); host != "" {
			options["headers"] = map[string]any{"Host": host}
		}
		extra["ws-opts"] = options
	}
	if network == "grpc" {
		extra["grpc-opts"] = map[string]any{"grpc-service-name": query.Get("serviceName")}
	}
	security := query.Get("security")
	if security == "tls" {
		extra["tls"] = true
		if sni := query.Get("sni"); sni != "" {
			extra["servername"] = sni
		}
		if fingerprint := query.Get("fp"); fingerprint != "" {
			extra["client-fingerprint"] = fingerprint
		}
		if alpn := query.Get("alpn"); alpn != "" {
			extra["alpn"] = strings.Split(alpn, ",")
		}
		if query.Get("insecure") == "1" {
			extra["skip-cert-verify"] = true
		}
	}
	if security == "reality" {
		extra["tls"] = true
		extra["reality-opts"] = map[string]any{"public-key": query.Get("pbk"), "short-id": query.Get("sid")}
		if sni := query.Get("sni"); sni != "" {
			extra["servername"] = sni
		}
		if fingerprint := query.Get("fp"); fingerprint != "" {
			extra["client-fingerprint"] = fingerprint
		}
	}
	if flow := query.Get("flow"); flow != "" {
		extra["flow"] = flow
	}
	return Proxy{Name: name, Type: "vless", Server: parsed.Hostname(), Port: port, Extra: extra}, nil
}

func parseTrojanURI(value string) (Proxy, error) {
	parsed, name, port, err := parseProxyURL(value, "trojan")
	if err != nil {
		return Proxy{}, err
	}
	password := ""
	if parsed.User != nil {
		password = parsed.User.Username()
	}
	extra := map[string]any{"password": password, "udp": true}
	query := parsed.Query()
	if sni := query.Get("sni"); sni != "" {
		extra["sni"] = sni
	}
	if alpn := query.Get("alpn"); alpn != "" {
		extra["alpn"] = strings.Split(alpn, ",")
	}
	if fingerprint := query.Get("fp"); fingerprint != "" {
		extra["client-fingerprint"] = fingerprint
	}
	if query.Get("insecure") == "1" || query.Get("allowInsecure") == "1" {
		extra["skip-cert-verify"] = true
	}
	network := valueOr(query.Get("type"), "tcp")
	if network == "ws" || network == "grpc" {
		extra["network"] = network
	}
	if network == "ws" {
		options := map[string]any{"path": valueOr(query.Get("path"), "/")}
		if host := query.Get("host"); host != "" {
			options["headers"] = map[string]any{"Host": host}
		}
		extra["ws-opts"] = options
	}
	if network == "grpc" {
		extra["grpc-opts"] = map[string]any{"grpc-service-name": query.Get("serviceName")}
	}
	return Proxy{Name: name, Type: "trojan", Server: parsed.Hostname(), Port: port, Extra: extra}, nil
}

func parseHysteria2URI(value string) (Proxy, error) {
	parsed, name, port, err := parseProxyURL(value, "hysteria2")
	if err != nil {
		return Proxy{}, err
	}
	password := ""
	if parsed.User != nil {
		password = parsed.User.Username()
	}
	extra := map[string]any{"password": password}
	query := parsed.Query()
	if sni := query.Get("sni"); sni != "" {
		extra["sni"] = sni
	}
	if obfs, obfsPassword := query.Get("obfs"), query.Get("obfs-password"); obfs != "" && obfsPassword != "" {
		extra["obfs"] = obfs
		extra["obfs-password"] = obfsPassword
	}
	if query.Get("insecure") == "1" {
		extra["skip-cert-verify"] = true
	}
	if alpn := query.Get("alpn"); alpn != "" {
		extra["alpn"] = strings.Split(alpn, ",")
	}
	return Proxy{Name: name, Type: "hysteria2", Server: parsed.Hostname(), Port: port, Extra: extra}, nil
}

func parseAnyTLSURI(value string) (Proxy, error) {
	parsed, name, port, err := parseProxyURL(value, "anytls")
	if err != nil {
		return Proxy{}, err
	}
	password := ""
	if parsed.User != nil {
		password = parsed.User.Username()
	}
	extra := map[string]any{"password": password, "udp": true}
	query := parsed.Query()
	if sni := query.Get("sni"); sni != "" {
		extra["sni"] = sni
	}
	if query.Get("insecure") == "1" {
		extra["skip-cert-verify"] = true
	}
	if fingerprint := query.Get("fp"); fingerprint != "" {
		extra["client-fingerprint"] = fingerprint
	}
	if alpn := query.Get("alpn"); alpn != "" {
		extra["alpn"] = strings.Split(alpn, ",")
	}
	return Proxy{Name: name, Type: "anytls", Server: parsed.Hostname(), Port: port, Extra: extra}, nil
}

func parseVMessURI(value string) (Proxy, error) {
	content := strings.TrimPrefix(value, "vmess://")
	content, _, _ = strings.Cut(content, "#")
	if decoded, ok := decodeSubscription(content); ok {
		var object map[string]any
		if json.Unmarshal(decoded, &object) == nil {
			server, _ := object["add"].(string)
			port := integer(object["port"])
			if server != "" && port > 0 && port <= 65535 {
				name, _ := object["ps"].(string)
				if name == "" {
					name, _ = object["remarks"].(string)
				}
				if name == "" {
					name = fmt.Sprintf("%s:%d", server, port)
				}
				extra := map[string]any{"alterId": integer(object["aid"]), "cipher": valueOr(stringValue(object["scy"]), "auto"), "udp": true}
				if id := stringValue(object["id"]); id != "" {
					extra["uuid"] = id
				}
				network := valueOr(stringValue(object["net"]), "tcp")
				if network != "tcp" {
					extra["network"] = network
				}
				if network == "ws" {
					opts := map[string]any{"path": valueOr(stringValue(object["path"]), "/")}
					if host := stringValue(object["host"]); host != "" {
						opts["headers"] = map[string]any{"Host": host}
					}
					extra["ws-opts"] = opts
				}
				if network == "grpc" {
					extra["grpc-opts"] = map[string]any{"grpc-service-name": stringValue(object["path"])}
				}
				if stringValue(object["tls"]) == "tls" {
					extra["tls"] = true
					copyObject(extra, object, "sni", "servername", "fp", "client-fingerprint")
					switch alpn := object["alpn"].(type) {
					case string:
						if alpn != "" {
							extra["alpn"] = strings.Split(alpn, ",")
						}
					case []any:
						extra["alpn"] = alpn
					}
				}
				return Proxy{Name: name, Type: "vmess", Server: server, Port: port, Extra: extra}, nil
			}
		}
	}
	parsed, name, port, err := parseProxyURL(value, "vmess")
	if err != nil {
		return Proxy{}, err
	}
	uuid := ""
	if parsed.User != nil {
		uuid = parsed.User.Username()
	}
	return Proxy{Name: name, Type: "vmess", Server: parsed.Hostname(), Port: port, Extra: map[string]any{"uuid": uuid, "alterId": 0, "cipher": "auto", "udp": true}}, nil
}

func parseSSURI(value string) (Proxy, error) {
	parsed, err := url.Parse(value)
	if err == nil && parsed.Hostname() != "" && parsed.Port() != "" && parsed.User != nil {
		port, portErr := strconv.Atoi(parsed.Port())
		if portErr != nil || port < 0 || port > 65535 {
			return Proxy{}, fmt.Errorf("invalid ss URI")
		}
		userinfo := parsed.User.Username()
		decoded, ok := decodeSubscription(userinfo)
		if ok {
			method, password, found := strings.Cut(string(decoded), ":")
			if found {
				name, _ := url.PathUnescape(parsed.Fragment)
				if name == "" {
					name = fmt.Sprintf("%s:%d", parsed.Hostname(), port)
				}
				extra := map[string]any{"cipher": method, "password": password, "udp": true}
				if plugin := parsed.Query().Get("plugin"); plugin != "" {
					parseSSPlugin(plugin, extra)
				}
				return Proxy{Name: name, Type: "ss", Server: parsed.Hostname(), Port: port, Extra: extra}, nil
			}
		}
	}
	raw := strings.TrimPrefix(value, "ss://")
	raw, fragment, _ := strings.Cut(raw, "#")
	raw, _, _ = strings.Cut(raw, "?")
	decoded, ok := decodeSubscription(raw)
	if !ok {
		return Proxy{}, fmt.Errorf("invalid ss URI")
	}
	decodedText := string(decoded)
	at := strings.LastIndex(decodedText, "@")
	if at < 0 {
		return Proxy{}, fmt.Errorf("invalid ss URI")
	}
	userinfo, address := decodedText[:at], decodedText[at+1:]
	method, password, credentials := strings.Cut(userinfo, ":")
	colon := strings.LastIndex(address, ":")
	if colon < 0 {
		return Proxy{}, fmt.Errorf("invalid ss URI")
	}
	host, portText := address[:colon], address[colon+1:]
	port, portErr := strconv.Atoi(portText)
	if !credentials || host == "" || portErr != nil || port < 0 || port > 65535 {
		return Proxy{}, fmt.Errorf("invalid ss URI")
	}
	name, _ := url.PathUnescape(fragment)
	if name == "" {
		name = fmt.Sprintf("%s:%d", host, port)
	}
	return Proxy{Name: name, Type: "ss", Server: host, Port: port, Extra: map[string]any{"cipher": method, "password": password, "udp": true}}, nil
}

func parseSSPlugin(plugin string, extra map[string]any) {
	parts := strings.Split(plugin, ";")
	extra["plugin"] = parts[0]
	options := map[string]any{}
	for _, part := range parts[1:] {
		key, value, ok := strings.Cut(part, "=")
		if ok {
			options[key] = value
		}
	}
	if parts[0] == "obfs-local" || parts[0] == "obfs" {
		if value, ok := options["obfs"]; ok {
			options["mode"] = value
			delete(options, "obfs")
		}
		if value, ok := options["obfs-host"]; ok {
			options["host"] = value
			delete(options, "obfs-host")
		}
	}
	if len(options) > 0 {
		extra["plugin-opts"] = options
	}
}

func decodeSubscription(value string) ([]byte, bool) {
	for _, encoding := range []*base64.Encoding{base64.StdEncoding, base64.RawStdEncoding, base64.URLEncoding, base64.RawURLEncoding} {
		if decoded, err := encoding.DecodeString(strings.TrimSpace(value)); err == nil {
			return decoded, true
		}
	}
	return nil, false
}

func containsProxyURI(value string) bool {
	for _, line := range strings.Split(value, "\n") {
		line = strings.TrimSpace(line)
		for _, prefix := range []string{"vless://", "vmess://", "ss://", "trojan://", "ssr://", "hysteria://", "hysteria2://", "hy2://", "anytls://"} {
			if strings.HasPrefix(line, prefix) {
				return true
			}
		}
	}
	return false
}

func integer(value any) int {
	switch item := value.(type) {
	case int:
		return item
	case int64:
		return int(item)
	case uint64:
		return int(item)
	case float64:
		return int(item)
	case string:
		result, _ := strconv.Atoi(item)
		return result
	}
	return 0
}
func stringValue(value any) string { result, _ := value.(string); return result }
func valueOr(value, fallback string) string {
	if value == "" {
		return fallback
	}
	return value
}
func copyObject(target, source map[string]any, pairs ...string) {
	for index := 0; index+1 < len(pairs); index += 2 {
		if value := source[pairs[index]]; value != nil && value != "" {
			target[pairs[index+1]] = value
		}
	}
}
