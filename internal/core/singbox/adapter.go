package singbox

import (
	"context"
	"fmt"
	"io"
	"os/exec"
	"strings"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
)

const repository = "SagerNet/sing-box"

type Adapter struct {
	releases *release.Client
}

func New() *Adapter {
	return &Adapter{releases: release.NewClient()}
}

func (adapter *Adapter) ID() string {
	return "sing-box"
}

func (adapter *Adapter) Resolve(ctx context.Context, reference string, target core.Target) (core.Package, error) {
	var (
		item release.GitHubRelease
		err  error
	)
	if reference == core.Stable {
		item, err = adapter.releases.LatestStable(ctx, repository)
		if err == nil && item.Prerelease {
			err = fmt.Errorf("latest release %s is a prerelease", item.Tag)
		}
	} else {
		item, err = adapter.releases.Version(ctx, repository, reference)
	}
	if err != nil {
		return core.Package{}, err
	}
	version := strings.TrimPrefix(item.Tag, "v")
	format := "tar.gz"
	extension := ".tar.gz"
	if target.OS == "windows" {
		format = "zip"
		extension = ".zip"
	}
	name := fmt.Sprintf("sing-box-%s-%s-%s%s", version, target.OS, target.Arch, extension)
	for _, asset := range item.Assets {
		if asset.Name == name {
			return core.Package{
				Version: version,
				Name:    asset.Name,
				URL:     asset.URL,
				Digest:  asset.Digest,
				Size:    asset.Size,
				Format:  format,
			}, nil
		}
	}
	return core.Package{}, fmt.Errorf("release %s has no asset for %s/%s", version, target.OS, target.Arch)
}

func (adapter *Adapter) ExecutableName(target core.Target) string {
	if target.OS == "windows" {
		return "sing-box.exe"
	}
	return "sing-box"
}

func (adapter *Adapter) Version(ctx context.Context, binary string) (string, error) {
	output, err := exec.CommandContext(ctx, binary, "version").CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("run sing-box version: %w: %s", err, strings.TrimSpace(string(output)))
	}
	line, _, _ := strings.Cut(strings.TrimSpace(string(output)), "\n")
	version := strings.TrimPrefix(strings.TrimSpace(line), "sing-box version ")
	if version == "" {
		return "", fmt.Errorf("sing-box returned an empty version")
	}
	return version, nil
}

func (adapter *Adapter) Validate(
	ctx context.Context,
	binary string,
	config string,
	dataDir string,
	stdout io.Writer,
	stderr io.Writer,
) error {
	command := exec.CommandContext(ctx, binary, "check", "-c", config, "-D", dataDir, "--disable-color")
	command.Stdout = stdout
	command.Stderr = stderr
	if err := command.Run(); err != nil {
		return fmt.Errorf("sing-box configuration validation failed: %w", err)
	}
	return nil
}

func (adapter *Adapter) Run(binary, config, dataDir string) core.RunSpec {
	return core.RunSpec{
		Path:       binary,
		Args:       []string{"run", "-c", config, "-D", dataDir, "--disable-color"},
		WorkingDir: dataDir,
	}
}

var _ core.Adapter = (*Adapter)(nil)
