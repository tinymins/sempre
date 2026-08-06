package core

import (
	"context"
	"fmt"
	"regexp"
	"strings"

	"github.com/tinymins/sempre/internal/release"
)

var releaseDigestPattern = regexp.MustCompile(`(?i)^sha256:[0-9a-f]{64}$`)

type ReleaseResolver interface {
	LatestStable(context.Context, string) (release.GitHubRelease, error)
	Version(context.Context, string, string) (release.GitHubRelease, error)
}

type AssetSelection struct {
	Names  []string
	Format string
}

func ResolveGitHubPackage(
	ctx context.Context,
	resolver ReleaseResolver,
	defaultRepository string,
	source string,
	reference string,
	selection func(string) (AssetSelection, error),
) (Package, error) {
	if source == "" {
		source = defaultRepository
	}
	var (
		item release.GitHubRelease
		err  error
	)
	if reference == Stable {
		item, err = resolver.LatestStable(ctx, source)
		if err == nil && item.Prerelease {
			err = fmt.Errorf("latest release %s is a prerelease", item.Tag)
		}
	} else {
		item, err = resolver.Version(ctx, source, reference)
	}
	if err != nil {
		return Package{}, err
	}
	version := strings.TrimPrefix(item.Tag, "v")
	selected, err := selection(version)
	if err != nil {
		return Package{}, err
	}
	for _, name := range selected.Names {
		for _, asset := range item.Assets {
			if asset.Name != name {
				continue
			}
			if !releaseDigestPattern.MatchString(strings.TrimSpace(asset.Digest)) {
				return Package{}, fmt.Errorf("release asset %s does not provide a valid SHA-256 digest", name)
			}
			return Package{Version: version, Name: name, URL: asset.URL, Digest: asset.Digest, Size: asset.Size, Format: selected.Format}, nil
		}
	}
	return Package{}, fmt.Errorf("release %s has no supported asset; tried %s", version, strings.Join(selected.Names, ", "))
}
