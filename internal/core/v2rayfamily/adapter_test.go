package v2rayfamily

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"testing"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
)

type fakeReleases struct {
	item release.GitHubRelease
}

func (releases fakeReleases) LatestStable(context.Context, string) (release.GitHubRelease, error) {
	return releases.item, nil
}

func (releases fakeReleases) Version(context.Context, string, string) (release.GitHubRelease, error) {
	return releases.item, nil
}

func testKind() Kind {
	return Kind{
		ID: "xray", Name: "Xray-core", Repository: "XTLS/Xray-core", Asset: "Xray",
		VersionRE: regexp.MustCompile(`Xray\s+([0-9.]+)`), Services: []string{"RoutingService"},
	}
}

func TestResolveXrayOfficialAsset(t *testing.T) {
	t.Parallel()
	digest := "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
	adapter := New(testKind())
	adapter.releases = fakeReleases{item: release.GitHubRelease{Tag: "v26.3.27", Assets: []release.Asset{{
		Name: "Xray-linux-arm64-v8a.zip", URL: "https://example.invalid/xray.zip", Digest: digest, Size: 42,
	}}}}
	item, err := adapter.Resolve(context.Background(), "", core.Stable, core.Target{OS: "linux", Arch: "arm64"})
	if err != nil {
		t.Fatal(err)
	}
	if item.Version != "26.3.27" || item.Name != "Xray-linux-arm64-v8a.zip" || item.Format != "zip" {
		t.Fatalf("package = %#v", item)
	}
}

func TestPrepareRuntimeAddsLoopbackGRPCWithoutChangingSource(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	config := filepath.Join(root, "config.json")
	original := []byte(`{"inbounds":[],"outbounds":[],"routing":{"rules":[]}}`)
	if err := os.WriteFile(config, original, 0o600); err != nil {
		t.Fatal(err)
	}
	adapter := New(testKind())
	spec, err := adapter.PrepareRuntime(config, filepath.Join(root, "runtime"))
	if err != nil {
		t.Fatal(err)
	}
	if spec.Control.Protocol != core.ControlProtocolGRPC || spec.Control.Core != "xray" {
		t.Fatalf("control = %#v", spec.Control)
	}
	after, err := os.ReadFile(config)
	if err != nil || string(after) != string(original) {
		t.Fatalf("source changed: %v", err)
	}
	data, err := os.ReadFile(spec.Config)
	if err != nil {
		t.Fatal(err)
	}
	var document map[string]any
	if err := json.Unmarshal(data, &document); err != nil {
		t.Fatal(err)
	}
	inbounds := document["inbounds"].([]any)
	api := inbounds[0].(map[string]any)
	if api["tag"] != "sempre-api-in" || api["listen"] != "127.0.0.1" || api["port"] == float64(0) {
		t.Fatalf("API inbound = %#v", api)
	}
}

func TestRunUsesInstalledAssetDirectory(t *testing.T) {
	t.Parallel()
	binary := filepath.Join("opt", "sempre", "xray")
	spec := New(testKind()).Run(binary, filepath.Join("runtime", "config.json"), "runtime")
	if len(spec.Env) != 1 || spec.Env[0] != "xray.location.asset="+filepath.Dir(binary) {
		t.Fatalf("run environment = %#v", spec.Env)
	}
}
