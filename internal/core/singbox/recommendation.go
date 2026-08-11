package singbox

import "github.com/tinymins/sempre/internal/core"

func (adapter *Adapter) AutoConfigCandidates(context core.AutoConfigContext) []core.AutoConfigCandidate {
	if !supportsAutoConfigTarget(context.Target) {
		return nil
	}
	if context.Target.OS == "darwin" {
		return []core.AutoConfigCandidate{
			{
				ID: "sing-box/macos-standalone-v12", Reference: "sing-box@1.12.20",
				ConfigurationMode: "macos-tun-real-ip", Score: 100,
				Reasons:  []string{"macos-standalone-compatible", "legacy-destination-override"},
				Warnings: []string{"legacy-core-version"},
			},
			{
				ID: "sing-box/macos-stable", Reference: "sing-box@stable",
				ConfigurationMode: "macos-tun-external-dns", Score: 55,
				Reasons:  []string{"stable-release", "broad-protocol-support"},
				Warnings: []string{"external-system-dns-required"},
			},
		}
	}
	return []core.AutoConfigCandidate{{
		ID: "sing-box/stable", Reference: "sing-box@stable",
		ConfigurationMode: "platform-tun", Score: 100,
		Reasons: []string{"stable-release", "broad-protocol-support"},
	}}
}

func supportsAutoConfigTarget(target core.Target) bool {
	if target.Arch != "amd64" && target.Arch != "arm64" {
		return false
	}
	return target.OS == "darwin" || target.OS == "linux" || target.OS == "windows"
}

var _ core.AutoConfigProvider = (*Adapter)(nil)
