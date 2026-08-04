package ui

import (
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"

	"github.com/tinymins/sempre/internal/release"
)

const (
	GitHubAssetName    = "sempre-ui.zip"
	GitHubChecksumName = "SHA256SUMS"
	maxChecksumSize    = int64(1 << 20)
)

var (
	githubRepositoryPattern = regexp.MustCompile(`^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})/[A-Za-z0-9_.-]{1,100}$`)
	githubVersionPattern    = regexp.MustCompile(`^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$`)
)

type GitHubReference struct {
	Repository string
	Value      string
}

func (reference GitHubReference) String() string {
	return reference.Repository + "@" + reference.Value
}

func ParseGitHubReference(value string) (GitHubReference, error) {
	value = strings.TrimSpace(value)
	if strings.Count(value, "@") > 1 {
		return GitHubReference{}, fmt.Errorf("invalid UI GitHub reference %q", value)
	}
	repository, reference, found := strings.Cut(value, "@")
	if !githubRepositoryPattern.MatchString(repository) {
		return GitHubReference{}, fmt.Errorf("invalid UI GitHub repository %q; expected owner/repository", repository)
	}
	_, name, _ := strings.Cut(repository, "/")
	if name == "." || name == ".." {
		return GitHubReference{}, fmt.Errorf("invalid UI GitHub repository %q", repository)
	}
	if !found {
		reference = "stable"
	}
	reference = strings.TrimPrefix(reference, "v")
	if reference != "stable" && !githubVersionPattern.MatchString(reference) {
		return GitHubReference{}, fmt.Errorf("invalid UI version or channel %q", reference)
	}
	return GitHubReference{Repository: strings.ToLower(repository), Value: reference}, nil
}

type ReleaseResolver interface {
	LatestStable(context.Context, string) (release.GitHubRelease, error)
	Version(context.Context, string, string) (release.GitHubRelease, error)
}

func (manager *Manager) InstallGitHub(ctx context.Context, resolver ReleaseResolver, value string) (Metadata, error) {
	reference, err := ParseGitHubReference(value)
	if err != nil {
		return Metadata{}, err
	}
	var item release.GitHubRelease
	if reference.Value == "stable" {
		item, err = resolver.LatestStable(ctx, reference.Repository)
		if err == nil && item.Prerelease {
			err = fmt.Errorf("latest UI release %s is a prerelease", item.Tag)
		}
	} else {
		item, err = resolver.Version(ctx, reference.Repository, reference.Value)
	}
	if err != nil {
		return Metadata{}, err
	}
	var archive *release.Asset
	var checksums *release.Asset
	for index := range item.Assets {
		asset := &item.Assets[index]
		switch asset.Name {
		case GitHubAssetName:
			archive = asset
		case GitHubChecksumName:
			checksums = asset
		}
	}
	if archive == nil {
		return Metadata{}, fmt.Errorf("UI release %s has no %s", item.Tag, GitHubAssetName)
	}
	digest, digestErr := releaseAssetDigest(archive.Digest)
	if digestErr != nil && checksums != nil {
		digest, digestErr = manager.checksumFromRelease(ctx, *checksums, GitHubAssetName)
	}
	if digestErr != nil {
		return Metadata{}, fmt.Errorf("UI release %s does not provide a valid SHA-256 for %s: %w", item.Tag, GitHubAssetName, digestErr)
	}
	return manager.InstallRemoteSource(ctx, archive.URL, digest, "github", reference.String())
}

func releaseAssetDigest(value string) (string, error) {
	value = strings.TrimSpace(value)
	if !strings.HasPrefix(strings.ToLower(value), "sha256:") {
		return "", fmt.Errorf("asset digest is missing")
	}
	value = value[len("sha256:"):]
	decoded, err := hex.DecodeString(value)
	if err != nil || len(decoded) != sha256.Size {
		return "", fmt.Errorf("asset digest is invalid")
	}
	return strings.ToLower(value), nil
}

func (manager *Manager) checksumFromRelease(ctx context.Context, asset release.Asset, name string) (string, error) {
	parsed, err := url.Parse(asset.URL)
	if err != nil || !strings.EqualFold(parsed.Scheme, "https") || parsed.Hostname() == "" || parsed.User != nil {
		return "", fmt.Errorf("checksum URL must be HTTPS without credentials")
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, parsed.String(), nil)
	if err != nil {
		return "", err
	}
	response, err := manager.http.Do(request)
	if err != nil {
		return "", fmt.Errorf("download UI checksums: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return "", fmt.Errorf("download UI checksums: HTTP %s", response.Status)
	}
	if response.ContentLength > maxChecksumSize {
		return "", fmt.Errorf("UI checksums exceed %d bytes", maxChecksumSize)
	}
	data, err := io.ReadAll(io.LimitReader(response.Body, maxChecksumSize+1))
	if err != nil {
		return "", err
	}
	if int64(len(data)) > maxChecksumSize {
		return "", fmt.Errorf("UI checksums exceed %d bytes", maxChecksumSize)
	}
	reader := bufio.NewScanner(strings.NewReader(string(data)))
	for reader.Scan() {
		fields := strings.Fields(reader.Text())
		if len(fields) != 2 || strings.TrimPrefix(fields[1], "*") != name {
			continue
		}
		decoded, decodeErr := hex.DecodeString(fields[0])
		if decodeErr == nil && len(decoded) == sha256.Size {
			return strings.ToLower(fields[0]), nil
		}
	}
	if err := reader.Err(); err != nil {
		return "", err
	}
	return "", fmt.Errorf("checksum for %s is missing or invalid", name)
}
