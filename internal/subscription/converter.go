package subscription

import (
	"fmt"
	"sort"
	"strconv"
	"strings"
)

func ConvertProxy(proxy Proxy) (map[string]any, FieldDiff, bool) {
	consumed := consumedKeys(proxy.Type)
	ignored := ignoredKeys(proxy.Type)
	diff := FieldDiff{Node: proxy.Name, Consumed: []string{}, Ignored: []string{}, Dropped: []string{}, Warnings: []string{}}
	for key := range proxy.Extra {
		if consumed[key] {
			diff.Consumed = append(diff.Consumed, key)
		} else if ignored[key] {
			diff.Ignored = append(diff.Ignored, key)
		} else {
			diff.Dropped = append(diff.Dropped, key)
		}
	}
	sort.Strings(diff.Consumed)
	sort.Strings(diff.Ignored)
	sort.Strings(diff.Dropped)
	base := map[string]any{"tag": proxy.Name, "server": proxy.Server, "server_port": proxy.Port}
	stringField := func(key string) string { value, _ := proxy.Extra[key].(string); return value }
	boolField := func(key string) bool { value, _ := proxy.Extra[key].(bool); return value }
	if boolField("tfo") {
		base["tcp_fast_open"] = true
	}
	if boolField("mptcp") {
		base["tcp_multi_path"] = true
	}

	switch proxy.Type {
	case "vmess":
		base["type"], base["uuid"], base["security"], base["alter_id"] = "vmess", stringField("uuid"), valueOr(stringField("cipher"), "auto"), integer(proxy.Extra["alterId"])
		addTransportTLSMultiplex(base, proxy, false)
	case "vless":
		base["type"], base["uuid"] = "vless", stringField("uuid")
		if flow := stringField("flow"); flow != "" {
			base["flow"] = flow
		}
		addTransportTLSMultiplex(base, proxy, false)
	case "ss":
		base["type"], base["method"], base["password"] = "shadowsocks", valueOr(stringField("cipher"), "aes-256-gcm"), stringField("password")
		if value, ok := proxy.Extra["udp"].(bool); ok && !value {
			base["network"] = "tcp"
		}
		if plugin := stringField("plugin"); plugin != "" {
			if plugin == "shadow-tls" {
				if options, ok := objectValue(proxy.Extra["plugin-opts"]); ok {
					base["type"] = "shadowtls"
					delete(base, "method")
					if password := stringValue(options["password"]); password != "" {
						base["password"] = password
					}
					if version := options["version"]; version != nil {
						base["version"] = version
					}
					if host := stringValue(options["host"]); host != "" {
						base["tls"] = map[string]any{"enabled": true, "server_name": host}
					}
				}
			} else {
				if plugin == "obfs" {
					plugin = "obfs-local"
				}
				base["plugin"] = plugin
				if options, ok := objectValue(proxy.Extra["plugin-opts"]); ok {
					base["plugin_opts"] = pluginOptions(options, plugin)
				}
			}
		}
	case "trojan":
		base["type"], base["password"] = "trojan", stringField("password")
		addTransportTLSMultiplex(base, proxy, true)
		if value, ok := proxy.Extra["udp"].(bool); ok && !value {
			base["network"] = "tcp"
		}
	case "hysteria2":
		base["type"], base["password"] = "hysteria2", stringField("password")
		base["tls"] = protocolTLS(proxy, valueOr(stringField("sni"), proxy.Server), true, false)
		if ports := stringField("ports"); ports != "" {
			base["server_ports"] = []string{strings.ReplaceAll(ports, "-", ":")}
		}
		if value := parseMbps(stringField("up")); value > 0 {
			base["up_mbps"] = value
		}
		if value := parseMbps(stringField("down")); value > 0 {
			base["down_mbps"] = value
		}
	case "hysteria":
		base["type"], base["up"], base["down"] = "hysteria", stringField("up"), stringField("down")
		base["tls"] = protocolTLS(proxy, valueOr(stringField("sni"), proxy.Server), true, false)
		copyFields(base, proxy.Extra, "obfs", "auth-str")
		if value, ok := base["auth-str"]; ok {
			base["auth_str"] = value
			delete(base, "auth-str")
		}
	case "tuic":
		base["type"], base["uuid"] = "tuic", stringField("uuid")
		base["tls"] = protocolTLS(proxy, stringField("sni"), true, false)
		copyFields(base, proxy.Extra, "password")
		if interval := integer(proxy.Extra["heartbeat-interval"]); interval > 0 {
			base["heartbeat"] = fmt.Sprintf("%ds", interval/1000)
		}
		if boolField("reduce-rtt") {
			base["zero_rtt_handshake"] = true
		}
		fieldAliases(base, proxy.Extra, map[string]string{"udp-relay-mode": "udp_relay_mode", "congestion-controller": "congestion_control"})
		if boolField("udp-over-stream") {
			base["udp_over_stream"] = true
		}
	case "http":
		base["type"] = "http"
		if username := stringField("username"); username != "" {
			base["username"] = username
			if password := stringField("password"); password != "" {
				base["password"] = password
			}
		}
		if boolField("tls") {
			base["tls"] = protocolTLS(proxy, stringField("sni"), false, false)
		}
	case "socks5":
		base["type"] = "socks"
		if username := stringField("username"); username != "" {
			base["username"] = username
			if password := stringField("password"); password != "" {
				base["password"] = password
			}
		}
		if value, ok := proxy.Extra["udp"].(bool); ok && !value {
			base["network"] = "tcp"
		}
	case "anytls":
		base["type"], base["password"] = "anytls", stringField("password")
		base["tls"] = protocolTLS(proxy, stringField("sni"), true, true)
	default:
		diff.Warnings = append(diff.Warnings, "unsupported proxy type "+proxy.Type)
		return nil, diff, false
	}
	if len(diff.Dropped) > 0 {
		diff.Warnings = append(diff.Warnings, "fields not representable in sing-box: "+strings.Join(diff.Dropped, ", "))
	}
	diff.Outbound = base
	diff.FieldOrigins = buildFieldOrigins(proxy, base)
	return base, diff, true
}

func addTransportTLSMultiplex(out map[string]any, proxy Proxy, forceTLS bool) {
	if transport := convertTransport(proxy); transport != nil {
		out["transport"] = transport
	}
	if tls := buildTLS(proxy, forceTLS); tls != nil {
		out["tls"] = tls
	}
	if multiplex := buildMultiplex(proxy); multiplex != nil {
		out["multiplex"] = multiplex
	}
}

func convertTransport(proxy Proxy) map[string]any {
	if options, ok := objectValue(proxy.Extra["http-opts"]); ok {
		result := map[string]any{"type": "http"}
		copyFields(result, options, "method")
		if paths, ok := options["path"].([]any); ok && len(paths) > 0 {
			result["path"] = paths[0]
		}
		if headers, ok := objectValue(options["headers"]); ok {
			normalized := map[string]any{}
			for key, value := range headers {
				if values, ok := value.([]any); ok && len(values) > 0 {
					normalized[key] = values[0]
				}
			}
			if len(normalized) > 0 {
				result["headers"] = normalized
			}
		}
		return result
	}
	if options, ok := objectValue(proxy.Extra["h2-opts"]); ok {
		result := map[string]any{"type": "http"}
		copyFields(result, options, "host", "path")
		return result
	}
	if options, ok := objectValue(proxy.Extra["ws-opts"]); ok {
		result := map[string]any{"type": "ws"}
		copyFields(result, options, "path")
		if headers := options["headers"]; headers != nil {
			result["headers"] = headers
			fieldAliases(result, options, map[string]string{"max-early-data": "max_early_data", "early-data-header-name": "early_data_header_name"})
		}
		return result
	}
	if options, ok := objectValue(proxy.Extra["grpc-opts"]); ok {
		return map[string]any{"type": "grpc", "service_name": stringValue(options["grpc-service-name"])}
	}
	return nil
}

func buildTLS(proxy Proxy, force bool) map[string]any {
	enabled, _ := proxy.Extra["tls"].(bool)
	if !enabled && !force {
		return nil
	}
	name := stringValue(proxy.Extra["servername"])
	if name == "" {
		name = stringValue(proxy.Extra["sni"])
	}
	return simpleTLS(proxy, name)
}

func simpleTLS(proxy Proxy, serverName string) map[string]any {
	result := map[string]any{"enabled": true}
	if serverName != "" {
		result["server_name"] = serverName
	}
	if alpn := proxy.Extra["alpn"]; alpn != nil {
		result["alpn"] = alpn
	}
	if insecure, _ := proxy.Extra["skip-cert-verify"].(bool); insecure {
		result["insecure"] = true
	}
	if fingerprint := stringValue(proxy.Extra["client-fingerprint"]); fingerprint != "" {
		result["utls"] = map[string]any{"enabled": true, "fingerprint": fingerprint}
	}
	if reality, ok := objectValue(proxy.Extra["reality-opts"]); ok {
		value := map[string]any{"enabled": true}
		if publicKey := stringValue(reality["public-key"]); publicKey != "" {
			value["public_key"] = publicKey
		}
		if shortID := stringValue(reality["short-id"]); shortID != "" {
			value["short_id"] = shortID
		}
		result["reality"] = value
	}
	return result
}

func protocolTLS(proxy Proxy, serverName string, includeALPN, includeFingerprint bool) map[string]any {
	result := map[string]any{"enabled": true}
	if serverName != "" {
		result["server_name"] = serverName
	}
	if includeALPN {
		if alpn := proxy.Extra["alpn"]; alpn != nil {
			result["alpn"] = alpn
		}
	}
	if insecure, _ := proxy.Extra["skip-cert-verify"].(bool); insecure {
		result["insecure"] = true
	}
	if includeFingerprint {
		if fingerprint := stringValue(proxy.Extra["client-fingerprint"]); fingerprint != "" {
			result["utls"] = map[string]any{"enabled": true, "fingerprint": fingerprint}
		}
	}
	return result
}

func buildMultiplex(proxy Proxy) map[string]any {
	value := proxy.Extra["smux"]
	if value == nil {
		value = proxy.Extra["multiplex"]
	}
	if enabled, ok := value.(bool); ok {
		if !enabled {
			return nil
		}
		return map[string]any{"enabled": true, "protocol": "h2mux", "max_connections": 8, "min_streams": 16, "padding": true}
	}
	options, ok := objectValue(value)
	if !ok {
		return nil
	}
	if enabled, present := options["enabled"].(bool); present && !enabled {
		return nil
	}
	return map[string]any{"enabled": true, "protocol": valueOr(stringValue(options["protocol"]), "h2mux"), "max_connections": integerDefault(options["max-connections"], 8), "min_streams": integerDefault(options["min-streams"], 16), "padding": boolDefault(options["padding"], true)}
}

func consumedKeys(proxyType string) map[string]bool {
	keys := map[string]bool{"udp": true, "tfo": true, "mptcp": true}
	add := func(values ...string) {
		for _, value := range values {
			keys[value] = true
		}
	}
	transport := []string{"http-opts", "h2-opts", "ws-opts", "grpc-opts", "network"}
	tls := []string{"tls", "servername", "sni", "alpn", "skip-cert-verify", "client-fingerprint", "reality-opts"}
	switch proxyType {
	case "vmess":
		add(append(append(transport, tls...), "multiplex", "smux", "uuid", "cipher", "alterId")...)
	case "vless":
		add(append(append(transport, tls...), "multiplex", "smux", "uuid", "flow")...)
	case "ss":
		add("cipher", "password", "plugin", "plugin-opts")
	case "trojan":
		add(append(append(transport, tls...), "multiplex", "smux", "password")...)
	case "hysteria2":
		add("sni", "skip-cert-verify", "password", "alpn", "ports", "mport", "hop-interval", "up", "down")
	case "hysteria":
		add("sni", "alpn", "skip-cert-verify", "up", "down", "obfs", "auth-str")
	case "tuic":
		add("sni", "skip-cert-verify", "alpn", "uuid", "password", "heartbeat-interval", "reduce-rtt", "udp-relay-mode", "congestion-controller", "udp-over-stream")
	case "http":
		add("username", "password", "tls", "skip-cert-verify", "sni")
	case "socks5":
		add("username", "password")
	case "anytls":
		add("sni", "skip-cert-verify", "alpn", "client-fingerprint", "password")
	}
	return keys
}

func ignoredKeys(proxyType string) map[string]bool {
	if proxyType == "hysteria2" || proxyType == "hysteria" || proxyType == "tuic" {
		return map[string]bool{"multiplex": true, "smux": true}
	}
	return map[string]bool{}
}

func objectValue(value any) (map[string]any, bool) {
	result, ok := value.(map[string]any)
	return result, ok
}
func copyFields(target, source map[string]any, keys ...string) {
	for _, key := range keys {
		if value := source[key]; value != nil {
			target[key] = value
		}
	}
}
func fieldAliases(target, source map[string]any, aliases map[string]string) {
	for from, to := range aliases {
		if value := source[from]; value != nil {
			target[to] = value
		}
	}
}
func integerDefault(value any, fallback int) int {
	if result := integer(value); result != 0 {
		return result
	}
	return fallback
}
func boolDefault(value any, fallback bool) bool {
	result, ok := value.(bool)
	if ok {
		return result
	}
	return fallback
}
func parseMbps(value string) int {
	value = strings.TrimSpace(value)
	value = strings.TrimSpace(strings.TrimRightFunc(value, func(r rune) bool { return (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || r == ' ' }))
	result, _ := strconv.Atoi(value)
	return result
}
func pluginOptions(options map[string]any, plugin string) string {
	parts := []string{}
	if value := stringValue(options["mode"]); value != "" {
		parts = append(parts, "mode="+value)
	}
	if value := stringValue(options["host"]); value != "" {
		parts = append(parts, "host="+value)
	}
	if plugin == "v2ray-plugin" {
		if value, _ := options["tls"].(bool); value {
			parts = append(parts, "tls")
		}
		if value := stringValue(options["path"]); value != "" {
			parts = append(parts, "path="+value)
		}
		if value := options["mux"]; value != nil {
			parts = append(parts, "mux="+fmt.Sprint(value))
		}
	}
	return strings.Join(parts, ";")
}
