package subscription

import (
	"fmt"
	"net"
	"strings"
)

type runtimeGroup struct {
	Name, Type, Default, URL string
	Members                  []string
	Interval                 int
}

type runtimeModel struct {
	Profile Profile
	Nodes   []Proxy
	Names   []string
	Groups  []runtimeGroup
	Final   string
	DNS     dnsShared
}

func newRuntimeModel(profile Profile, nodes []Proxy) (runtimeModel, error) {
	names := make([]string, 0, len(nodes))
	for _, node := range nodes {
		names = append(names, node.Name)
	}
	configured := profile.Groups
	if len(configured) == 0 {
		configured = []ProxyGroup{{Name: "proxy", Type: "select", IncludeAll: true}}
	}
	groups := make([]runtimeGroup, 0, len(configured))
	for _, group := range configured {
		if strings.TrimSpace(group.Name) == "" {
			return runtimeModel{}, fmt.Errorf("proxy group name is required")
		}
		members := append([]string{}, group.Proxies...)
		if !group.Readonly || group.IncludeAll || len(members) == 0 {
			members = appendUnique(members, names...)
		}
		if len(members) == 0 {
			return runtimeModel{}, fmt.Errorf("proxy group %q has no members", group.Name)
		}
		defaultMember := group.Default
		if defaultMember == "" {
			defaultMember = members[0]
		}
		if !configuredMember(members, defaultMember) {
			return runtimeModel{}, fmt.Errorf("proxy group %q default %q is not an available member", group.Name, group.Default)
		}
		groups = append(groups, runtimeGroup{
			Name: group.Name, Type: valueOr(group.Type, "select"), Default: defaultMember,
			URL: valueOr(group.URL, "https://www.gstatic.com/generate_204"), Members: members, Interval: group.Interval,
		})
	}
	return runtimeModel{Profile: profile, Nodes: nodes, Names: names, Groups: groups, Final: clashFinalGroup(configured), DNS: resolveDNSShared(profile.DNS)}, nil
}

func buildV2RayFamily(profile Profile, nodes []Proxy, coreID string) (map[string]any, []FieldDiff, []string, error) {
	model, err := newRuntimeModel(profile, nodes)
	if err != nil {
		return nil, nil, nil, err
	}
	outbounds := []any{
		map[string]any{"tag": "direct", "protocol": "freedom", "settings": map[string]any{"domainStrategy": "UseIP"}},
		map[string]any{"tag": "reject", "protocol": "blackhole", "settings": map[string]any{}},
		map[string]any{"tag": "dns-out", "protocol": "dns", "settings": map[string]any{}},
	}
	diffs := make([]FieldDiff, 0, len(nodes))
	warnings := []string{}
	represented := map[string]bool{}
	for _, node := range nodes {
		outbound, diff, ok := v2RayOutbound(node, coreID == "xray")
		diffs = append(diffs, diff)
		if !ok {
			warnings = append(warnings, node.Name+": unsupported proxy type "+node.Type)
			continue
		}
		outbounds = append(outbounds, outbound)
		represented[node.Name] = true
	}
	if representedNodeCount(diffs) == 0 {
		return nil, diffs, warnings, fmt.Errorf("no nodes can be represented by %s", coreID)
	}
	for index := range model.Groups {
		group := &model.Groups[index]
		members := group.Members[:0]
		for _, member := range group.Members {
			if represented[member] {
				members = append(members, member)
			}
		}
		if len(members) == 0 {
			return nil, diffs, warnings, fmt.Errorf("proxy group %q has no members supported by %s", group.Name, coreID)
		}
		group.Members = members
		if !represented[group.Default] {
			group.Default = members[0]
		}
	}
	routing, routingWarnings := v2RayRouting(model)
	warnings = append(warnings, routingWarnings...)
	dns := v2RayDNS(model)
	if override := coreDNSOverride(profile.DNS, coreID); override != nil {
		dns = override
	}
	document := map[string]any{
		"log":       v2RayLog(profile.LogLevel, coreID == "xray"),
		"dns":       dns,
		"inbounds":  v2RayInbounds(model, coreID),
		"outbounds": outbounds,
		"routing":   routing,
		"policy": map[string]any{"system": map[string]any{
			"statsInboundUplink": true, "statsInboundDownlink": true,
			"statsOutboundUplink": true, "statsOutboundDownlink": true,
		}},
		"stats": map[string]any{},
	}
	if observatory := v2RayObservatory(model); observatory != nil {
		document["observatory"] = observatory
	}
	if override := profile.CoreOverrides[coreID]; len(override) > 0 {
		if err := validateV2RayOverrides(profile, coreID); err != nil {
			return nil, diffs, warnings, err
		}
		deepMerge(document, override)
	}
	return document, diffs, warnings, nil
}

func v2RayOutbound(proxy Proxy, modern bool) (map[string]any, FieldDiff, bool) {
	diff := FieldDiff{Node: proxy.Name, Consumed: []string{}, Ignored: []string{}, Dropped: []string{}, Warnings: []string{}, FieldOrigins: map[string]FieldOrigin{}}
	if _, reality := objectValue(proxy.Extra["reality-opts"]); reality && !modern {
		diff.Warnings = append(diff.Warnings, "Reality is not supported by V2Ray-core")
		return nil, diff, false
	}
	stringField := func(key string) string { return stringValue(proxy.Extra[key]) }
	settings := map[string]any{}
	protocol := proxy.Type
	if protocol == "ss" {
		protocol = "shadowsocks"
	}
	if modern {
		switch proxy.Type {
		case "vmess":
			settings = map[string]any{"address": proxy.Server, "port": proxy.Port, "id": stringField("uuid"), "security": valueOr(stringField("cipher"), "auto")}
		case "vless":
			settings = map[string]any{"address": proxy.Server, "port": proxy.Port, "id": stringField("uuid"), "encryption": "none"}
			if flow := stringField("flow"); flow != "" {
				settings["flow"] = flow
			}
		case "trojan":
			settings = map[string]any{"address": proxy.Server, "port": proxy.Port, "password": stringField("password")}
		case "ss":
			settings = map[string]any{"address": proxy.Server, "port": proxy.Port, "method": valueOr(stringField("cipher"), "aes-256-gcm"), "password": stringField("password")}
		case "socks5", "http":
			settings = map[string]any{"address": proxy.Server, "port": proxy.Port, "user": stringField("username"), "pass": stringField("password")}
			if proxy.Type == "socks5" {
				protocol = "socks"
			}
		default:
			diff.Warnings = append(diff.Warnings, "unsupported proxy type "+proxy.Type)
			return nil, diff, false
		}
	} else {
		server := map[string]any{"address": proxy.Server, "port": proxy.Port}
		switch proxy.Type {
		case "vmess":
			server["users"] = []any{map[string]any{"id": stringField("uuid"), "alterId": integer(proxy.Extra["alterId"]), "security": valueOr(stringField("cipher"), "auto")}}
			settings["vnext"] = []any{server}
		case "vless":
			user := map[string]any{"id": stringField("uuid"), "encryption": "none"}
			if flow := stringField("flow"); flow != "" {
				user["flow"] = flow
			}
			server["users"] = []any{user}
			settings["vnext"] = []any{server}
		case "trojan":
			server["password"] = stringField("password")
			settings["servers"] = []any{server}
		case "ss":
			server["method"], server["password"] = valueOr(stringField("cipher"), "aes-256-gcm"), stringField("password")
			settings["servers"] = []any{server}
		case "socks5", "http":
			if username := stringField("username"); username != "" {
				server["users"] = []any{map[string]any{"user": username, "pass": stringField("password")}}
			}
			settings["servers"] = []any{server}
			if proxy.Type == "socks5" {
				protocol = "socks"
			}
		default:
			diff.Warnings = append(diff.Warnings, "unsupported proxy type "+proxy.Type)
			return nil, diff, false
		}
	}
	outbound := map[string]any{"tag": proxy.Name, "protocol": protocol, "settings": settings}
	if stream := v2RayStream(proxy, modern); stream != nil {
		outbound["streamSettings"] = stream
	}
	diff.Consumed = sortedKeys(proxy.Extra)
	diff.Outbound = outbound
	diff.FieldOrigins = buildFieldOrigins(proxy, outbound)
	return outbound, diff, true
}

func v2RayStream(proxy Proxy, modern bool) map[string]any {
	stream := map[string]any{}
	network := valueOr(stringValue(proxy.Extra["network"]), "tcp")
	if _, ok := objectValue(proxy.Extra["ws-opts"]); ok {
		network = "ws"
	}
	if _, ok := objectValue(proxy.Extra["grpc-opts"]); ok {
		network = "grpc"
	}
	if _, ok := objectValue(proxy.Extra["http-opts"]); ok {
		network = "http"
	}
	if modern {
		method := map[string]string{"tcp": "raw", "ws": "websocket", "http": "xhttp", "h2": "xhttp"}[network]
		if method == "" {
			method = network
		}
		stream["method"] = method
	} else {
		stream["network"] = map[string]string{"http": "h2"}[network]
		if stream["network"] == "" {
			stream["network"] = network
		}
	}
	if options, ok := objectValue(proxy.Extra["ws-opts"]); ok {
		settings := map[string]any{"path": valueOr(stringValue(options["path"]), "/")}
		if headers, exists := objectValue(options["headers"]); exists {
			settings["headers"] = headers
		}
		stream["wsSettings"] = settings
	}
	if options, ok := objectValue(proxy.Extra["grpc-opts"]); ok {
		stream["grpcSettings"] = map[string]any{"serviceName": stringValue(options["grpc-service-name"])}
	}
	if options, ok := objectValue(proxy.Extra["http-opts"]); ok {
		settings := map[string]any{"path": valueOr(firstListString(options["path"]), "/")}
		if modern {
			stream["xhttpSettings"] = settings
		} else {
			stream["httpSettings"] = settings
		}
	}
	security := "none"
	if _, reality := objectValue(proxy.Extra["reality-opts"]); reality && modern {
		security = "reality"
	} else if tls, _ := proxy.Extra["tls"].(bool); tls || proxy.Type == "trojan" {
		security = "tls"
	}
	stream["security"] = security
	if security == "tls" {
		settings := map[string]any{"serverName": serverName(proxy), "allowInsecure": boolValue(proxy.Extra["skip-cert-verify"])}
		if alpn := proxy.Extra["alpn"]; alpn != nil {
			settings["alpn"] = alpn
		}
		if fingerprint := stringValue(proxy.Extra["client-fingerprint"]); fingerprint != "" && modern {
			settings["fingerprint"] = fingerprint
		}
		stream["tlsSettings"] = settings
	}
	if security == "reality" {
		reality, _ := objectValue(proxy.Extra["reality-opts"])
		stream["realitySettings"] = map[string]any{
			"serverName": serverName(proxy), "fingerprint": valueOr(stringValue(proxy.Extra["client-fingerprint"]), "chrome"),
			"password": stringValue(reality["public-key"]), "shortId": stringValue(reality["short-id"]), "spiderX": "/",
		}
	}
	return stream
}

func v2RayInbounds(model runtimeModel, coreID string) []any {
	result := []any{}
	local := model.Profile.LocalProxy
	socksUsersKey, httpUsersKey := "accounts", "accounts"
	if coreID == "xray" {
		socksUsersKey, httpUsersKey = "users", "users"
	}
	result = append(result, map[string]any{
		"tag": "sempre-socks-in", "listen": "127.0.0.1", "port": local.SOCKSPort, "protocol": "socks",
		"settings": map[string]any{"auth": "password", socksUsersKey: []any{map[string]any{"user": local.Username, "pass": local.Password}}, "udp": true, "ip": "127.0.0.1"},
	}, map[string]any{
		"tag": "sempre-http-in", "listen": "127.0.0.1", "port": local.HTTPPort, "protocol": "http",
		"settings": map[string]any{httpUsersKey: []any{map[string]any{"user": local.Username, "pass": local.Password}}},
	})
	transparent := model.Profile.TransparentProxy
	switch transparent.Mode {
	case TransparentProxyTUN:
		if coreID == "xray" {
			gateway := valueOr(transparent.TUN.Address, "172.19.0.1/30")
			result = append(result, map[string]any{
				"tag": "tun-in", "protocol": "tun", "settings": map[string]any{
					"name": transparent.TUN.InterfaceName, "mtu": 9000, "gateway": []string{gateway},
					"autoSystemRoutingTable": []string{"0.0.0.0/0", "::/0"}, "autoOutboundsInterface": "auto",
				}, "sniffing": map[string]any{"enabled": true, "destOverride": []string{"http", "tls", "quic"}},
			})
		}
	case TransparentProxyTProxy:
		result = append(result,
			map[string]any{"tag": "tproxy-in", "listen": "0.0.0.0", "port": transparent.TProxy.ListenPort, "protocol": "dokodemo-door", "settings": map[string]any{"network": "tcp,udp", "followRedirect": true}, "streamSettings": map[string]any{"sockopt": map[string]any{"tproxy": "tproxy"}}, "sniffing": map[string]any{"enabled": true, "destOverride": []string{"http", "tls", "quic"}}},
			map[string]any{"tag": "dns-in", "listen": "0.0.0.0", "port": transparent.TProxy.DNSListenPort, "protocol": "dokodemo-door", "settings": map[string]any{"address": model.DNS.RemoteDNS, "port": 53, "network": "tcp,udp"}},
		)
	}
	return result
}

func v2RayDNS(model runtimeModel) map[string]any {
	bootstrapDomains := []string{}
	for _, node := range model.Nodes {
		if net.ParseIP(node.Server) == nil {
			bootstrapDomains = append(bootstrapDomains, "full:"+node.Server)
		}
	}
	local := map[string]any{"address": model.DNS.LocalDNS, "port": model.DNS.LocalDNSPort, "domains": []string{"geosite:cn"}, "expectedIPs": []string{"geoip:cn"}, "skipFallback": true, "tag": "local-dns"}
	bootstrap := map[string]any{"address": model.DNS.BootstrapDNS, "port": 53, "domains": bootstrapDomains, "skipFallback": true, "tag": "bootstrap-dns"}
	remoteHost := net.JoinHostPort(model.DNS.RemoteServerName, "443")
	remote := map[string]any{"address": "https://" + remoteHost + "/dns-query", "tag": "remote-dns", "finalQuery": true}
	return map[string]any{
		"hosts":   map[string]any{model.DNS.RemoteServerName: model.DNS.RemoteDNS, model.DNS.BootstrapServerName: model.DNS.BootstrapDNS},
		"servers": []any{bootstrap, local, remote}, "queryStrategy": map[bool]string{true: "UseIPv4", false: "UseIP"}[model.DNS.PreferIPv4],
		"disableFallbackIfMatch": true, "tag": "remote-dns",
	}
}

func v2RayRouting(model runtimeModel) (map[string]any, []string) {
	groupNames := map[string]bool{}
	balancers := []any{}
	for _, group := range model.Groups {
		groupNames[group.Name] = true
		members := []string{group.Default}
		strategy := "random"
		if group.Type == "url-test" {
			members = append([]string{}, group.Members...)
			strategy = "leastPing"
		}
		balancers = append(balancers, map[string]any{"tag": group.Name, "selector": appendUnique([]string{}, members...), "strategy": map[string]any{"type": strategy}})
	}
	rules := []any{
		map[string]any{"type": "field", "inboundTag": []string{"dns-in"}, "outboundTag": "dns-out"},
		map[string]any{"type": "field", "inboundTag": []string{"local-dns", "bootstrap-dns"}, "outboundTag": "direct"},
		v2RayRouteTarget(map[string]any{"type": "field", "inboundTag": []string{"remote-dns"}}, model.Final, groupNames),
		map[string]any{"type": "field", "ip": []string{"geoip:private", "geoip:cn"}, "outboundTag": "direct"},
		map[string]any{"type": "field", "domain": []string{"geosite:cn"}, "outboundTag": "direct"},
	}
	warnings := []string{}
	for _, line := range model.Profile.Rules {
		if rule, target, ok := v2RayRule(line); ok {
			rules = append(rules, v2RayRouteTarget(rule, target, groupNames))
		} else {
			warnings = append(warnings, "unsupported rule: "+line)
		}
	}
	for _, provider := range model.Profile.RuleProviders {
		warnings = append(warnings, "rule provider "+provider.Tag+" is not representable by the "+"v2ray-family renderer")
	}
	rules = append(rules, v2RayRouteTarget(map[string]any{"type": "field", "network": "tcp,udp"}, model.Final, groupNames))
	return map[string]any{"domainStrategy": "IPIfNonMatch", "domainMatcher": "hybrid", "rules": rules, "balancers": balancers}, warnings
}

func v2RayRule(line string) (map[string]any, string, bool) {
	parts := strings.Split(line, ",")
	if len(parts) < 2 {
		return nil, "", false
	}
	for index := range parts {
		parts[index] = strings.TrimSpace(parts[index])
	}
	target := parts[len(parts)-1]
	rule := map[string]any{"type": "field"}
	switch strings.ToUpper(parts[0]) {
	case "DOMAIN":
		rule["domain"] = []string{"full:" + parts[1]}
	case "DOMAIN-SUFFIX":
		rule["domain"] = []string{"domain:" + parts[1]}
	case "DOMAIN-KEYWORD":
		rule["domain"] = []string{"keyword:" + parts[1]}
	case "GEOSITE":
		rule["domain"] = []string{"geosite:" + strings.ToLower(parts[1])}
	case "GEOIP":
		rule["ip"] = []string{"geoip:" + strings.ToLower(parts[1])}
	case "IP-CIDR", "IP-CIDR6":
		rule["ip"] = []string{parts[1]}
	case "SRC-IP-CIDR":
		rule["source"] = []string{parts[1]}
	case "DST-PORT":
		rule["port"] = parts[1]
	case "NETWORK":
		rule["network"] = strings.ToLower(parts[1])
	case "MATCH":
		return nil, target, false
	default:
		return nil, target, false
	}
	return rule, target, true
}

func v2RayRouteTarget(rule map[string]any, target string, groupNames map[string]bool) map[string]any {
	target = normalizeOutboundName(target)
	if groupNames[target] {
		rule["balancerTag"] = target
	} else if target == "reject" {
		rule["outboundTag"] = "reject"
	} else {
		rule["outboundTag"] = target
	}
	return rule
}

func v2RayObservatory(model runtimeModel) map[string]any {
	selectors := []string{}
	interval := 300
	for _, group := range model.Groups {
		if group.Type == "url-test" {
			selectors = append(selectors, group.Members...)
			if group.Interval > 0 && group.Interval < interval {
				interval = group.Interval
			}
		}
	}
	if len(selectors) == 0 {
		return nil
	}
	return map[string]any{"subjectSelector": appendUnique([]string{}, selectors...), "probeURL": "https://www.gstatic.com/generate_204", "probeInterval": fmt.Sprintf("%ds", interval), "enableConcurrency": true}
}

func v2RayLog(level string, modern bool) map[string]any {
	if level == "off" {
		level = "none"
	}
	if level != "none" && level != "error" && level != "warning" && level != "warn" && level != "info" && level != "debug" {
		level = "warning"
	}
	if level == "warn" {
		level = "warning"
	}
	result := map[string]any{"loglevel": level}
	if modern {
		result["dnsLog"] = level == "debug"
	}
	return result
}

func validateV2RayOverrides(profile Profile, coreID string) error {
	override := profile.CoreOverrides[coreID]
	if _, exists := override["api"]; exists {
		return fmt.Errorf("top-level api is managed by Sempre's internal core control")
	}
	if _, exists := override["inbounds"]; exists {
		return fmt.Errorf("top-level inbounds are managed by Sempre's authenticated local proxy")
	}
	return nil
}

func serverName(proxy Proxy) string {
	return valueOr(stringValue(proxy.Extra["servername"]), valueOr(stringValue(proxy.Extra["sni"]), proxy.Server))
}

func firstListString(value any) string {
	if text := stringValue(value); text != "" {
		return text
	}
	values, _ := value.([]any)
	if len(values) > 0 {
		return stringValue(values[0])
	}
	return ""
}

func boolValue(value any) bool {
	result, _ := value.(bool)
	return result
}
