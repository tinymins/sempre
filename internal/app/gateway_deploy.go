package app

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"

	"github.com/tinymins/sempre/internal/gateway"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
)

func (manager *Manager) stageGatewayConfig(target string, existingWins bool) (*swapOperation, error) {
	config, err := manager.gateway.Read()
	if err != nil {
		return nil, err
	}
	if existingWins {
		if _, statErr := os.Stat(target); statErr == nil {
			targetPaths := layout.At(filepath.Dir(target))
			targetPaths.GatewayConfig = target
			config, err = gateway.NewStore(targetPaths).Read()
			if err != nil {
				return nil, err
			}
		} else if !errors.Is(statErr, os.ErrNotExist) {
			return nil, statErr
		}
	}
	data, err := json.MarshalIndent(config, "", "  ")
	if err != nil {
		return nil, err
	}
	if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
		return nil, err
	}
	staging, err := unusedSibling(target, ".sempre-gateway-*")
	if err != nil {
		return nil, err
	}
	if err := state.WriteAtomic(staging, append(data, '\n'), 0o600); err != nil {
		return nil, err
	}
	return &swapOperation{staged: staging, target: target}, nil
}
