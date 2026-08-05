package core

import (
	"context"
	"fmt"
	"io"
	"regexp"
	"runtime"
	"strings"

	"github.com/klauspost/cpuid/v2"
)

const Stable = "stable"

var (
	namePattern       = regexp.MustCompile(`^[a-z0-9][a-z0-9-]*$`)
	repositoryPattern = regexp.MustCompile(`^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})/[A-Za-z0-9_.-]{1,100}$`)
	versionPattern    = regexp.MustCompile(`^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$`)
)

type Ref struct {
	Core       string
	Repository string
	Value      string
}

func ParseRef(value string) (Ref, error) {
	value = strings.TrimSpace(value)
	if strings.Count(value, "@") > 1 {
		return Ref{}, fmt.Errorf("invalid core reference %q", value)
	}
	source, reference, found := strings.Cut(value, "@")
	if strings.Count(source, ":") > 1 {
		return Ref{}, fmt.Errorf("invalid core source %q", source)
	}
	name, repository, hasRepository := strings.Cut(source, ":")
	if !namePattern.MatchString(name) {
		return Ref{}, fmt.Errorf("invalid core name %q", name)
	}
	if hasRepository {
		if !validRepository(repository) {
			return Ref{}, fmt.Errorf("invalid GitHub repository %q; expected owner/repository", repository)
		}
		repository = strings.ToLower(repository)
	}
	if !found {
		reference = Stable
	}
	if reference == "" {
		return Ref{}, fmt.Errorf("core reference cannot be empty")
	}
	if reference != Stable && !versionPattern.MatchString(strings.TrimPrefix(reference, "v")) {
		return Ref{}, fmt.Errorf("invalid core version or channel %q", reference)
	}
	return Ref{Core: name, Repository: repository, Value: strings.TrimPrefix(reference, "v")}, nil
}

func validRepository(repository string) bool {
	if !repositoryPattern.MatchString(repository) {
		return false
	}
	_, name, _ := strings.Cut(repository, "/")
	return name != "." && name != ".."
}

func (ref Ref) String() string {
	source := ref.Core
	if ref.Repository != "" {
		source += ":" + ref.Repository
	}
	return source + "@" + ref.Value
}

func (ref Ref) IsChannel() bool {
	return ref.Value == Stable
}

type Target struct {
	OS         string
	Arch       string
	AMD64Level int
}

func CurrentTarget() Target {
	target := Target{OS: runtime.GOOS, Arch: runtime.GOARCH}
	if target.Arch == "amd64" {
		target.AMD64Level = normalizeAMD64Level(cpuid.CPU.X64Level())
	}
	return target
}

func normalizeAMD64Level(level int) int {
	if level < 1 {
		return 0
	}
	if level > 3 {
		return 3
	}
	return level
}

type Package struct {
	Version string
	Name    string
	URL     string
	Digest  string
	Size    int64
	Format  string
}

type RunSpec struct {
	Path       string
	Args       []string
	WorkingDir string
}

type ControlSpec struct {
	Core     string
	Protocol string
	BaseURL  string
	Secret   string
}

const (
	ControlProtocolClashREST = "clash-rest"
	ControlProtocolGRPC      = "grpc"
)

type RuntimeSpec struct {
	Config  string
	Control ControlSpec
}

type CompilerTarget struct {
	Format   string
	Version  string
	Platform string
	Warnings []string
}

type RuntimePreparer interface {
	PrepareRuntime(string, string) (RuntimeSpec, error)
}

type Adapter interface {
	ID() string
	DefaultRepository() string
	Resolve(context.Context, string, string, Target) (Package, error)
	ExecutableName(Target) string
	Version(context.Context, string) (string, error)
	CompilerTarget(string, Target) (CompilerTarget, error)
	Validate(context.Context, string, string, string, io.Writer, io.Writer) error
	Run(string, string, string) RunSpec
}

type Registry struct {
	adapters map[string]Adapter
}

func NewRegistry(adapters ...Adapter) *Registry {
	registry := &Registry{adapters: map[string]Adapter{}}
	for _, adapter := range adapters {
		registry.adapters[adapter.ID()] = adapter
	}
	return registry
}

func (registry *Registry) Get(name string) (Adapter, error) {
	adapter := registry.adapters[name]
	if adapter == nil {
		return nil, fmt.Errorf("core %q is not supported", name)
	}
	return adapter, nil
}

func (registry *Registry) IDs() []string {
	result := make([]string, 0, len(registry.adapters))
	for name := range registry.adapters {
		result = append(result, name)
	}
	return result
}

func (registry *Registry) Capabilities(adapter Adapter, version string, target Target) Capabilities {
	provider, ok := adapter.(CapabilityProvider)
	if !ok {
		return NormalizeCapabilities(Capabilities{})
	}
	return NormalizeCapabilities(provider.Capabilities(version, target))
}

func (registry *Registry) StableCapabilities(target Target) Capabilities {
	values := []Capabilities{}
	for _, adapter := range registry.adapters {
		provider, ok := adapter.(CapabilityProvider)
		if !ok || provider.Stability() != StabilityStable {
			continue
		}
		values = append(values, provider.Capabilities("", target))
	}
	return IntersectCapabilities(values)
}
