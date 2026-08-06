package v2rayfamily

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
)

type Kind struct {
	ID         string
	Name       string
	Repository string
	Asset      string
	VersionRE  *regexp.Regexp
	Services   []string
	Features   []string
	Protocols  []core.ProtocolCapability
}

type Adapter struct {
	kind     Kind
	releases core.ReleaseResolver
}

func New(kind Kind) *Adapter {
	return &Adapter{kind: kind, releases: release.NewClient()}
}

func (adapter *Adapter) ID() string { return adapter.kind.ID }

func (adapter *Adapter) Definition() core.Definition {
	return core.Definition{
		ID: adapter.ID(), Name: adapter.kind.Name, Stability: core.StabilityStable,
		CompilerFormat: adapter.ID(), ControlProtocol: core.ControlProtocolGRPC,
		Platforms: []string{"darwin/amd64", "darwin/arm64", "linux/amd64", "linux/arm64", "windows/amd64", "windows/arm64"},
	}
}

func (adapter *Adapter) Stability() string { return core.StabilityStable }

func (adapter *Adapter) Capabilities(_ string, target core.Target) core.Capabilities {
	features := append([]string{}, adapter.kind.Features...)
	if target.OS == "linux" || target.OS == "" {
		features = append(features, core.CapabilityTransparentTProxy)
	}
	return core.Capabilities{
		Features: features,
		EnumValues: map[string][]string{
			"proxy_group.type": {"select", "url-test"},
		},
		Protocols: adapter.kind.Protocols,
	}
}

func (adapter *Adapter) DefaultRepository() string { return adapter.kind.Repository }

func (adapter *Adapter) Resolve(ctx context.Context, source, reference string, target core.Target) (core.Package, error) {
	if err := validateTarget(adapter.ID(), target); err != nil {
		return core.Package{}, err
	}
	return core.ResolveGitHubPackage(ctx, adapter.releases, adapter.kind.Repository, source, reference, func(string) (core.AssetSelection, error) {
		return core.AssetSelection{Names: []string{assetName(adapter.kind.Asset, target)}, Format: "zip"}, nil
	})
}

func validateTarget(id string, target core.Target) error {
	if target.OS != "windows" && target.OS != "linux" && target.OS != "darwin" {
		return fmt.Errorf("%s does not support target OS %q", id, target.OS)
	}
	if target.Arch != "amd64" && target.Arch != "arm64" {
		return fmt.Errorf("%s does not support target architecture %q", id, target.Arch)
	}
	return nil
}

func assetName(prefix string, target core.Target) string {
	osName := target.OS
	if osName == "darwin" {
		osName = "macos"
	}
	arch := map[string]string{"amd64": "64", "arm64": "arm64-v8a"}[target.Arch]
	return prefix + "-" + osName + "-" + arch + ".zip"
}

func (adapter *Adapter) ExecutableName(target core.Target) string {
	if target.OS == "windows" {
		return strings.ToLower(adapter.kind.Asset) + ".exe"
	}
	return strings.ToLower(adapter.kind.Asset)
}

func (adapter *Adapter) Version(ctx context.Context, binary string) (string, error) {
	output, err := exec.CommandContext(ctx, binary, "version").CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("run %s version: %w: %s", adapter.ID(), err, strings.TrimSpace(string(output)))
	}
	match := adapter.kind.VersionRE.FindStringSubmatch(string(output))
	if len(match) != 2 {
		return "", fmt.Errorf("%s returned unrecognized version output %q", adapter.ID(), strings.TrimSpace(string(output)))
	}
	return strings.TrimPrefix(match[1], "v"), nil
}

func (adapter *Adapter) CompilerTarget(_ string, target core.Target) (core.CompilerTarget, error) {
	if err := validateTarget(adapter.ID(), target); err != nil {
		return core.CompilerTarget{}, err
	}
	return core.CompilerTarget{Format: adapter.ID(), Platform: "default"}, nil
}

func (adapter *Adapter) Validate(ctx context.Context, binary, config, dataDir string, stdout, stderr io.Writer) error {
	args := []string{"test", "-c", config}
	if adapter.ID() == "xray" {
		args = []string{"run", "-test", "-config", config}
	}
	command := exec.CommandContext(ctx, binary, args...)
	command.Env = append(os.Environ(), adapter.assetEnvironment(binary))
	command.Dir, command.Stdout, command.Stderr = dataDir, stdout, stderr
	if err := command.Run(); err != nil {
		return fmt.Errorf("%s configuration validation failed: %w", adapter.ID(), err)
	}
	return nil
}

func (adapter *Adapter) Run(binary, config, dataDir string) core.RunSpec {
	args := []string{"run", "-c", config}
	if adapter.ID() == "xray" {
		args = []string{"run", "-config", config}
	}
	return core.RunSpec{Path: binary, Args: args, Env: []string{adapter.assetEnvironment(binary)}, WorkingDir: dataDir}
}

func (adapter *Adapter) assetEnvironment(binary string) string {
	return adapter.ID() + ".location.asset=" + filepath.Dir(binary)
}

func (adapter *Adapter) PrepareRuntime(config, runtimeDirectory string) (core.RuntimeSpec, error) {
	data, err := os.ReadFile(config)
	if err != nil {
		return core.RuntimeSpec{}, fmt.Errorf("read %s configuration: %w", adapter.ID(), err)
	}
	document := map[string]any{}
	if err := json.Unmarshal(data, &document); err != nil {
		return core.RuntimeSpec{}, fmt.Errorf("decode %s configuration: %w", adapter.ID(), err)
	}
	control, err := core.NewPrivateControl(adapter.ID(), core.ControlProtocolGRPC)
	if err != nil {
		return core.RuntimeSpec{}, err
	}
	hostPort := strings.TrimPrefix(control.BaseURL, "http://")
	_, portValue, err := net.SplitHostPort(hostPort)
	if err != nil {
		return core.RuntimeSpec{}, err
	}
	port, _ := strconv.Atoi(portValue)
	document["api"] = map[string]any{"tag": "sempre-api", "services": adapter.kind.Services}
	document["stats"] = map[string]any{}
	inbounds := objectSlice(document["inbounds"], "sempre-api-in")
	inbounds = append(inbounds, map[string]any{
		"tag": "sempre-api-in", "listen": "127.0.0.1", "port": port,
		"protocol": "dokodemo-door", "settings": map[string]any{"address": "127.0.0.1"},
	})
	document["inbounds"] = inbounds
	routing := object(document["routing"])
	rules := objectSlice(routing["rules"], "")
	rules = append([]any{map[string]any{"type": "field", "inboundTag": []string{"sempre-api-in"}, "outboundTag": "sempre-api"}}, rules...)
	routing["rules"] = rules
	document["routing"] = routing
	encoded, err := json.MarshalIndent(document, "", "  ")
	if err != nil {
		return core.RuntimeSpec{}, err
	}
	if err := os.MkdirAll(runtimeDirectory, 0o700); err != nil {
		return core.RuntimeSpec{}, err
	}
	runtimeConfig := filepath.Join(runtimeDirectory, "config.json")
	if err := os.WriteFile(runtimeConfig, append(encoded, '\n'), 0o600); err != nil {
		return core.RuntimeSpec{}, err
	}
	return core.RuntimeSpec{Config: runtimeConfig, Control: control}, nil
}

func object(value any) map[string]any {
	if result, ok := value.(map[string]any); ok {
		return result
	}
	return map[string]any{}
}

func objectSlice(value any, excludedTag string) []any {
	values, _ := value.([]any)
	result := make([]any, 0, len(values))
	for _, value := range values {
		item, _ := value.(map[string]any)
		if excludedTag != "" && item["tag"] == excludedTag {
			continue
		}
		result = append(result, value)
	}
	return result
}

var _ core.Adapter = (*Adapter)(nil)
var _ core.CapabilityProvider = (*Adapter)(nil)
var _ core.DefinitionProvider = (*Adapter)(nil)
var _ core.RuntimePreparer = (*Adapter)(nil)
