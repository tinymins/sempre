package dae

import (
	"context"
	"path/filepath"
	"testing"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
)

type fakeReleases struct{ item release.GitHubRelease }

func (releases fakeReleases) LatestStable(context.Context, string) (release.GitHubRelease, error) {
	return releases.item, nil
}

func (releases fakeReleases) Version(context.Context, string, string) (release.GitHubRelease, error) {
	return releases.item, nil
}

func TestResolveSelectsAMD64LevelAsset(t *testing.T) {
	t.Parallel()
	digest := "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
	adapter := New()
	adapter.releases = fakeReleases{item: release.GitHubRelease{Tag: "v2.0.0", Assets: []release.Asset{{
		Name: "dae-linux-x86_64_v3_avx2.zip", URL: "https://example.invalid/dae.zip", Digest: digest,
	}}}}
	item, err := adapter.Resolve(context.Background(), "", core.Stable, core.Target{OS: "linux", Arch: "amd64", AMD64Level: 3})
	if err != nil {
		t.Fatal(err)
	}
	if item.Name != "dae-linux-x86_64_v3_avx2.zip" || item.Format != "zip" || adapter.ExecutableName(core.Target{OS: "linux", Arch: "amd64", AMD64Level: 3}) != "dae-linux-x86_64_v3_avx2" {
		t.Fatalf("package = %#v", item)
	}
}

func TestRejectsNonLinuxTarget(t *testing.T) {
	t.Parallel()
	if _, err := New().Resolve(context.Background(), "", core.Stable, core.Target{OS: "darwin", Arch: "arm64"}); err == nil {
		t.Fatal("expected unsupported target")
	}
}

func TestRunUsesInstalledAssetDirectory(t *testing.T) {
	t.Parallel()
	binary := filepath.Join("opt", "sempre", "dae")
	spec := New().Run(binary, filepath.Join("runtime", "config.dae"), "runtime")
	if len(spec.Env) != 1 || spec.Env[0] != "DAE_LOCATION_ASSET="+filepath.Dir(binary) {
		t.Fatalf("run environment = %#v", spec.Env)
	}
}
