package ui

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/release"
)

type fakeReleaseResolver struct {
	latest release.GitHubRelease
	exact  release.GitHubRelease
}

func (resolver fakeReleaseResolver) LatestStable(context.Context, string) (release.GitHubRelease, error) {
	return resolver.latest, nil
}

func (resolver fakeReleaseResolver) Version(context.Context, string, string) (release.GitHubRelease, error) {
	return resolver.exact, nil
}

func TestParseGitHubReference(t *testing.T) {
	t.Parallel()
	for _, test := range []struct {
		value string
		want  string
	}{
		{value: "TinyMins/Sempre-UI", want: "tinymins/sempre-ui@stable"},
		{value: "tinymins/sempre-ui@v1.2.3", want: "tinymins/sempre-ui@1.2.3"},
		{value: "tinymins/sempre-ui@1.2.3-beta.1", want: "tinymins/sempre-ui@1.2.3-beta.1"},
	} {
		reference, err := ParseGitHubReference(test.value)
		if err != nil || reference.String() != test.want {
			t.Errorf("ParseGitHubReference(%q) = %#v, %v; want %q", test.value, reference, err, test.want)
		}
	}
	for _, value := range []string{"sempre-ui", "tinymins/sempre-ui@latest", "tinymins/sempre-ui@1.2"} {
		if _, err := ParseGitHubReference(value); err == nil {
			t.Errorf("ParseGitHubReference(%q) unexpectedly succeeded", value)
		}
	}
}

func TestInstallGitHubUsesAssetDigest(t *testing.T) {
	archive, digest, server := githubUITestServer(t, false)
	defer server.Close()
	manager := New(filepath.Join(t.TempDir(), "ui"), filepath.Join(t.TempDir(), "current"))
	manager.current = filepath.Join(manager.root, "current")
	manager.http = server.Client()
	resolver := fakeReleaseResolver{latest: release.GitHubRelease{Tag: "v1.2.3", Assets: []release.Asset{{
		Name: GitHubAssetName, URL: server.URL + "/ui.zip", Digest: "sha256:" + digest, Size: int64(len(archive)),
	}}}}
	metadata, err := manager.InstallGitHub(context.Background(), resolver, "TinyMins/Sempre-UI@stable")
	if err != nil {
		t.Fatal(err)
	}
	if metadata.SourceType != "github" || metadata.Source != "tinymins/sempre-ui@stable" || metadata.Digest != digest {
		t.Fatalf("metadata = %#v", metadata)
	}
}

func TestInstallGitHubFallsBackToReleaseChecksums(t *testing.T) {
	archive, digest, server := githubUITestServer(t, true)
	defer server.Close()
	root := filepath.Join(t.TempDir(), "ui")
	manager := New(root, filepath.Join(root, "current"))
	manager.http = server.Client()
	resolver := fakeReleaseResolver{exact: release.GitHubRelease{Tag: "v1.2.3", Assets: []release.Asset{
		{Name: GitHubAssetName, URL: server.URL + "/ui.zip", Size: int64(len(archive))},
		{Name: GitHubChecksumName, URL: server.URL + "/SHA256SUMS"},
	}}}
	metadata, err := manager.InstallGitHub(context.Background(), resolver, "tinymins/sempre-ui@1.2.3")
	if err != nil {
		t.Fatal(err)
	}
	if metadata.Digest != digest || metadata.Source != "tinymins/sempre-ui@1.2.3" {
		t.Fatalf("metadata = %#v", metadata)
	}
}

func TestInstallGitHubRejectsReleaseWithoutDigest(t *testing.T) {
	archive, _, server := githubUITestServer(t, false)
	defer server.Close()
	root := filepath.Join(t.TempDir(), "ui")
	manager := New(root, filepath.Join(root, "current"))
	manager.http = server.Client()
	resolver := fakeReleaseResolver{latest: release.GitHubRelease{Tag: "v1.2.3", Assets: []release.Asset{{
		Name: GitHubAssetName, URL: server.URL + "/ui.zip", Size: int64(len(archive)),
	}}}}
	if _, err := manager.InstallGitHub(context.Background(), resolver, "tinymins/sempre-ui@stable"); err == nil || !strings.Contains(err.Error(), "valid SHA-256") {
		t.Fatalf("InstallGitHub error = %v", err)
	}
}

func TestChecksumFromReleaseRejectsOversizedChunkedResponse(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		_, _ = writer.Write([]byte(strings.Repeat("a", int(maxChecksumSize)+1)))
	}))
	defer server.Close()
	root := filepath.Join(t.TempDir(), "ui")
	manager := New(root, filepath.Join(root, "current"))
	manager.http = server.Client()
	if _, err := manager.checksumFromRelease(context.Background(), release.Asset{URL: server.URL}, GitHubAssetName); err == nil || !strings.Contains(err.Error(), "exceed") {
		t.Fatalf("checksumFromRelease error = %v", err)
	}
}

func githubUITestServer(t *testing.T, serveChecksums bool) ([]byte, string, *httptest.Server) {
	t.Helper()
	path := writeTestArchive(t, map[string]string{
		"index.html": "<main>Sempre</main>",
		ManifestName: `{"schema":1,"name":"GitHub UI","version":"1.2.3","entry":"index.html","api":{"major":1}}`,
	})
	archive, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256(archive)
	digest := hex.EncodeToString(sum[:])
	server := httptest.NewTLSServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/ui.zip":
			writer.Header().Set("Content-Length", strconv.Itoa(len(archive)))
			_, _ = writer.Write(archive)
		case "/SHA256SUMS":
			if !serveChecksums {
				http.NotFound(writer, request)
				return
			}
			_, _ = writer.Write([]byte(digest + "  " + GitHubAssetName + "\n"))
		default:
			http.NotFound(writer, request)
		}
	}))
	return archive, digest, server
}
