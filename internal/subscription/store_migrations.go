package subscription

import (
	"fmt"
	"net"
	"strings"
)

func migrateLinuxRuntimeConfig(profile *Profile) {
	profile.TransparentProxy = defaultTransparentProxyConfig()
	shared := profile.DNS
	if nested, ok := shared["shared"].(map[string]any); ok {
		shared = nested
	}
	if value, ok := numberValue(shared["tproxyPort"]); ok && value > 0 {
		profile.TransparentProxy.TProxy.ListenPort = value
	}
	if value, ok := numberValue(shared["dnsListenPort"]); ok && value > 0 {
		profile.TransparentProxy.TProxy.DNSListenPort = value
	}
}

func migrateCoreConfiguration(profile *Profile, legacy legacyProfileConfiguration) {
	if profile.TransparentProxy.TUN.InterfaceName == "sing-box" {
		profile.TransparentProxy.TUN.InterfaceName = "sempre-tun"
	}
	if profile.CoreOverrides == nil {
		profile.CoreOverrides = map[string]map[string]any{}
	}
	if len(legacy.CustomConfig) > 0 && len(profile.CoreOverrides["sing-box"]) == 0 {
		profile.CoreOverrides["sing-box"] = cloneMap(legacy.CustomConfig)
	}
	if legacy.ClashAPI != nil && profile.ManagementAPI.ExternalController == "" && profile.ManagementAPI.Secret == "" && profile.ManagementAPI.ExternalUI == "" {
		profile.ManagementAPI = *legacy.ClashAPI
	}
	migrateLegacyDNSFields(profile)
}

func migrateRuntimeIntent(profile *Profile, legacy legacyTransparentProxyConfig) {
	if legacy.Mode != "" {
		profile.TransparentProxy.Mode = legacy.Mode
	}
	if legacy.TUN.InterfaceName != "" {
		profile.TransparentProxy.TUN.InterfaceName = legacy.TUN.InterfaceName
	}
	if legacy.TUN.Address != "" {
		profile.TransparentProxy.TUN.Address = legacy.TUN.Address
	}
	if legacy.TUN.RouteExcludeAddress != nil {
		profile.TransparentProxy.RouteExclusions = append([]string{}, legacy.TUN.RouteExcludeAddress...)
	}
	if legacy.TUN.InterfaceMode != "" {
		profile.TransparentProxy.InterfaceMode = legacy.TUN.InterfaceMode
	}
	if legacy.TUN.Interfaces != nil {
		profile.TransparentProxy.Interfaces = append([]string{}, legacy.TUN.Interfaces...)
	}
	profile.TransparentProxy.AutoExcludeLocalRoutes = legacy.TUN.AutoExcludeLocalRoutes
	profile.TransparentProxy.AutoExcludeVPNRoutes = legacy.TUN.AutoExcludeVPNRoutes
	if legacy.TProxy.ListenPort > 0 {
		profile.TransparentProxy.TProxy.ListenPort = legacy.TProxy.ListenPort
	}
	if legacy.TProxy.DNSListenPort > 0 {
		profile.TransparentProxy.TProxy.DNSListenPort = legacy.TProxy.DNSListenPort
	}
	profile.TransparentProxy.CaptureHost = legacy.TProxy.CaptureHost
	if legacy.TProxy.LANInterfaces != nil {
		profile.TransparentProxy.LANInterfaces = append([]string{}, legacy.TProxy.LANInterfaces...)
	}
	profile.LocalProxy = defaultLocalProxyConfig()
}

func migrateLegacyDNSFields(profile *Profile) {
	if profile.DNS == nil {
		return
	}
	shared := profile.DNS
	if nested, ok := objectValue(profile.DNS["shared"]); ok {
		shared = nested
	}
	if value, ok := numberValue(shared["tproxyPort"]); ok && value > 0 && profile.TransparentProxy.TProxy.ListenPort == 7893 {
		profile.TransparentProxy.TProxy.ListenPort = value
	}
	if value, ok := numberValue(shared["dnsListenPort"]); ok && value > 0 && profile.TransparentProxy.TProxy.DNSListenPort == 1053 {
		profile.TransparentProxy.TProxy.DNSListenPort = value
	}
	if secret := stringValue(shared["clashApiSecret"]); secret != "" && profile.ManagementAPI.Secret == "" {
		profile.ManagementAPI.Secret = secret
	}
	if ui := stringValue(shared["clashApiUiPath"]); ui != "" && profile.ManagementAPI.ExternalUI == "" {
		profile.ManagementAPI.ExternalUI = ui
	}
	if port, ok := numberValue(shared["clashApiPort"]); ok && port > 0 && profile.ManagementAPI.ExternalController == "" {
		profile.ManagementAPI.ExternalController = net.JoinHostPort("127.0.0.1", fmt.Sprint(port))
	}
	for _, key := range []string{"tproxyPort", "dnsListenPort", "clashApiPort", "clashApiSecret", "clashApiUiPath"} {
		delete(shared, key)
	}
	if overrides, ok := objectValue(profile.DNS["overrides"]); ok {
		migrated := cloneMap(overrides)
		modes, _ := objectValue(profile.DNS["modes"])
		modes = cloneMap(modes)
		if modes == nil {
			modes = map[string]any{}
		}
		if value, exists := overrides["sing_box_v11"]; exists {
			migrated["sing_box_v11"] = value
		} else if value, exists := overrides["singbox"]; exists {
			migrated["sing_box_v11"] = value
		}
		delete(migrated, "singbox")
		if value, exists := overrides["sing_box_v12"]; exists {
			migrated["sing_box_v12"] = value
		} else if value, exists := overrides["singboxV12"]; exists {
			migrated["sing_box_v12"] = value
		}
		delete(migrated, "singboxV12")
		if value, exists := overrides["mihomo"]; exists {
			migrated["mihomo"] = value
		} else if value, exists := overrides["clashMeta"]; exists {
			migrated["mihomo"] = value
		} else if value, exists := overrides["clash"]; exists {
			migrated["mihomo"] = value
		}
		delete(migrated, "clashMeta")
		delete(migrated, "clash")
		for _, key := range []string{"sing_box_v11", "sing_box_v12", "mihomo"} {
			if _, exists := migrated[key]; exists {
				if _, configured := modes[key]; !configured {
					modes[key] = "native"
				}
			}
		}
		if len(migrated) > 0 {
			profile.DNS["overrides"] = migrated
			profile.DNS["modes"] = modes
		} else {
			delete(profile.DNS, "overrides")
		}
	}
	if strings.TrimSpace(profile.Editor.DNSConfig) != "" {
		profile.Editor.DNSConfig = marshalEditorJSON(profile.DNS, "")
	}
}
