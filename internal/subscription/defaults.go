package subscription

type Defaults struct {
	Groups        []ProxyGroup   `json:"groups"`
	RuleProviders []RuleProvider `json:"rule_providers"`
	Filters       []string       `json:"filters"`
	Rules         []string       `json:"rules"`
	DNS           map[string]any `json:"dns"`
}

func SystemDefaults() Defaults {
	direct := "🚀 直接连接"
	foreign := "🔰 国外流量"
	return Defaults{
		Groups: []ProxyGroup{
			{Name: foreign, Type: "select", Proxies: []string{direct}, IncludeAll: true},
			{Name: "🏳️‍🌈 Google", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "✈️ Telegram", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "🎬 Youtube", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "🎬 TikTok", Type: "select", Proxies: []string{direct, foreign}, IncludeAll: true},
			{Name: "🎬 Netflix", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "🎬 PTTracker", Type: "select", Proxies: []string{direct, foreign}, IncludeAll: true},
			{Name: "👽 Reddit", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "🍎 苹果APNs", Type: "select", Proxies: []string{direct, foreign}, IncludeAll: true},
			{Name: "🍎 苹果服务", Type: "select", Proxies: []string{direct, foreign}, IncludeAll: true},
			{Name: "🪟 Microsoft", Type: "select", Proxies: []string{direct, foreign}, IncludeAll: true},
			{Name: "🎮 Steam", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "🎮 SteamContent", Type: "select", Proxies: []string{direct, foreign}, IncludeAll: true},
			{Name: "🎮 SeasunGame", Type: "select", Proxies: []string{direct, foreign}, IncludeAll: true},
			{Name: "🎮 Discord", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "🤖 ChatGPT-IOS", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "🤖 AI", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "🐙 GitHub", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "🪙 Crypto", Type: "select", Proxies: []string{foreign, direct}, IncludeAll: true},
			{Name: "🛡️ 正版验证拦截", Type: "select", Proxies: []string{"REJECT", direct, foreign}, IncludeAll: true},
			{Name: "🧹 秋风广告规则 AWAvenue", Type: "select", Proxies: []string{direct, foreign, "REJECT"}, IncludeAll: true},
			{Name: direct, Type: "select", Proxies: []string{"DIRECT"}, Readonly: true},
			{Name: "💊 广告合集", Type: "select", Proxies: []string{"DIRECT", "REJECT"}, Readonly: true},
			{Name: "⚓️ 其他流量", Type: "select", Proxies: []string{foreign, direct}, Readonly: true},
		},
		RuleProviders: []RuleProvider{
			{Tag: "AppleApns", Outbound: "🍎 苹果APNs", URL: "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/AppleAPNs.yaml"},
			{Tag: "Apple", Outbound: "🍎 苹果服务", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Apple.yaml"},
			{Tag: "AppleTV", Outbound: "🍎 苹果服务", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Media/Apple%20TV.yaml"},
			{Tag: "AppleMusic", Outbound: "🍎 苹果服务", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Media/Apple%20Music.yaml"},
			{Tag: "Microsoft", Outbound: "🪟 Microsoft", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Microsoft.yaml"},
			{Tag: "Reddit", Outbound: "👽 Reddit", URL: "https://raw.githubusercontent.com/blackmatrix7/ios_rule_script/refs/heads/master/rule/Clash/Reddit/Reddit_No_Resolve.yaml"},
			{Tag: "ChatGPT-IOS", Outbound: "🤖 ChatGPT-IOS", URL: "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/chatgpt-ios.yaml"},
			{Tag: "AI", Outbound: "🤖 AI", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/AI%20Suite.yaml"},
			{Tag: "GitHub", Outbound: "🐙 GitHub", URL: "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/github.yaml"},
			{Tag: "Crypto", Outbound: "🪙 Crypto", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Crypto.yaml"},
			{Tag: "Youtube", Outbound: "🎬 Youtube", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Media/YouTube.yaml"},
			{Tag: "TikTok", Outbound: "🎬 TikTok", URL: "https://raw.githubusercontent.com/Z-Siqi/Clash-for-Windows_Rule/refs/heads/main/Rule/TikTok"},
			{Tag: "Netflix", Outbound: "🎬 Netflix", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Media/Netflix.yaml"},
			{Tag: "PTTracker", Outbound: "🎬 PTTracker", URL: "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/PTTracker.yaml"},
			{Tag: "Steam", Outbound: "🎮 Steam", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Steam.yaml"},
			{Tag: "SteamContent", Outbound: "🎮 SteamContent", URL: "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/SteamContent.yaml"},
			{Tag: "SeasunGame", Outbound: "🎮 SeasunGame", URL: "https://raw.githubusercontent.com/ohmywrt/clash-rule/refs/heads/master/SeasunGame.yaml"},
			{Tag: "Discord", Outbound: "🎮 Discord", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Discord.yaml"},
			{Tag: "Telegram", Outbound: "✈️ Telegram", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/Telegram.yaml"},
			{Tag: "GoogleCIDRv2", Outbound: "🏳️‍🌈 Google", URL: "https://vercel.williamchan.me/api/google-ips"},
			{Tag: "a.dove.is.dumb", Outbound: "🛡️ 正版验证拦截", URL: "https://raw.githubusercontent.com/ignaciocastro/a-dove-is-dumb/main/clash.yaml"},
			{Tag: "AWAvenueAD", Outbound: "🧹 秋风广告规则 AWAvenue", URL: "https://raw.githubusercontent.com/TG-Twilight/AWAvenue-Ads-Rule/main/Filters/AWAvenue-Ads-Rule-Clash.yaml"},
			{Tag: "AD", Outbound: "💊 广告合集", URL: "https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/AdBlock.yaml"},
		},
		Filters: []string{"官网", "客服", "qq群"},
		Rules:   []string{},
		DNS: map[string]any{"shared": map[string]any{
			"localDns": "127.0.0.1", "localDnsPort": 53,
			"fakeipIpv4Range": "198.18.0.0/15", "fakeipIpv6Range": "fc00::/18",
			"fakeipEnabled": true, "fakeipTtl": 300,
			"dnsListenPort": 1053, "tproxyPort": 7893,
			"rejectHttps": true, "cnDomainLocalDns": true,
		}},
	}
}

func EffectiveProfile(profile Profile) Profile {
	defaults := SystemDefaults()
	if profile.UseSystemGroups {
		profile.Groups = defaults.Groups
	}
	if profile.UseSystemRules {
		profile.RuleProviders = defaults.RuleProviders
	}
	if profile.UseSystemFilters {
		profile.Filters = defaults.Filters
	}
	if profile.UseSystemDNS {
		profile.DNS = defaults.DNS
	}
	if profile.UseSystemCustomConfig {
		profile.Rules = defaults.Rules
	}
	return profile
}
