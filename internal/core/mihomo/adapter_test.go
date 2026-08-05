package mihomo

import (
	"context"
	"net"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
	"gopkg.in/yaml.v3"
)

const testDigest = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

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

func TestResolveSelectsPlatformAndCPUAssets(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name   string
		target core.Target
		asset  string
		format string
	}{
		{name: "linux amd64 v3", target: core.Target{OS: "linux", Arch: "amd64", AMD64Level: 3}, asset: "mihomo-linux-amd64-v3-v1.19.29.gz", format: "gz"},
		{name: "darwin amd64 v2", target: core.Target{OS: "darwin", Arch: "amd64", AMD64Level: 2}, asset: "mihomo-darwin-amd64-v2-v1.19.29.gz", format: "gz"},
		{name: "windows amd64 compatible", target: core.Target{OS: "windows", Arch: "amd64", AMD64Level: 1}, asset: "mihomo-windows-amd64-compatible-v1.19.29.zip", format: "zip"},
		{name: "linux arm64", target: core.Target{OS: "linux", Arch: "arm64"}, asset: "mihomo-linux-arm64-v1.19.29.gz", format: "gz"},
		{name: "darwin arm64", target: core.Target{OS: "darwin", Arch: "arm64"}, asset: "mihomo-darwin-arm64-v1.19.29.gz", format: "gz"},
		{name: "windows arm64", target: core.Target{OS: "windows", Arch: "arm64"}, asset: "mihomo-windows-arm64-v1.19.29.zip", format: "zip"},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			releases := &fakeReleases{item: release.GitHubRelease{Tag: "v1.19.29", Assets: []release.Asset{{Name: test.asset, URL: "https://example.com/core", Digest: testDigest, Size: 42}}}}
			adapter := &Adapter{releases: releases}
			item, err := adapter.Resolve(context.Background(), "", core.Stable, test.target)
			if err != nil {
				t.Fatal(err)
			}
			if releases.repository != repository || item.Name != test.asset || item.Format != test.format || item.Version != "1.19.29" {
				t.Fatalf("resolved package = %#v, repository = %q", item, releases.repository)
			}
		})
	}
}

func TestResolveFallsBackToCompatibleAndReportsAttempts(t *testing.T) {
	t.Parallel()
	compatible := "mihomo-linux-amd64-compatible-v1.19.29.gz"
	releases := &fakeReleases{item: release.GitHubRelease{Tag: "v1.19.29", Assets: []release.Asset{{Name: compatible, URL: "https://example.com/core", Digest: testDigest, Size: 42}}}}
	adapter := &Adapter{releases: releases}
	item, err := adapter.Resolve(context.Background(), "", core.Stable, core.Target{OS: "linux", Arch: "amd64", AMD64Level: 3})
	if err != nil {
		t.Fatal(err)
	}
	if item.Name != compatible {
		t.Fatalf("asset = %q", item.Name)
	}
	releases.item.Assets = nil
	_, err = adapter.Resolve(context.Background(), "", core.Stable, core.Target{OS: "linux", Arch: "amd64", AMD64Level: 3})
	if err == nil || !strings.Contains(err.Error(), "amd64-v3") || !strings.Contains(err.Error(), "amd64-v2") || !strings.Contains(err.Error(), "amd64-compatible") {
		t.Fatalf("missing asset error = %v", err)
	}
}

func TestResolveUsesCustomRepositoryForExactPrerelease(t *testing.T) {
	t.Parallel()
	name := "mihomo-linux-amd64-compatible-v1.20.0-alpha.1.gz"
	releases := &fakeReleases{item: release.GitHubRelease{Tag: "v1.20.0-alpha.1", Prerelease: true, Assets: []release.Asset{{Name: name, URL: "https://example.com/core", Digest: testDigest, Size: 42}}}}
	adapter := &Adapter{releases: releases}
	if _, err := adapter.Resolve(context.Background(), "example/mihomo", "1.20.0-alpha.1", core.Target{OS: "linux", Arch: "amd64"}); err != nil {
		t.Fatal(err)
	}
	if releases.repository != "example/mihomo" || releases.version != "1.20.0-alpha.1" {
		t.Fatalf("release query = %s@%s", releases.repository, releases.version)
	}
}

func TestResolveRejectsInvalidDigestAndTarget(t *testing.T) {
	t.Parallel()
	name := "mihomo-linux-arm64-v1.19.29.gz"
	adapter := &Adapter{releases: &fakeReleases{item: release.GitHubRelease{Tag: "v1.19.29", Assets: []release.Asset{{Name: name, URL: "https://example.com/core", Size: 42}}}}}
	if _, err := adapter.Resolve(context.Background(), "", core.Stable, core.Target{OS: "linux", Arch: "arm64"}); err == nil || !strings.Contains(err.Error(), "SHA-256") {
		t.Fatalf("digest error = %v", err)
	}
	if _, err := adapter.Resolve(context.Background(), "", core.Stable, core.Target{OS: "freebsd", Arch: "amd64"}); err == nil || !strings.Contains(err.Error(), "target OS") {
		t.Fatalf("target error = %v", err)
	}
}

func TestVersionParsingAndCommandSpecs(t *testing.T) {
	t.Parallel()
	for input, expected := range map[string]string{
		"Mihomo Meta v1.19.29 linux amd64\n": "1.19.29",
		"Mihomo Meta 1.20.0-alpha.1\n":       "1.20.0-alpha.1",
	} {
		actual, err := parseVersionOutput(input)
		if err != nil || actual != expected {
			t.Fatalf("parseVersionOutput(%q) = %q, %v", input, actual, err)
		}
	}
	if _, err := parseVersionOutput("mihomo 1.19.29"); err == nil {
		t.Fatal("invalid version output was accepted")
	}
	if got := validationArgs("config.yaml", "data"); !reflect.DeepEqual(got, []string{"-t", "-f", "config.yaml", "-d", "data"}) {
		t.Fatalf("validation args = %v", got)
	}
	run := New().Run("mihomo", "config.yaml", "data")
	if run.Path != "mihomo" || run.WorkingDir != "data" || !reflect.DeepEqual(run.Args, []string{"-f", "config.yaml", "-d", "data"}) {
		t.Fatalf("run spec = %#v", run)
	}
}

func TestPrepareRuntimeIsolatesController(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	config := filepath.Join(root, "source.yaml")
	original := []byte(`mode: rule
custom:
  preserved: true
external-controller: 0.0.0.0:9090
external-controller-tls: 0.0.0.0:9443
external-controller-unix: /tmp/mihomo.sock
external-controller-pipe: mihomo
external-doh-server: 0.0.0.0:8853
external-ui: ./ui
external-ui-url: https://example.com/ui.zip
secret: user-secret
external-controller-cors:
  allow-origins: ['*']
  allow-private-network: true
`)
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
	document := map[string]any{}
	if err := yaml.Unmarshal(data, &document); err != nil {
		t.Fatal(err)
	}
	if document["custom"].(map[string]any)["preserved"] != true || document["mode"] != "rule" {
		t.Fatalf("unrelated configuration was not preserved: %#v", document)
	}
	for _, key := range []string{"external-controller-tls", "external-controller-unix", "external-controller-pipe", "external-doh-server", "external-ui", "external-ui-url"} {
		if _, ok := document[key]; ok {
			t.Fatalf("unsafe setting %q remains", key)
		}
	}
	address, _ := document["external-controller"].(string)
	host, _, err := net.SplitHostPort(address)
	if err != nil || host != "127.0.0.1" || spec.Control.BaseURL != "http://"+address {
		t.Fatalf("controller = %q, control = %#v, error = %v", address, spec.Control, err)
	}
	if spec.Control.Core != "mihomo" || len(spec.Control.Secret) != 64 || document["secret"] != spec.Control.Secret || spec.Control.Secret == "user-secret" {
		t.Fatalf("control secret was not isolated: %#v", spec.Control)
	}
	cors := document["external-controller-cors"].(map[string]any)
	if cors["allow-private-network"] != false || !reflect.DeepEqual(cors["allow-origins"], []any{"http://localhost.invalid"}) {
		t.Fatalf("controller CORS = %#v", cors)
	}
}
