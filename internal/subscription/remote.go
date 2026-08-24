package subscription

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
	"unicode/utf8"
)

const maxManifestSize = int64(1 << 20)

type RemoteClient struct {
	http *http.Client
}

type remoteManifest struct {
	Schema  int    `json:"schema"`
	Service string `json:"service"`
	Profile struct {
		Name      string    `json:"name"`
		Revision  int64     `json:"revision"`
		UpdatedAt time.Time `json:"updated_at"`
	} `json:"profile"`
	Target   Target `json:"target"`
	Artifact struct {
		URL         string    `json:"url"`
		SHA256      string    `json:"sha256"`
		ContentType string    `json:"content_type"`
		NodeCount   int       `json:"node_count"`
		CreatedAt   time.Time `json:"created_at"`
	} `json:"artifact"`
	Runtime  *remoteRuntime `json:"runtime,omitempty"`
	EditURL  string         `json:"edit_url"`
	ReadOnly bool           `json:"read_only"`
}

type remoteRuntime struct {
	LocalProxy       LocalProxyConfig       `json:"local_proxy"`
	TransparentProxy TransparentProxyConfig `json:"transparent_proxy"`
	ManagementAPI    ManagementAPIConfig    `json:"management_api"`
}

func NewRemoteClient(client *http.Client) *RemoteClient {
	if client == nil {
		client = &http.Client{Timeout: 30 * time.Second}
	}
	isolated := *client
	isolated.CheckRedirect = func(_ *http.Request, _ []*http.Request) error {
		return http.ErrUseLastResponse
	}
	return &RemoteClient{http: &isolated}
}

func ValidateRemoteManifestURL(value string) error {
	_, err := parseRemoteURL(value)
	return err
}

func (client *RemoteClient) Render(ctx context.Context, profile Profile, target Target) (RenderResult, Profile, error) {
	if profile.Remote == nil {
		return RenderResult{}, profile, fmt.Errorf("remote profile has no manifest URL")
	}
	manifestURL, err := parseRemoteURL(profile.Remote.ManifestURL)
	if err != nil {
		return RenderResult{}, profile, err
	}
	query := manifestURL.Query()
	query.Set("target", target.Format)
	manifestURL.RawQuery = query.Encode()
	var manifest remoteManifest
	if err := client.getJSON(ctx, manifestURL, &manifest); err != nil {
		return RenderResult{}, profile, fmt.Errorf("fetch remote manifest: %w", err)
	}
	if err := validateRemoteManifest(manifest, target.Format); err != nil {
		return RenderResult{}, profile, err
	}
	artifactURL, err := manifestURL.Parse(manifest.Artifact.URL)
	if err != nil {
		return RenderResult{}, profile, fmt.Errorf("remote artifact URL is invalid")
	}
	if err := validateSameOrigin(manifestURL, artifactURL); err != nil {
		return RenderResult{}, profile, err
	}
	content, err := client.getText(ctx, artifactURL)
	if err != nil {
		return RenderResult{}, profile, fmt.Errorf("fetch remote artifact: %w", err)
	}
	hash := sha256.Sum256([]byte(content))
	actualHash := hex.EncodeToString(hash[:])
	if !strings.EqualFold(actualHash, manifest.Artifact.SHA256) {
		return RenderResult{}, profile, fmt.Errorf("remote artifact SHA-256 does not match its manifest")
	}
	updated := profile
	if manifest.Runtime != nil {
		updated.LocalProxy = manifest.Runtime.LocalProxy
		updated.TransparentProxy = manifest.Runtime.TransparentProxy
		updated.ManagementAPI = manifest.Runtime.ManagementAPI
	}
	remote := *profile.Remote
	remote.EditURL = manifest.EditURL
	remote.ServerProfile = manifest.Profile.Name
	remote.ServerRevision = manifest.Profile.Revision
	remote.ArtifactSHA256 = actualHash
	remote.Target = target.Format
	remote.NodeCount = manifest.Artifact.NodeCount
	remote.ServerUpdatedAt = manifest.Profile.UpdatedAt
	remote.ArtifactCreatedAt = manifest.Artifact.CreatedAt
	remote.LastSyncedAt = time.Now().UTC()
	updated.Remote = &remote
	return RenderResult{
		Format: target.Format, Version: target.Version, Platform: target.Platform,
		Content: content, NodeCount: manifest.Artifact.NodeCount,
		Warnings: []string{"configuration supplied by the remote Sempre conversion service"},
	}, updated, nil
}

func (client *RemoteClient) getJSON(ctx context.Context, address *url.URL, result any) error {
	response, err := client.get(ctx, address)
	if err != nil {
		return err
	}
	defer response.Body.Close()
	data, err := io.ReadAll(io.LimitReader(response.Body, maxManifestSize+1))
	if err != nil {
		return err
	}
	if int64(len(data)) > maxManifestSize {
		return fmt.Errorf("manifest exceeds %d bytes", maxManifestSize)
	}
	if err := json.Unmarshal(data, result); err != nil {
		return fmt.Errorf("decode response: %w", err)
	}
	return nil
}

func (client *RemoteClient) getText(ctx context.Context, address *url.URL) (string, error) {
	response, err := client.get(ctx, address)
	if err != nil {
		return "", err
	}
	defer response.Body.Close()
	data, err := io.ReadAll(io.LimitReader(response.Body, MaxSourceSize+1))
	if err != nil {
		return "", err
	}
	if int64(len(data)) > MaxSourceSize {
		return "", fmt.Errorf("response exceeds %d bytes", MaxSourceSize)
	}
	if !utf8.Valid(data) {
		return "", fmt.Errorf("response is not UTF-8")
	}
	return string(data), nil
}

func (client *RemoteClient) get(ctx context.Context, address *url.URL) (*http.Response, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, address.String(), nil)
	if err != nil {
		return nil, err
	}
	request.Header.Set("Accept", "application/json, application/yaml, text/plain")
	request.Header.Set("User-Agent", "sempre-client/remote-subscription")
	response, err := client.http.Do(request)
	if err != nil {
		return nil, err
	}
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		response.Body.Close()
		return nil, fmt.Errorf("server returned HTTP %d", response.StatusCode)
	}
	return response, nil
}

func validateRemoteManifest(manifest remoteManifest, target string) error {
	if manifest.Schema != 1 || manifest.Service != "sempre" || !manifest.ReadOnly {
		return fmt.Errorf("unsupported remote subscription manifest")
	}
	if manifest.Target.Format != target {
		return fmt.Errorf("remote artifact target %q does not match requested target %q", manifest.Target.Format, target)
	}
	if manifest.Profile.Revision < 1 || strings.TrimSpace(manifest.Profile.Name) == "" || manifest.Artifact.NodeCount < 0 {
		return fmt.Errorf("remote subscription manifest is incomplete")
	}
	if len(manifest.Artifact.SHA256) != 64 {
		return fmt.Errorf("remote artifact manifest has an invalid SHA-256")
	}
	if _, err := hex.DecodeString(manifest.Artifact.SHA256); err != nil {
		return fmt.Errorf("remote artifact manifest has an invalid SHA-256")
	}
	if manifest.EditURL != "" {
		if _, err := parseRemoteURL(manifest.EditURL); err != nil {
			return fmt.Errorf("remote edit URL is invalid")
		}
	}
	return nil
}

func parseRemoteURL(value string) (*url.URL, error) {
	parsed, err := url.Parse(strings.TrimSpace(value))
	if err != nil || parsed.Host == "" || (parsed.Scheme != "http" && parsed.Scheme != "https") || parsed.User != nil {
		return nil, fmt.Errorf("remote subscription URL must be an HTTP or HTTPS URL without credentials")
	}
	return parsed, nil
}

func validateSameOrigin(manifest, artifact *url.URL) error {
	if artifact.User != nil || manifest.Scheme != artifact.Scheme || !strings.EqualFold(manifest.Host, artifact.Host) {
		return fmt.Errorf("remote artifact URL must use the manifest origin")
	}
	return nil
}
