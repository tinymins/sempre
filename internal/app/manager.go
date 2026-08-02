package app

import (
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/core/singbox"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/state"
)

type Change struct {
	Changed        bool
	NeedsRestart   bool
	Message        string
	PreviousDetail string
	CurrentDetail  string
}

func (manager *Manager) validateConfiguration(
	ctx context.Context,
	adapter core.Adapter,
	binary string,
	config string,
	output io.Writer,
	errorOutput io.Writer,
) error {
	directory, err := os.MkdirTemp(manager.paths.Runtime, "validate-*")
	if err != nil {
		return fmt.Errorf("create validation directory: %w", err)
	}
	defer os.RemoveAll(directory)
	dataDirectory := filepath.Join(directory, "data")
	if err := os.MkdirAll(dataDirectory, 0o700); err != nil {
		return err
	}
	return adapter.Validate(ctx, binary, config, dataDirectory, output, errorOutput)
}

type Manager struct {
	paths    layout.Layout
	store    *state.Store
	registry *core.Registry
	output   io.Writer
	errors   io.Writer
	service  service.Controller
}

func New(paths layout.Layout, output, errorOutput io.Writer) (*Manager, error) {
	store := state.New(paths)
	if err := store.Initialize(); err != nil {
		return nil, err
	}
	return &Manager{
		paths:    paths,
		store:    store,
		registry: core.NewRegistry(singbox.New()),
		output:   output,
		errors:   errorOutput,
		service:  service.New(),
	}, nil
}

func (manager *Manager) Paths() layout.Layout {
	return manager.paths
}

func (manager *Manager) State() (state.Document, error) {
	return manager.store.Read()
}

func (manager *Manager) CoreIDs() []string {
	ids := manager.registry.IDs()
	sort.Strings(ids)
	return ids
}

func acquireOperationLocks(paths ...layout.Layout) (func(), error) {
	type candidate struct {
		key   string
		paths layout.Layout
	}
	byKey := map[string]layout.Layout{}
	for _, item := range paths {
		key := filepath.Clean(item.OperationLock)
		if runtime.GOOS == "windows" {
			key = strings.ToLower(key)
		}
		byKey[key] = item
	}
	candidates := make([]candidate, 0, len(byKey))
	for key, item := range byKey {
		candidates = append(candidates, candidate{key: key, paths: item})
	}
	sort.Slice(candidates, func(left, right int) bool {
		return candidates[left].key < candidates[right].key
	})

	leases := make([]*state.Lease, 0, len(candidates))
	release := func() {
		for index := len(leases) - 1; index >= 0; index-- {
			leases[index].Release()
		}
	}
	for _, item := range candidates {
		lease, err := state.New(item.paths).AcquireOperation()
		if err != nil {
			release()
			return nil, err
		}
		leases = append(leases, lease)
	}
	return release, nil
}

func (manager *Manager) withOperation(action func() error) error {
	release, err := acquireOperationLocks(manager.paths)
	if err != nil {
		return err
	}
	defer release()
	return action()
}

func (manager *Manager) withSystemOperation(action func() error) error {
	systemPaths, err := layout.ForMode(layout.System)
	if err != nil {
		return err
	}
	release, err := acquireOperationLocks(manager.paths, systemPaths)
	if err != nil {
		return err
	}
	defer release()
	return action()
}

func (manager *Manager) active(document state.Document) (state.Deployment, core.Adapter, error) {
	if document.Active == nil {
		return state.Deployment{}, nil, fmt.Errorf("no core is selected; install and use a core first")
	}
	adapter, err := manager.registry.Get(document.Active.Core)
	if err != nil {
		return state.Deployment{}, nil, err
	}
	return *document.Active, adapter, nil
}

func (manager *Manager) configurationTarget(document state.Document) (state.Deployment, core.Adapter, error) {
	if document.Selected == nil {
		return state.Deployment{}, nil, fmt.Errorf("no core is selected; run 'sempre core use <core@version>' first")
	}
	selection := document.Selected
	coreState := document.Cores[selection.Core]
	if coreState == nil {
		return state.Deployment{}, nil, fmt.Errorf("%s is not installed", selection.Core)
	}
	version := selection.Ref
	if selection.Ref == core.Stable {
		version = coreState.Channels[selection.Ref]
	}
	if version == "" || coreState.Installed[version] == nil {
		return state.Deployment{}, nil, fmt.Errorf("%s@%s is not installed", selection.Core, selection.Ref)
	}
	adapter, err := manager.registry.Get(selection.Core)
	return state.Deployment{
		Core:       selection.Core,
		Ref:        selection.Ref,
		Version:    version,
		ConfigHash: document.Configs[selection.Core],
	}, adapter, err
}
