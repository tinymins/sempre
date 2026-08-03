package singbox

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
)

const repository = "SagerNet/sing-box"

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
	return "sing-box"
}

func (adapter *Adapter) DefaultRepository() string {
	return repository
}

func (adapter *Adapter) Resolve(ctx context.Context, source, reference string, target core.Target) (core.Package, error) {
	if source == "" {
		source = repository
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

func (adapter *Adapter) PrepareRuntime(config, runtimeDirectory string) (core.RuntimeSpec, error) {
	data, err := os.ReadFile(config)
	if err != nil {
		return core.RuntimeSpec{}, fmt.Errorf("read sing-box configuration: %w", err)
	}
	var document map[string]any
	if err := json.Unmarshal(data, &document); err != nil {
		return core.RuntimeSpec{}, fmt.Errorf("decode sing-box configuration: %w", err)
	}
	experimental := object(document["experimental"])
	clashAPI := object(experimental["clash_api"])
	address, err := availableLoopbackAddress()
	if err != nil {
		return core.RuntimeSpec{}, err
	}
	secretBytes := make([]byte, 32)
	if _, err := rand.Read(secretBytes); err != nil {
		return core.RuntimeSpec{}, fmt.Errorf("generate internal core API secret: %w", err)
	}
	secret := hex.EncodeToString(secretBytes)
	clashAPI["external_controller"] = address
	clashAPI["secret"] = secret
	clashAPI["external_ui"] = ""
	clashAPI["external_ui_download_url"] = ""
	clashAPI["external_ui_download_detour"] = ""
	clashAPI["access_control_allow_origin"] = []string{"http://localhost.invalid"}
	clashAPI["access_control_allow_private_network"] = false
	experimental["clash_api"] = clashAPI
	document["experimental"] = experimental

	runtimeConfig := filepath.Join(runtimeDirectory, "config.json")
	encoded, err := json.MarshalIndent(document, "", "  ")
	if err != nil {
		return core.RuntimeSpec{}, err
	}
	if err := os.MkdirAll(runtimeDirectory, 0o700); err != nil {
		return core.RuntimeSpec{}, err
	}
	if err := os.WriteFile(runtimeConfig, append(encoded, '\n'), 0o600); err != nil {
		return core.RuntimeSpec{}, fmt.Errorf("write sing-box runtime configuration: %w", err)
	}
	return core.RuntimeSpec{
		Config: runtimeConfig,
		Control: core.ControlSpec{
			BaseURL: "http://" + address,
			Secret:  secret,
		},
	}, nil
}

func object(value any) map[string]any {
	if result, ok := value.(map[string]any); ok {
		return result
	}
	return map[string]any{}
}

func availableLoopbackAddress() (string, error) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return "", fmt.Errorf("reserve internal core API address: %w", err)
	}
	address := listener.Addr().String()
	if err := listener.Close(); err != nil {
		return "", err
	}
	return address, nil
}

var _ core.Adapter = (*Adapter)(nil)
var _ core.RuntimePreparer = (*Adapter)(nil)
