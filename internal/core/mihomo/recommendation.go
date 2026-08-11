package mihomo

import "github.com/tinymins/sempre/internal/core"

func (adapter *Adapter) AutoConfigCandidates(context core.AutoConfigContext) []core.AutoConfigCandidate {
	if err := validateTarget(context.Target); err != nil {
		return nil
	}
	score := 90
	warnings := []string{}
	if context.Target.OS == "darwin" {
		score = 70
		warnings = append(warnings, "not-verified-for-standalone-macos")
	}
	return []core.AutoConfigCandidate{{
		ID: "mihomo/stable", Reference: "mihomo@stable",
		ConfigurationMode: "mihomo-tun", Score: score,
		Reasons: []string{"stable-release", "broad-protocol-support"}, Warnings: warnings,
	}}
}

var _ core.AutoConfigProvider = (*Adapter)(nil)
