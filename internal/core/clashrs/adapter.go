package clashrs

import (
	"context"
	"fmt"
	"io"
	"os/exec"
	"regexp"
	"strings"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
)

const repository = "Watfaq/clash-rs"

var versionPattern = regexp.MustCompile(`(?mi)clash-rs\s+v?([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?)\b`)

type Adapter struct {
	releases core.ReleaseResolver
}

func New() *Adapter { return &Adapter{releases: release.NewClient()} }

func (adapter *Adapter) ID() string                { return "clash-rs" }
func (adapter *Adapter) DefaultRepository() string { return repository }
func (adapter *Adapter) Stability() string         { return core.StabilityExperimental }

func (adapter *Adapter) Definition() core.Definition {
	return core.Definition{
		ID: adapter.ID(), Name: "clash-rs", Stability: core.StabilityExperimental,
		CompilerFormat: "clash-rs", ControlProtocol: core.ControlProtocolClashREST,
		Platforms: []string{"darwin/amd64", "darwin/arm64", "linux/amd64", "linux/arm64", "windows/amd64", "windows/arm64"},
	}
}

func (adapter *Adapter) Capabilities(_ string, target core.Target) core.Capabilities {
	features := []string{
		core.CapabilityLoggingLevel,
		core.CapabilityDNSLocalUpstream, core.CapabilityDNSRemoteUpstream, core.CapabilityDNSRemotePort, core.CapabilityDNSBootstrapUpstream,
		core.CapabilityDNSFakeIP, core.CapabilityDNSRemoteDetour, core.CapabilityDNSRejectHTTPS,
		core.CapabilityDNSSplit, core.CapabilityDNSNative,
		core.CapabilityRoutingRules, core.CapabilityRoutingRuleProviders,
		core.CapabilityRoutingSelector, core.CapabilityRoutingURLTest,
		core.CapabilityLocalProxy,
		core.CapabilityManagementConnections, core.CapabilityManagementSelectors,
		core.CapabilityManagementDelay, core.CapabilityManagementTraffic,
		core.CapabilityManagementExternalAPI, core.CapabilityNativeOverride,
	}
	if target.OS != "windows" {
		features = append(features, core.CapabilityTransparentTUN, core.CapabilityTransparentTUNAddress)
	}
	if target.OS == "linux" || target.OS == "" {
		features = append(features, core.CapabilityTransparentTProxy)
	}
	return core.Capabilities{
		Features: features,
		EnumValues: map[string][]string{
			"proxy_group.type": {"select", "url-test"}, "rule_provider.format": {"yaml", "text", "mrs"},
		},
		Protocols: []core.ProtocolCapability{
			{Protocol: "anytls", Transports: []string{"tcp"}, Security: []string{"tls"}},
			{Protocol: "hysteria2", Transports: []string{"udp"}, Security: []string{"tls"}},
			{Protocol: "shadowsocks", Transports: []string{"tcp", "udp"}, Security: []string{"cipher"}},
			{Protocol: "socks5", Transports: []string{"tcp", "udp"}, Security: []string{"none"}},
			{Protocol: "trojan", Transports: []string{"tcp", "ws", "grpc"}, Security: []string{"tls"}},
			{Protocol: "tuic", Transports: []string{"udp"}, Security: []string{"tls"}},
			{Protocol: "vless", Transports: []string{"tcp", "ws", "http", "grpc"}, Security: []string{"none", "tls", "reality"}},
			{Protocol: "vmess", Transports: []string{"tcp", "ws", "http", "grpc"}, Security: []string{"none", "tls"}},
		},
	}
}

func (adapter *Adapter) Resolve(ctx context.Context, source, reference string, target core.Target) (core.Package, error) {
	if err := validateTarget(target); err != nil {
		return core.Package{}, err
	}
	return core.ResolveGitHubPackage(ctx, adapter.releases, repository, source, reference, func(string) (core.AssetSelection, error) {
		return core.AssetSelection{Names: []string{assetName(target)}, Format: "raw"}, nil
	})
}

func validateTarget(target core.Target) error {
	if target.OS != "windows" && target.OS != "linux" && target.OS != "darwin" {
		return fmt.Errorf("clash-rs does not support target OS %q", target.OS)
	}
	if target.Arch != "amd64" && target.Arch != "arm64" {
		return fmt.Errorf("clash-rs does not support target architecture %q", target.Arch)
	}
	return nil
}

func assetName(target core.Target) string {
	arch := map[string]string{"amd64": "x86_64", "arm64": "aarch64"}[target.Arch]
	platform := map[string]string{"darwin": "apple-darwin", "linux": "unknown-linux-gnu", "windows": "pc-windows-msvc"}[target.OS]
	name := "clash-rs-" + arch + "-" + platform
	if target.OS == "windows" {
		name += ".exe"
	}
	return name
}

func (adapter *Adapter) ExecutableName(target core.Target) string {
	if target.OS == "windows" {
		return "clash-rs.exe"
	}
	return "clash-rs"
}

func (adapter *Adapter) Version(ctx context.Context, binary string) (string, error) {
	output, err := exec.CommandContext(ctx, binary, "--version").CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("run clash-rs version: %w: %s", err, strings.TrimSpace(string(output)))
	}
	match := versionPattern.FindStringSubmatch(string(output))
	if len(match) != 2 {
		return "", fmt.Errorf("clash-rs returned unrecognized version output %q", strings.TrimSpace(string(output)))
	}
	return match[1], nil
}

func (adapter *Adapter) CompilerTarget(string, core.Target) (core.CompilerTarget, error) {
	return core.CompilerTarget{Format: "clash-rs", Platform: "default"}, nil
}

func (adapter *Adapter) Validate(ctx context.Context, binary, config, dataDir string, stdout, stderr io.Writer) error {
	command := exec.CommandContext(ctx, binary, "--compatibility", "--test-config", "--config", config, "--directory", dataDir)
	command.Stdout, command.Stderr = stdout, stderr
	if err := command.Run(); err != nil {
		return fmt.Errorf("clash-rs configuration validation failed: %w", err)
	}
	return nil
}

func (adapter *Adapter) Run(binary, config, dataDir string) core.RunSpec {
	return core.RunSpec{Path: binary, Args: []string{"--compatibility", "--config", config, "--directory", dataDir}, WorkingDir: dataDir}
}

func (adapter *Adapter) PrepareRuntime(config, runtimeDirectory string) (core.RuntimeSpec, error) {
	return core.PrepareClashYAMLRuntime(adapter.ID(), config, runtimeDirectory)
}

var _ core.Adapter = (*Adapter)(nil)
var _ core.CapabilityProvider = (*Adapter)(nil)
var _ core.DefinitionProvider = (*Adapter)(nil)
var _ core.RuntimePreparer = (*Adapter)(nil)
