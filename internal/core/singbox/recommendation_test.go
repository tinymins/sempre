package singbox

import (
	"testing"

	"github.com/tinymins/sempre/internal/core"
)

func TestAutoConfigCandidatesPreferStandaloneMacOSCompatibility(t *testing.T) {
	candidates := New().AutoConfigCandidates(core.AutoConfigContext{Target: core.Target{OS: "darwin", Arch: "arm64"}})
	if len(candidates) != 2 {
		t.Fatalf("candidates = %#v", candidates)
	}
	if candidates[0].Reference != "sing-box@1.12.20" || candidates[0].ConfigurationMode != "macos-tun-real-ip" || candidates[0].Score <= candidates[1].Score {
		t.Fatalf("macOS recommendation order = %#v", candidates)
	}
}

func TestAutoConfigCandidatesUseStableOnLinux(t *testing.T) {
	candidates := New().AutoConfigCandidates(core.AutoConfigContext{Target: core.Target{OS: "linux", Arch: "amd64"}})
	if len(candidates) != 1 || candidates[0].Reference != "sing-box@stable" {
		t.Fatalf("Linux candidates = %#v", candidates)
	}
}
