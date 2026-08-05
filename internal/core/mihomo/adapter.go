package mihomo

import (
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
	"gopkg.in/yaml.v3"
)

const repository = "MetaCubeX/mihomo"

var (
	versionLinePattern = regexp.MustCompile(`^Mihomo Meta v?([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?)(?:\s|$)`)
	digestPattern      = regexp.MustCompile(`(?i)^sha256:[0-9a-f]{64}$`)
)

type Adapter struct {
	releases releaseResolver
}

type releaseResolver interface {
	LatestStable(context.Context, string) (release.GitHubRelease, error)
	Version(context.Context, string, string) (release.GitHubRelease, error)
}

func New() *Adapter {
	return &Adapter{releases: release.NewClient()}
}

func (adapter *Adapter) ID() string {
	return "mihomo"
}

func (adapter *Adapter) DefaultRepository() string {
	return repository
}

func (adapter *Adapter) Resolve(ctx context.Context, source, reference string, target core.Target) (core.Package, error) {
	if source == "" {
		source = repository
	}
	if err := validateTarget(target); err != nil {
		return core.Package{}, err
	}
	var (
		item release.GitHubRelease
		err  error
	)
	if reference == core.Stable {
		item, err = adapter.releases.LatestStable(ctx, source)
		if err == nil && item.Prerelease {
			err = fmt.Errorf("latest release %s is a prerelease", item.Tag)
		}
	} else {
		item, err = adapter.releases.Version(ctx, source, reference)
	}
	if err != nil {
		return core.Package{}, err
	}
	version := strings.TrimPrefix(item.Tag, "v")
	format, extension := "gz", ".gz"
	if target.OS == "windows" {
		format, extension = "zip", ".zip"
	}
	candidates := assetNames(version, extension, target)
	for _, name := range candidates {
		for _, asset := range item.Assets {
			if asset.Name != name {
				continue
			}
			if !digestPattern.MatchString(strings.TrimSpace(asset.Digest)) {
				return core.Package{}, fmt.Errorf("release asset %s does not provide a valid SHA-256 digest", name)
			}
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
	return core.Package{}, fmt.Errorf(
		"release %s has no supported asset for %s/%s; tried %s",
		version,
		target.OS,
		target.Arch,
		strings.Join(candidates, ", "),
	)
}

func validateTarget(target core.Target) error {
	switch target.OS {
	case "windows", "linux", "darwin":
	default:
		return fmt.Errorf("mihomo does not support target OS %q", target.OS)
	}
	if target.Arch != "amd64" && target.Arch != "arm64" {
		return fmt.Errorf("mihomo does not support target architecture %q", target.Arch)
	}
	return nil
}

func assetNames(version, extension string, target core.Target) []string {
	prefix := fmt.Sprintf("mihomo-%s-%s", target.OS, target.Arch)
	if target.Arch == "arm64" {
		return []string{fmt.Sprintf("%s-v%s%s", prefix, version, extension)}
	}
	variants := []string{"compatible"}
	if target.AMD64Level >= 3 {
		variants = []string{"v3", "v2", "compatible"}
	} else if target.AMD64Level == 2 {
		variants = []string{"v2", "compatible"}
	}
	result := make([]string, 0, len(variants))
	for _, variant := range variants {
		result = append(result, fmt.Sprintf("%s-%s-v%s%s", prefix, variant, version, extension))
	}
	return result
}

func (adapter *Adapter) ExecutableName(target core.Target) string {
	if target.OS == "windows" {
		return "mihomo.exe"
	}
	return "mihomo"
}

func (adapter *Adapter) Version(ctx context.Context, binary string) (string, error) {
	output, err := exec.CommandContext(ctx, binary, "-v").CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("run mihomo version: %w: %s", err, strings.TrimSpace(string(output)))
	}
	return parseVersionOutput(string(output))
}

func parseVersionOutput(output string) (string, error) {
	line, _, _ := strings.Cut(strings.TrimSpace(output), "\n")
	match := versionLinePattern.FindStringSubmatch(strings.TrimSpace(line))
	if len(match) != 2 {
		return "", fmt.Errorf("mihomo returned an unrecognized version line %q", line)
	}
	return match[1], nil
}

func (adapter *Adapter) CompilerTarget(string, core.Target) (core.CompilerTarget, error) {
	return core.CompilerTarget{Format: "clash-meta"}, nil
}

func (adapter *Adapter) Validate(
	ctx context.Context,
	binary string,
	config string,
	dataDir string,
	stdout io.Writer,
	stderr io.Writer,
) error {
	command := exec.CommandContext(ctx, binary, validationArgs(config, dataDir)...)
	command.Stdout = stdout
	command.Stderr = stderr
	if err := command.Run(); err != nil {
		return fmt.Errorf("mihomo configuration validation failed: %w", err)
	}
	return nil
}

func validationArgs(config, dataDir string) []string {
	return []string{"-t", "-f", config, "-d", dataDir}
}

func (adapter *Adapter) Run(binary, config, dataDir string) core.RunSpec {
	return core.RunSpec{
		Path:       binary,
		Args:       []string{"-f", config, "-d", dataDir},
		WorkingDir: dataDir,
	}
}

func (adapter *Adapter) PrepareRuntime(config, runtimeDirectory string) (core.RuntimeSpec, error) {
	data, err := os.ReadFile(config)
	if err != nil {
		return core.RuntimeSpec{}, fmt.Errorf("read mihomo configuration: %w", err)
	}
	document := map[string]any{}
	if err := yaml.Unmarshal(data, &document); err != nil {
		return core.RuntimeSpec{}, fmt.Errorf("decode mihomo configuration: %w", err)
	}
	control, err := core.NewPrivateControl(adapter.ID())
	if err != nil {
		return core.RuntimeSpec{}, err
	}
	for _, key := range []string{
		"external-controller-tls",
		"external-controller-unix",
		"external-controller-pipe",
		"external-doh-server",
		"external-ui",
		"external-ui-name",
		"external-ui-url",
		"external-ui-headers",
	} {
		delete(document, key)
	}
	document["external-controller"] = strings.TrimPrefix(control.BaseURL, "http://")
	document["secret"] = control.Secret
	document["external-controller-cors"] = map[string]any{
		"allow-origins":         []string{"http://localhost.invalid"},
		"allow-private-network": false,
	}
	encoded, err := yaml.Marshal(document)
	if err != nil {
		return core.RuntimeSpec{}, fmt.Errorf("encode mihomo runtime configuration: %w", err)
	}
	if err := os.MkdirAll(runtimeDirectory, 0o700); err != nil {
		return core.RuntimeSpec{}, err
	}
	runtimeConfig := filepath.Join(runtimeDirectory, "config.yaml")
	if err := os.WriteFile(runtimeConfig, encoded, 0o600); err != nil {
		return core.RuntimeSpec{}, fmt.Errorf("write mihomo runtime configuration: %w", err)
	}
	return core.RuntimeSpec{Config: runtimeConfig, Control: control}, nil
}

var _ core.Adapter = (*Adapter)(nil)
var _ core.RuntimePreparer = (*Adapter)(nil)
