package release

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestLatestStableDecodesRelease(t *testing.T) {
	t.Parallel()
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/repos/SagerNet/sing-box/releases/latest" {
			t.Errorf("path = %q", request.URL.Path)
		}
		response.Header().Set("Content-Type", "application/json")
		_, _ = response.Write([]byte(`{
			"tag_name":"v1.2.3",
			"draft":false,
			"prerelease":false,
			"assets":[{"name":"asset.zip","browser_download_url":"https://example.com/asset.zip","digest":"sha256:00","size":42}]
		}`))
	}))
	defer server.Close()
	client := NewClient()
	client.base = server.URL
	client.http = server.Client()
	item, err := client.LatestStable(context.Background(), "SagerNet/sing-box")
	if err != nil {
		t.Fatal(err)
	}
	if item.Tag != "v1.2.3" || len(item.Assets) != 1 {
		t.Fatalf("release = %#v", item)
	}
}

func TestClientUsesGitHubToken(t *testing.T) {
	t.Setenv("GITHUB_TOKEN", "secret-token")
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if got := request.Header.Get("Authorization"); got != "Bearer secret-token" {
			t.Errorf("authorization = %q", got)
		}
		response.Header().Set("Content-Type", "application/json")
		_, _ = response.Write([]byte(`{"tag_name":"v1.2.3","draft":false,"prerelease":false,"assets":[]}`))
	}))
	defer server.Close()
	client := NewClient()
	client.base = server.URL
	client.http = server.Client()
	if _, err := client.LatestStable(context.Background(), "SagerNet/sing-box"); err != nil {
		t.Fatal(err)
	}
}
