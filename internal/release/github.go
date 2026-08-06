package release

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/tinymins/sempre/internal/buildinfo"
)

type Asset struct {
	Name      string `json:"name"`
	URL       string `json:"browser_download_url"`
	Digest    string `json:"digest"`
	Size      int64  `json:"size"`
	CreatedAt string `json:"created_at"`
}

type GitHubRelease struct {
	Tag        string  `json:"tag_name"`
	Draft      bool    `json:"draft"`
	Prerelease bool    `json:"prerelease"`
	Assets     []Asset `json:"assets"`
}

var releaseCache = struct {
	sync.Mutex
	items map[string]GitHubRelease
}{items: map[string]GitHubRelease{}}

type Client struct {
	http  *http.Client
	base  string
	token string
}

func NewClient() *Client {
	return &Client{
		http: &http.Client{
			Timeout: 30 * time.Second,
			CheckRedirect: func(request *http.Request, via []*http.Request) error {
				if len(via) >= 10 {
					return fmt.Errorf("too many redirects")
				}
				if !strings.EqualFold(request.URL.Scheme, "https") {
					return fmt.Errorf("refuse non-HTTPS redirect")
				}
				return nil
			},
		},
		base:  "https://api.github.com",
		token: githubTokenFromEnvironment(),
	}
}

func (client *Client) LatestStable(ctx context.Context, repository string) (GitHubRelease, error) {
	return client.get(ctx, client.base+"/repos/"+repository+"/releases/latest")
}

func (client *Client) Version(ctx context.Context, repository, version string) (GitHubRelease, error) {
	tag := "v" + strings.TrimPrefix(version, "v")
	return client.get(ctx, client.base+"/repos/"+repository+"/releases/tags/"+url.PathEscape(tag))
}

func (client *Client) get(ctx context.Context, endpoint string) (GitHubRelease, error) {
	releaseCache.Lock()
	if cached, ok := releaseCache.items[endpoint]; ok {
		releaseCache.Unlock()
		return cloneRelease(cached), nil
	}
	releaseCache.Unlock()
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return GitHubRelease{}, err
	}
	request.Header.Set("Accept", "application/vnd.github+json")
	request.Header.Set("X-GitHub-Api-Version", "2022-11-28")
	request.Header.Set("User-Agent", "Sempre/"+buildinfo.Version)
	if client.token != "" {
		request.Header.Set("Authorization", "Bearer "+client.token)
	}
	response, err := client.http.Do(request)
	if err != nil {
		return GitHubRelease{}, fmt.Errorf("query GitHub release: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return GitHubRelease{}, fmt.Errorf("query GitHub release: HTTP %s", response.Status)
	}
	var result GitHubRelease
	decoder := json.NewDecoder(io.LimitReader(response.Body, 4<<20))
	if err := decoder.Decode(&result); err != nil {
		return GitHubRelease{}, fmt.Errorf("decode GitHub release: %w", err)
	}
	if result.Draft {
		return GitHubRelease{}, fmt.Errorf("GitHub release %s is a draft", result.Tag)
	}
	releaseCache.Lock()
	releaseCache.items[endpoint] = cloneRelease(result)
	releaseCache.Unlock()
	return result, nil
}

func cloneRelease(item GitHubRelease) GitHubRelease {
	item.Assets = append([]Asset(nil), item.Assets...)
	return item
}

func githubTokenFromEnvironment() string {
	if token := strings.TrimSpace(os.Getenv("GITHUB_TOKEN")); token != "" {
		return token
	}
	return strings.TrimSpace(os.Getenv("GH_TOKEN"))
}
