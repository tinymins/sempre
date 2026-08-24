package singbox

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
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

func (adapter *Adapter) Definition() core.Definition {
	return core.Definition{
		ID: adapter.ID(), Name: "sing-box", Stability: core.StabilityStable,
		CompilerFormat: "sing-box-v13", ControlProtocol: core.ControlProtocolClashREST,
		Platforms: []string{"darwin/amd64", "darwin/arm64", "linux/amd64", "linux/arm64", "windows/amd64", "windows/arm64"},
	}
}

func (adapter *Adapter) Stability() string {
	return core.StabilityStable
}

func (adapter *Adapter) Capabilities(version string, target core.Target) core.Capabilities {
	compilerTarget, _ := adapter.CompilerTarget(version, target)
	features := []string{
		core.CapabilityLoggingLevel,
		core.CapabilityDNSLocalUpstream, core.CapabilityDNSLocalTransport, core.CapabilityDNSGeoSources,
		core.CapabilityDNSRemoteUpstream, core.CapabilityDNSRemotePort,
		core.CapabilityDNSBootstrapUpstream,
		core.CapabilityDNSBootstrapPort, core.CapabilityDNSBootstrapServerName,
		core.CapabilityDNSRemoteServerName,
		core.CapabilityDNSRemoteDetour, core.CapabilityDNSRejectHTTPS,
		core.CapabilityDNSSplit, core.CapabilityDNSPreferIPv4,
		core.CapabilityRoutingRules, core.CapabilityRoutingRuleProviders,
		core.CapabilityRoutingSelector, core.CapabilityRoutingURLTest,
		core.CapabilityLocalProxy, core.CapabilityTransparentTUN, core.CapabilityTransparentTUNAddress,
		core.CapabilityManagementConnections, core.CapabilityManagementSelectors,
		core.CapabilityManagementDelay, core.CapabilityManagementTraffic,
		core.CapabilityManagementExternalAPI,
	}
	if policy := ResolvePlatformPolicy(compilerTarget.Version, compilerTarget.Platform); policy.FakeIP {
		features = append(features, core.CapabilityDNSFakeIP)
	}
	if target.OS == "linux" || target.OS == "" {
		features = append(features, core.CapabilityDNSSystemTakeover, core.CapabilityTransparentTProxy, core.CapabilityTransparentInterfaces)
	}
	if compilerTarget.Version != "11" {
		features = append(features, core.CapabilityPrivateAccess)
	}
	return core.Capabilities{
		Features: features,
		EnumValues: map[string][]string{
			"proxy_group.type":             {"select", "url-test"},
			"rule_provider.format":         {"yaml", "text"},
			"transparent.interface_policy": {"all", "include", "exclude"},
		},
		Protocols: []core.ProtocolCapability{
			{Protocol: "http", Transports: []string{"tcp"}, Security: []string{"none", "tls"}},
			{Protocol: "socks5", Transports: []string{"tcp", "udp"}, Security: []string{"none"}},
			{Protocol: "vmess", Transports: []string{"tcp", "ws", "http", "grpc"}, Security: []string{"none", "tls"}},
			{Protocol: "vless", Transports: []string{"tcp", "ws", "http", "grpc"}, Security: []string{"none", "tls", "reality"}},
			{Protocol: "shadowsocks", Transports: []string{"tcp", "udp"}, Security: []string{"cipher"}},
			{Protocol: "shadowtls", Transports: []string{"tcp"}, Security: []string{"tls"}},
			{Protocol: "trojan", Transports: []string{"tcp", "ws", "grpc"}, Security: []string{"tls", "reality"}},
			{Protocol: "hysteria", Transports: []string{"udp"}, Security: []string{"tls"}},
			{Protocol: "hysteria2", Transports: []string{"udp"}, Security: []string{"tls"}},
			{Protocol: "tuic", Transports: []string{"udp"}, Security: []string{"tls"}},
			{Protocol: "anytls", Transports: []string{"tcp"}, Security: []string{"tls"}, MinimumVersion: "1.12.0"},
		},
	}
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

func (adapter *Adapter) CompilerTarget(version string, target core.Target) (core.CompilerTarget, error) {
	compilerVersion, warnings := ResolveCompilerVersion(version)
	platform := "default"
	if target.OS == "windows" {
		platform = "windows"
	} else if target.OS == "darwin" {
		platform = "macos"
	}
	format := "sing-box-v" + compilerVersion
	if compilerVersion == "11" {
		format = "sing-box"
	}
	if platform != "default" {
		format += "-" + platform
	}
	return core.CompilerTarget{Format: format, Version: compilerVersion, Platform: platform, Warnings: warnings}, nil
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
	control, err := core.NewPrivateControl(adapter.ID(), core.ControlProtocolClashREST)
	if err != nil {
		return core.RuntimeSpec{}, err
	}
	address := strings.TrimPrefix(control.BaseURL, "http://")
	clashAPI["external_controller"] = address
	clashAPI["secret"] = control.Secret
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
		Config:  runtimeConfig,
		Control: control,
	}, nil
}

func object(value any) map[string]any {
	if result, ok := value.(map[string]any); ok {
		return result
	}
	return map[string]any{}
}

var _ core.Adapter = (*Adapter)(nil)
var _ core.DefinitionProvider = (*Adapter)(nil)
var _ core.RuntimePreparer = (*Adapter)(nil)
