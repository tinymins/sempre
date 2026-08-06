package subscription

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net"
	"net/url"
	"strconv"
	"strings"
)

func buildDae(profile Profile, nodes []Proxy) (string, []FieldDiff, []string, error) {
	model, err := newRuntimeModel(profile, nodes)
	if err != nil {
		return "", nil, nil, err
	}
	links := []string{}
	diffs := []FieldDiff{}
	warnings := []string{}
	represented := map[string]bool{}
	for _, node := range nodes {
		link, ok := daeNodeURI(node)
		diff := FieldDiff{Node: node.Name, Consumed: sortedKeys(node.Extra), Ignored: []string{}, Dropped: []string{}, Warnings: []string{}, FieldOrigins: map[string]FieldOrigin{}}
		if !ok {
			diff.Warnings = append(diff.Warnings, "unsupported proxy type "+node.Type)
			warnings = append(warnings, node.Name+": unsupported proxy type "+node.Type)
			diffs = append(diffs, diff)
			continue
		}
		diff.Outbound = map[string]any{"type": "dae-node", "tag": node.Name, "uri": link}
		diffs = append(diffs, diff)
		links = append(links, strconv.Quote(link))
		represented[node.Name] = true
	}
	if len(links) == 0 {
		return "", diffs, warnings, fmt.Errorf("no nodes can be represented by dae")
	}
	groupTags := map[string]string{}
	groupBlocks := []string{}
	for index, group := range model.Groups {
		tag := fmt.Sprintf("sempre_group_%d", index+1)
		groupTags[group.Name] = tag
		members := []string{}
		defaultIndex := 0
		for _, member := range group.Members {
			if !represented[member] {
				continue
			}
			if member == group.Default {
				defaultIndex = len(members)
			}
			members = append(members, strconv.Quote(member))
		}
		if len(members) == 0 {
			continue
		}
		policy := fmt.Sprintf("fixed(%d)", defaultIndex)
		if group.Type == "url-test" {
			policy = "min_moving_avg"
		}
		groupBlocks = append(groupBlocks, fmt.Sprintf("  %s {\n    filter: name(%s)\n    policy: %s\n  }", tag, strings.Join(members, ", "), policy))
	}
	final := groupTags[model.Final]
	if final == "" {
		return "", diffs, warnings, fmt.Errorf("dae final proxy group %q has no represented members", model.Final)
	}
	dns := model.DNS
	localAddress := net.JoinHostPort(dns.LocalDNS, strconv.Itoa(dns.LocalDNSPort))
	remoteAddress := net.JoinHostPort(dns.RemoteDNS, strconv.Itoa(dns.RemoteDNSPort))
	bootstrap := net.JoinHostPort(dns.BootstrapDNS, "53")
	globalInterfaces := ""
	if profile.TransparentProxy.Mode == TransparentProxyEBPF {
		if len(profile.TransparentProxy.LANInterfaces) > 0 {
			globalInterfaces += "\n  lan_interface: " + strings.Join(profile.TransparentProxy.LANInterfaces, ",")
		}
		wan := valueOr(profile.TransparentProxy.EBPF.WANInterface, "auto")
		globalInterfaces += "\n  wan_interface: " + wan
	}
	routingRules := []string{
		"  dip(geoip:private) -> direct",
		"  dip(geoip:cn) -> direct",
		"  domain(geosite:cn) -> direct",
	}
	for _, line := range profile.Rules {
		if converted, ok := daeRule(line, groupTags); ok {
			routingRules = append(routingRules, "  "+converted)
		} else {
			warnings = append(warnings, "unsupported rule: "+line)
		}
	}
	for _, provider := range profile.RuleProviders {
		warnings = append(warnings, "rule provider "+provider.Tag+" is not representable by dae")
	}
	level := profile.LogLevel
	if level == "off" {
		level = "error"
	}
	config := fmt.Sprintf(`global {
  tproxy_port: 12345
  tproxy_port_protect: true
  log_level: %s
  auto_config_kernel_parameter: %t
  bootstrap_resolver: %s%s
}
node {
  %s
}
dns {
  ipversion_prefer: %d
  upstream {
    local: %s
    remote: %s
  }
  routing {
    request {
      qname(geosite:cn) -> local
      fallback: remote
    }
  }
}
group {
%s
}
routing {
%s
  fallback: %s
}
`, level, profile.TransparentProxy.EBPF.AutoConfigKernelParameter, strconv.Quote(bootstrap), globalInterfaces,
		strings.Join(links, "\n  "), map[bool]int{true: 4, false: 0}[dns.PreferIPv4],
		strconv.Quote("udp://"+localAddress), strconv.Quote("tls://"+remoteAddress), strings.Join(groupBlocks, "\n"), strings.Join(routingRules, "\n"), final)
	if appendConfig := stringValue(profile.CoreOverrides["dae"]["append"]); appendConfig != "" {
		config += "\n" + appendConfig + "\n"
	}
	return config, diffs, warnings, nil
}

func daeNodeURI(proxy Proxy) (string, bool) {
	name := url.QueryEscape(proxy.Name)
	host := net.JoinHostPort(proxy.Server, strconv.Itoa(proxy.Port))
	field := func(key string) string { return stringValue(proxy.Extra[key]) }
	query := url.Values{}
	setTransportQuery(query, proxy)
	switch proxy.Type {
	case "vless":
		security := "none"
		if _, ok := objectValue(proxy.Extra["reality-opts"]); ok {
			security = "reality"
		} else if boolValue(proxy.Extra["tls"]) {
			security = "tls"
		}
		query.Set("security", security)
		if flow := field("flow"); flow != "" {
			query.Set("flow", flow)
		}
		return "vless://" + url.QueryEscape(field("uuid")) + "@" + host + "?" + query.Encode() + "#" + name, true
	case "trojan":
		return "trojan://" + url.QueryEscape(field("password")) + "@" + host + "?" + query.Encode() + "#" + name, true
	case "vmess":
		value := map[string]any{"v": "2", "ps": proxy.Name, "add": proxy.Server, "port": strconv.Itoa(proxy.Port), "id": field("uuid"), "aid": integer(proxy.Extra["alterId"]), "scy": valueOr(field("cipher"), "auto"), "net": valueOr(field("network"), "tcp")}
		if boolValue(proxy.Extra["tls"]) {
			value["tls"] = "tls"
		}
		value["sni"] = serverName(proxy)
		if options, ok := objectValue(proxy.Extra["ws-opts"]); ok {
			value["net"], value["path"] = "ws", stringValue(options["path"])
		}
		if options, ok := objectValue(proxy.Extra["grpc-opts"]); ok {
			value["net"], value["path"] = "grpc", stringValue(options["grpc-service-name"])
		}
		data, _ := json.Marshal(value)
		return "vmess://" + base64.RawStdEncoding.EncodeToString(data), true
	case "ss":
		credential := base64.RawURLEncoding.EncodeToString([]byte(valueOr(field("cipher"), "aes-256-gcm") + ":" + field("password")))
		return "ss://" + credential + "@" + host + "#" + name, true
	case "socks5", "http":
		user := ""
		if field("username") != "" {
			user = url.UserPassword(field("username"), field("password")).String() + "@"
		}
		scheme := proxy.Type
		if proxy.Type == "http" && boolValue(proxy.Extra["tls"]) {
			scheme = "https"
		}
		return scheme + "://" + user + host + "/#" + name, true
	case "hysteria2", "anytls":
		return proxy.Type + "://" + url.QueryEscape(field("password")) + "@" + host + "?" + query.Encode() + "#" + name, true
	case "tuic":
		return "tuic://" + url.QueryEscape(field("uuid")) + ":" + url.QueryEscape(field("password")) + "@" + host + "?" + query.Encode() + "#" + name, true
	default:
		return "", false
	}
}

func setTransportQuery(query url.Values, proxy Proxy) {
	network := stringValue(proxy.Extra["network"])
	if options, ok := objectValue(proxy.Extra["ws-opts"]); ok {
		network = "ws"
		query.Set("path", stringValue(options["path"]))
		if headers, ok := objectValue(options["headers"]); ok {
			query.Set("host", stringValue(headers["Host"]))
		}
	}
	if options, ok := objectValue(proxy.Extra["grpc-opts"]); ok {
		network = "grpc"
		query.Set("serviceName", stringValue(options["grpc-service-name"]))
	}
	if network != "" {
		query.Set("type", network)
	}
	if name := serverName(proxy); name != "" {
		query.Set("sni", name)
	}
	if boolValue(proxy.Extra["skip-cert-verify"]) {
		query.Set("insecure", "1")
	}
	if fingerprint := stringValue(proxy.Extra["client-fingerprint"]); fingerprint != "" {
		query.Set("fp", fingerprint)
	}
	if reality, ok := objectValue(proxy.Extra["reality-opts"]); ok {
		query.Set("pbk", stringValue(reality["public-key"]))
		query.Set("sid", stringValue(reality["short-id"]))
	}
}

func daeRule(line string, groupTags map[string]string) (string, bool) {
	parts := strings.Split(line, ",")
	if len(parts) < 2 {
		return "", false
	}
	for index := range parts {
		parts[index] = strings.TrimSpace(parts[index])
	}
	target := normalizeOutboundName(parts[len(parts)-1])
	if groupTags[target] != "" {
		target = groupTags[target]
	}
	matcher := ""
	switch strings.ToUpper(parts[0]) {
	case "DOMAIN":
		matcher = "domain(full: " + parts[1] + ")"
	case "DOMAIN-SUFFIX":
		matcher = "domain(suffix: " + parts[1] + ")"
	case "DOMAIN-KEYWORD":
		matcher = "domain(keyword: " + parts[1] + ")"
	case "GEOSITE":
		matcher = "domain(geosite:" + strings.ToLower(parts[1]) + ")"
	case "GEOIP", "IP-CIDR", "IP-CIDR6":
		value := parts[1]
		if strings.EqualFold(parts[0], "GEOIP") {
			value = "geoip:" + strings.ToLower(value)
		}
		matcher = "dip(" + value + ")"
	case "SRC-IP-CIDR":
		matcher = "sip(" + parts[1] + ")"
	case "DST-PORT":
		matcher = "dport(" + parts[1] + ")"
	case "NETWORK":
		matcher = "l4proto(" + strings.ToLower(parts[1]) + ")"
	default:
		return "", false
	}
	return matcher + " -> " + target, true
}
