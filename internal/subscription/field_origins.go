package subscription

import "strings"

func clashFieldDiffs(nodes []Proxy) []FieldDiff {
	result := make([]FieldDiff, 0, len(nodes))
	for _, node := range nodes {
		outbound := node.Map()
		consumed := sortedKeys(node.Extra)
		origins := map[string]FieldOrigin{}
		for key, value := range outbound {
			sourceKey := key
			if key == "name" {
				sourceKey = "name"
			}
			origins[key] = mappedOrigin(sourceKey, value, "clash", "direct")
		}
		result = append(result, FieldDiff{
			Node: node.Name, Consumed: consumed, Ignored: []string{}, Dropped: []string{}, Warnings: []string{},
			Outbound: outbound, FieldOrigins: origins,
		})
	}
	return result
}

func buildFieldOrigins(proxy Proxy, outbound map[string]any) map[string]FieldOrigin {
	result := map[string]FieldOrigin{}
	walkOutputFields(outbound, "", func(path string) {
		result[path] = outputFieldOrigin(proxy, outbound, path)
	})
	return result
}

func walkOutputFields(value any, prefix string, visit func(string)) {
	object, ok := value.(map[string]any)
	if !ok {
		if prefix != "" {
			visit(prefix)
		}
		return
	}
	for key, child := range object {
		path := key
		if prefix != "" {
			path = prefix + "." + key
		}
		if _, nested := child.(map[string]any); nested {
			walkOutputFields(child, path, visit)
			continue
		}
		visit(path)
	}
}

func outputFieldOrigin(proxy Proxy, outbound map[string]any, path string) FieldOrigin {
	switch path {
	case "type":
		if outbound["type"] == "shadowtls" {
			return mappedOrigin("plugin", proxy.Extra["plugin"], "type", "convert")
		}
		transform := "direct"
		if outbound["type"] != proxy.Type {
			transform = "convert"
		}
		return mappedOrigin("type", proxy.Type, "core", transform)
	case "tag":
		return mappedOrigin("name", proxy.Name, "core", "rename")
	case "server":
		return mappedOrigin("server", proxy.Server, "core", "direct")
	case "server_port":
		return mappedOrigin("port", proxy.Port, "core", "rename")
	case "network":
		return mappedOrigin("udp", false, "core", "convert")
	case "tcp_fast_open":
		return mappedOrigin("tfo", true, "dial", "rename")
	case "tcp_multi_path":
		return mappedOrigin("mptcp", true, "dial", "rename")
	case "domain_resolver.server", "domain_resolver.strategy":
		return generatedOrigin("target", "desktop_domain_resolution")
	}

	if strings.HasPrefix(path, "tls.") {
		return tlsFieldOrigin(proxy, path)
	}
	if strings.HasPrefix(path, "transport.") {
		if path == "transport.type" {
			source := transportSource(proxy)
			return mappedOrigin(source, proxy.Extra[source], "transport", "convert")
		}
		return nestedFieldOrigin(proxy, path, transportSource(proxy), "transport")
	}
	if strings.HasPrefix(path, "multiplex.") {
		source := "multiplex"
		if proxy.Extra["smux"] != nil {
			source = "smux"
		}
		return nestedFieldOrigin(proxy, path, source, "multiplex")
	}

	sources := map[string]string{
		"uuid": "uuid", "security": "cipher", "alter_id": "alterId", "flow": "flow",
		"method": "cipher", "password": "password", "plugin": "plugin", "plugin_opts": "plugin-opts", "version": "plugin-opts.version",
		"server_ports": "ports", "up_mbps": "up", "down_mbps": "down", "up": "up", "down": "down", "obfs": "obfs", "auth_str": "auth-str",
		"heartbeat": "heartbeat-interval", "zero_rtt_handshake": "reduce-rtt", "udp_relay_mode": "udp-relay-mode", "congestion_control": "congestion-controller", "udp_over_stream": "udp-over-stream",
		"username": "username",
	}
	if source := sources[path]; source != "" {
		if path == "password" && outbound["type"] == "shadowtls" {
			source = "plugin-opts.password"
		}
		transform := "direct"
		if source != path {
			transform = "convert"
		}
		return mappedOrigin(source, proxySourceValue(proxy, source), "type", transform)
	}
	return generatedOrigin("converter", "converter_internal")
}

func tlsFieldOrigin(proxy Proxy, path string) FieldOrigin {
	switch path {
	case "tls.enabled":
		if proxy.Type == "trojan" || proxy.Type == "hysteria2" || proxy.Type == "hysteria" || proxy.Type == "tuic" || proxy.Type == "anytls" {
			return generatedOrigin("tls", proxy.Type+"_requires_tls")
		}
		if value, ok := proxy.Extra["tls"]; ok {
			return mappedOrigin("tls", value, "tls", "convert")
		}
		return generatedOrigin("tls", proxy.Type+"_requires_tls")
	case "tls.server_name":
		for _, key := range []string{"servername", "sni"} {
			if value := stringValue(proxy.Extra[key]); value != "" {
				return mappedOrigin(key, value, "tls", "rename")
			}
		}
		return mappedOrigin("server", proxy.Server, "tls", "fallback")
	case "tls.alpn":
		return mappedOrigin("alpn", proxy.Extra["alpn"], "tls", "direct")
	case "tls.insecure":
		return mappedOrigin("skip-cert-verify", proxy.Extra["skip-cert-verify"], "tls", "rename")
	case "tls.utls.enabled":
		return generatedOrigin("tls", "fingerprint_enables_utls")
	case "tls.utls.fingerprint":
		return mappedOrigin("client-fingerprint", proxy.Extra["client-fingerprint"], "tls", "rename")
	case "tls.reality.enabled":
		return generatedOrigin("tls", "reality_opts_present")
	case "tls.reality.public_key":
		return mappedOrigin("reality-opts.public-key", proxySourceValue(proxy, "reality-opts.public-key"), "tls", "extract")
	case "tls.reality.short_id":
		return mappedOrigin("reality-opts.short-id", proxySourceValue(proxy, "reality-opts.short-id"), "tls", "extract")
	default:
		return generatedOrigin("tls", "converter_internal")
	}
}

func nestedFieldOrigin(proxy Proxy, outputPath, sourceRoot, step string) FieldOrigin {
	field := strings.TrimPrefix(outputPath, step+".")
	aliases := map[string]string{
		"service_name": "grpc-service-name", "max_early_data": "max-early-data", "early_data_header_name": "early-data-header-name",
		"max_connections": "max-connections", "min_streams": "min-streams",
	}
	sourceField := aliases[field]
	if sourceField == "" {
		sourceField = field
	}
	sourceKey := sourceRoot + "." + sourceField
	return mappedOrigin(sourceKey, proxySourceValue(proxy, sourceKey), step, "extract")
}

func transportSource(proxy Proxy) string {
	for _, key := range []string{"http-opts", "h2-opts", "ws-opts", "grpc-opts"} {
		if proxy.Extra[key] != nil {
			return key
		}
	}
	return "network"
}

func proxySourceValue(proxy Proxy, path string) any {
	switch path {
	case "name":
		return proxy.Name
	case "type":
		return proxy.Type
	case "server":
		return proxy.Server
	case "port":
		return proxy.Port
	}
	parts := strings.Split(path, ".")
	var value any = proxy.Extra
	for _, part := range parts {
		object, ok := value.(map[string]any)
		if !ok {
			return nil
		}
		value = object[part]
	}
	return value
}

func mappedOrigin(source string, value any, step, transform string) FieldOrigin {
	return FieldOrigin{SourceKey: source, SourceValue: value, Step: step, Transform: transform}
}

func generatedOrigin(step, reason string) FieldOrigin {
	return FieldOrigin{Step: step, Transform: "generated", Reason: reason}
}
