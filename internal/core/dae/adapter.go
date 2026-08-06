package dae

import (
	"context"
	"fmt"
	"io"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
)

const repository = "daeuniverse/dae"

var versionPattern = regexp.MustCompile(`(?m)^dae version v?([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?)\s*$`)

type Adapter struct{ releases core.ReleaseResolver }

func New() *Adapter                                { return &Adapter{releases: release.NewClient()} }
func (adapter *Adapter) ID() string                { return "dae" }
func (adapter *Adapter) Stability() string         { return core.StabilityExperimental }
func (adapter *Adapter) DefaultRepository() string { return repository }

func (adapter *Adapter) Definition() core.Definition {
	return core.Definition{ID: adapter.ID(), Name: "dae", Stability: core.StabilityExperimental, CompilerFormat: "dae", Platforms: []string{"linux/amd64", "linux/arm64"}}
}

func (adapter *Adapter) Capabilities(_ string, target core.Target) core.Capabilities {
	features := []string{}
	if target.OS == "linux" || target.OS == "" {
		features = []string{
			core.CapabilityLoggingLevel,
			core.CapabilityDNSLocalUpstream, core.CapabilityDNSRemoteUpstream, core.CapabilityDNSRemotePort, core.CapabilityDNSBootstrapUpstream,
			core.CapabilityDNSPreferIPv4, core.CapabilityDNSSplit,
			core.CapabilityRoutingRules, core.CapabilityRoutingSelector, core.CapabilityRoutingURLTest,
			core.CapabilityTransparentEBPF, core.CapabilityNativeOverride,
		}
	}
	return core.Capabilities{
		Features:   features,
		EnumValues: map[string][]string{"proxy_group.type": {"select", "url-test"}},
		Protocols: []core.ProtocolCapability{
			{Protocol: "anytls", Transports: []string{"tcp"}, Security: []string{"tls"}},
			{Protocol: "http", Transports: []string{"tcp"}, Security: []string{"none", "tls"}},
			{Protocol: "hysteria2", Transports: []string{"udp"}, Security: []string{"tls"}},
			{Protocol: "shadowsocks", Transports: []string{"tcp", "udp"}, Security: []string{"cipher"}},
			{Protocol: "socks5", Transports: []string{"tcp", "udp"}, Security: []string{"none"}},
			{Protocol: "trojan", Transports: []string{"tcp", "ws", "grpc"}, Security: []string{"tls"}},
			{Protocol: "tuic", Transports: []string{"udp"}, Security: []string{"tls"}},
			{Protocol: "vless", Transports: []string{"tcp", "ws", "grpc"}, Security: []string{"none", "tls", "reality"}},
			{Protocol: "vmess", Transports: []string{"tcp", "ws", "grpc"}, Security: []string{"none", "tls"}},
		},
	}
}

func (adapter *Adapter) Resolve(ctx context.Context, source, reference string, target core.Target) (core.Package, error) {
	if target.OS != "linux" || (target.Arch != "amd64" && target.Arch != "arm64") {
		return core.Package{}, fmt.Errorf("dae supports only linux/amd64 and linux/arm64")
	}
	return core.ResolveGitHubPackage(ctx, adapter.releases, repository, source, reference, func(string) (core.AssetSelection, error) {
		return core.AssetSelection{Names: []string{adapter.ExecutableName(target) + ".zip"}, Format: "zip"}, nil
	})
}

func (adapter *Adapter) ExecutableName(target core.Target) string {
	arch := "arm64"
	if target.Arch == "amd64" {
		arch = "x86_64"
		if target.AMD64Level >= 3 {
			arch = "x86_64_v3_avx2"
		} else if target.AMD64Level >= 2 {
			arch = "x86_64_v2_sse"
		}
	}
	return "dae-linux-" + arch
}

func (adapter *Adapter) Version(ctx context.Context, binary string) (string, error) {
	output, err := exec.CommandContext(ctx, binary, "--version").CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("run dae version: %w: %s", err, strings.TrimSpace(string(output)))
	}
	match := versionPattern.FindStringSubmatch(string(output))
	if len(match) != 2 {
		return "", fmt.Errorf("dae returned unrecognized version output %q", strings.TrimSpace(string(output)))
	}
	return match[1], nil
}

func (adapter *Adapter) CompilerTarget(string, core.Target) (core.CompilerTarget, error) {
	return core.CompilerTarget{Format: "dae", Platform: "default"}, nil
}

func (adapter *Adapter) Validate(ctx context.Context, binary, config, dataDir string, stdout, stderr io.Writer) error {
	command := exec.CommandContext(ctx, binary, "validate", "--config", config)
	command.Env = append(command.Environ(), "DAE_LOCATION_ASSET="+filepath.Dir(binary))
	command.Dir, command.Stdout, command.Stderr = dataDir, stdout, stderr
	if err := command.Run(); err != nil {
		return fmt.Errorf("dae configuration validation failed: %w", err)
	}
	return nil
}

func (adapter *Adapter) Run(binary, config, dataDir string) core.RunSpec {
	return core.RunSpec{
		Path: binary, Args: []string{"run", "--config", config, "--disable-sudo", "--disable-pidfile"},
		Env: []string{"DAE_LOCATION_ASSET=" + filepath.Dir(binary)}, WorkingDir: dataDir,
	}
}

var _ core.Adapter = (*Adapter)(nil)
var _ core.CapabilityProvider = (*Adapter)(nil)
var _ core.DefinitionProvider = (*Adapter)(nil)
