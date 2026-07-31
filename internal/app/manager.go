package app

import (
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"

	"github.com/sempre-lab/sempre/internal/core"
	"github.com/sempre-lab/sempre/internal/core/singbox"
	"github.com/sempre-lab/sempre/internal/layout"
	"github.com/sempre-lab/sempre/internal/service"
	"github.com/sempre-lab/sempre/internal/state"
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
	if document.Active != nil {
		return manager.active(document)
	}
	for _, name := range manager.CoreIDs() {
		coreState := document.Cores[name]
		if coreState == nil {
			continue
		}
		if version := coreState.Channels[core.Stable]; version != "" {
			adapter, err := manager.registry.Get(name)
			return state.Deployment{Core: name, Ref: core.Stable, Version: version}, adapter, err
		}
	}
	return state.Deployment{}, nil, fmt.Errorf("no core is installed; run 'sempre core install sing-box' first")
}
