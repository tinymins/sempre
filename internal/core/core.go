package core

import (
	"context"
	"fmt"
	"io"
	"regexp"
	"runtime"
	"strings"
)

const Stable = "stable"

var (
	namePattern    = regexp.MustCompile(`^[a-z0-9][a-z0-9-]*$`)
	versionPattern = regexp.MustCompile(`^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$`)
)

type Ref struct {
	Core  string
	Value string
}

func ParseRef(value string) (Ref, error) {
	name, reference, found := strings.Cut(strings.TrimSpace(value), "@")
	if !namePattern.MatchString(name) {
		return Ref{}, fmt.Errorf("invalid core name %q", name)
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
	return Ref{Core: name, Value: strings.TrimPrefix(reference, "v")}, nil
}

func (ref Ref) String() string {
	return ref.Core + "@" + ref.Value
}

func (ref Ref) IsChannel() bool {
	return ref.Value == Stable
}

type Target struct {
	OS   string
	Arch string
}

func CurrentTarget() Target {
	return Target{OS: runtime.GOOS, Arch: runtime.GOARCH}
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

type Adapter interface {
	ID() string
	Resolve(context.Context, string, Target) (Package, error)
	ExecutableName(Target) string
	Version(context.Context, string) (string, error)
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
