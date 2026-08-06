package clashrs

import (
	"testing"

	"github.com/tinymins/sempre/internal/core"
)

func TestOfficialAssetNames(t *testing.T) {
	t.Parallel()
	tests := []struct {
		target core.Target
		name   string
	}{
		{core.Target{OS: "linux", Arch: "amd64"}, "clash-rs-x86_64-unknown-linux-gnu"},
		{core.Target{OS: "darwin", Arch: "arm64"}, "clash-rs-aarch64-apple-darwin"},
		{core.Target{OS: "windows", Arch: "amd64"}, "clash-rs-x86_64-pc-windows-msvc.exe"},
	}
	for _, test := range tests {
		if actual := assetName(test.target); actual != test.name {
			t.Fatalf("asset for %#v = %q", test.target, actual)
		}
	}
}

func TestVersionOutput(t *testing.T) {
	t.Parallel()
	match := versionPattern.FindStringSubmatch("clash-rs 0.10.8\n")
	if len(match) != 2 || match[1] != "0.10.8" {
		t.Fatalf("version match = %#v", match)
	}
}

func TestWindowsDoesNotAdvertiseUnmanagedTUN(t *testing.T) {
	t.Parallel()
	capabilities := New().Capabilities("0.10.8", core.Target{OS: "windows", Arch: "amd64"})
	for _, feature := range capabilities.Features {
		if feature == core.CapabilityTransparentTUN {
			t.Fatal("Windows TUN requires a managed Wintun dependency")
		}
	}
}
