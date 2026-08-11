package mihomo

import (
	"testing"

	"github.com/tinymins/sempre/internal/core"
)

func TestAutoConfigCandidatesRegisterStableCore(t *testing.T) {
	candidates := New().AutoConfigCandidates(core.AutoConfigContext{Target: core.Target{OS: "darwin", Arch: "arm64"}})
	if len(candidates) != 1 || candidates[0].Reference != "mihomo@stable" || candidates[0].Score >= 100 {
		t.Fatalf("macOS candidates = %#v", candidates)
	}
	if unsupported := New().AutoConfigCandidates(core.AutoConfigContext{Target: core.Target{OS: "freebsd", Arch: "amd64"}}); len(unsupported) != 0 {
		t.Fatalf("unsupported candidates = %#v", unsupported)
	}
}
