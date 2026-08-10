package singbox

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
)

type fakeReleases struct {
	repository string
	version    string
	item       release.GitHubRelease
}

func (releases *fakeReleases) LatestStable(_ context.Context, repository string) (release.GitHubRelease, error) {
	releases.repository = repository
	return releases.item, nil
}

func (releases *fakeReleases) Version(_ context.Context, repository, version string) (release.GitHubRelease, error) {
	releases.repository = repository
	releases.version = version
	return releases.item, nil
}

func TestResolveUsesCustomRepositoryAndExactPrerelease(t *testing.T) {
	t.Parallel()
	releases := &fakeReleases{item: release.GitHubRelease{
		Tag:        "v1.13.15-ddns.1",
		Prerelease: true,
		Assets: []release.Asset{{
			Name:   "sing-box-1.13.15-ddns.1-linux-amd64.tar.gz",
			URL:    "https://github.com/tinymins/sing-box/releases/download/v1.13.15-ddns.1/archive.tar.gz",
			Digest: "sha256:test",
			Size:   42,
		}},
	}}
	adapter := &Adapter{releases: releases}
	item, err := adapter.Resolve(context.Background(), "tinymins/sing-box", "1.13.15-ddns.1", core.Target{OS: "linux", Arch: "amd64"})
	if err != nil {
		t.Fatal(err)
	}
	if releases.repository != "tinymins/sing-box" || releases.version != "1.13.15-ddns.1" {
		t.Fatalf("release query = %s@%s", releases.repository, releases.version)
	}
	if item.Version != "1.13.15-ddns.1" || item.Name != "sing-box-1.13.15-ddns.1-linux-amd64.tar.gz" {
		t.Fatalf("package = %#v", item)
	}
}

func TestCompilerTargetTracksSupportedMinorVersions(t *testing.T) {
	t.Parallel()
	tests := []struct {
		version string
		format  string
		warn    bool
	}{
		{version: "1.11.15", format: "sing-box-macos"},
		{version: "1.12.20", format: "sing-box-v12-macos"},
		{version: "1.13.18", format: "sing-box-v13-macos"},
		{version: "1.14.0-beta.13", format: "sing-box-v14-macos"},
		{version: "1.15.0", format: "sing-box-v14-macos", warn: true},
	}
	for _, test := range tests {
		test := test
		t.Run(test.version, func(t *testing.T) {
			t.Parallel()
			target, err := New().CompilerTarget(test.version, core.Target{OS: "darwin", Arch: "arm64"})
			if err != nil {
				t.Fatal(err)
			}
			if target.Format != test.format || (len(target.Warnings) > 0) != test.warn {
				t.Fatalf("target = %#v", target)
			}
		})
	}
}

func TestMacOSCapabilitiesFollowCompilerVersion(t *testing.T) {
	t.Parallel()
	tests := []struct {
		version string
		fakeIP  bool
		tun     bool
	}{
		{version: "1.12.20", fakeIP: true, tun: true},
		{version: "1.13.18", fakeIP: true, tun: true},
		{version: "1.14.0-beta.13", fakeIP: true, tun: true},
	}
	for _, test := range tests {
		capabilities := New().Capabilities(test.version, core.Target{OS: "darwin", Arch: "arm64"})
		if hasFeature(capabilities, core.CapabilityDNSFakeIP) != test.fakeIP || hasFeature(capabilities, core.CapabilityTransparentTUN) != test.tun {
			t.Fatalf("%s capabilities = %#v", test.version, capabilities.Features)
		}
	}
	linux := New().Capabilities("1.13.18", core.Target{OS: "linux", Arch: "arm64"})
	if !hasFeature(linux, core.CapabilityDNSFakeIP) || !hasFeature(linux, core.CapabilityTransparentTUN) {
		t.Fatalf("Linux capabilities changed: %#v", linux.Features)
	}
}

func hasFeature(capabilities core.Capabilities, feature string) bool {
	for _, current := range capabilities.Features {
		if current == feature {
			return true
		}
	}
	return false
}

func TestPrepareRuntimeIsolatesControlAPIFromUserConfig(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	config := filepath.Join(root, "source.json")
	original := []byte(`{
  "custom": {"preserved": true},
  "experimental": {"other": "value", "clash_api": {"external_controller": "0.0.0.0:9090", "secret": "user-secret", "custom": 42}}
}`)
	if err := os.WriteFile(config, original, 0o600); err != nil {
		t.Fatal(err)
	}
	spec, err := New().PrepareRuntime(config, filepath.Join(root, "runtime"))
	if err != nil {
		t.Fatal(err)
	}
	after, err := os.ReadFile(config)
	if err != nil || string(after) != string(original) {
		t.Fatalf("source configuration changed: %v", err)
	}
	data, err := os.ReadFile(spec.Config)
	if err != nil {
		t.Fatal(err)
	}
	var document map[string]any
	if err := json.Unmarshal(data, &document); err != nil {
		t.Fatal(err)
	}
	experimental := document["experimental"].(map[string]any)
	clashAPI := experimental["clash_api"].(map[string]any)
	if document["custom"].(map[string]any)["preserved"] != true || experimental["other"] != "value" || clashAPI["custom"] != float64(42) {
		t.Fatalf("unrelated configuration was not preserved: %#v", document)
	}
	if clashAPI["external_controller"] == "0.0.0.0:9090" || clashAPI["external_controller"] != spec.Control.BaseURL[len("http://"):] {
		t.Fatalf("external controller = %#v, control = %#v", clashAPI["external_controller"], spec.Control)
	}
	if clashAPI["secret"] == "user-secret" || clashAPI["secret"] != spec.Control.Secret || spec.Control.Secret == "" {
		t.Fatalf("control secret was not isolated")
	}
	if clashAPI["external_ui"] != "" || clashAPI["access_control_allow_private_network"] != false {
		t.Fatalf("unsafe Clash API settings remain: %#v", clashAPI)
	}
}
