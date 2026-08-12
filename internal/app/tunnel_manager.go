package app

import (
	"context"
	"fmt"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"github.com/tinymins/sempre/internal/tunnel"
)

func (manager *Manager) UpdateTunnels(ctx context.Context, config tunnel.Config) (tunnel.Config, bool, error) {
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return tunnel.Config{}, false, err
	}
	used := referencedTunnelForwards(catalog)
	for id, profiles := range used {
		if _, found := config.Forward(id); !found {
			return tunnel.Config{}, false, fmt.Errorf("tunnel forward %q is referenced by subscription profile %q", id, profiles[0])
		}
	}
	saved, err := manager.tunnels.Update(config)
	if err != nil {
		return tunnel.Config{}, false, err
	}
	if len(used) == 0 {
		return saved, false, nil
	}
	if err := manager.subscriptions.Update(func(stored *subscriptions.Catalog) error {
		for index := range stored.Profiles {
			if profileUsesTunnel(stored.Profiles[index]) {
				stored.Profiles[index].Revision++
			}
		}
		return nil
	}); err != nil {
		return saved, false, err
	}
	catalog, profile, document, err := manager.activeProfile()
	if err != nil || !profileUsesTunnel(*profile) || document.Selected == nil {
		return saved, false, err
	}
	change, err := manager.compileActiveProfileForSelectedCore(ctx, catalog, *profile, document)
	if err != nil {
		return saved, false, fmt.Errorf("tunnels saved but active profile could not be recompiled: %w", err)
	}
	if change.NeedsRestart {
		manager.RequestReload()
	}
	return saved, change.NeedsRestart, nil
}

func referencedTunnelForwards(catalog subscriptions.Catalog) map[string][]string {
	result := map[string][]string{}
	for _, profile := range catalog.Profiles {
		for _, id := range profileTunnelForwardIDs(profile) {
			result[id] = append(result[id], profile.Name)
		}
	}
	return result
}

func profileUsesTunnel(profile subscriptions.Profile) bool {
	return len(profileTunnelForwardIDs(profile)) > 0
}

func profileTunnelForwardIDs(profile subscriptions.Profile) []string {
	connectors, _ := profile.PrivateAccess["connectors"].([]any)
	result := []string{}
	for _, item := range connectors {
		connector, ok := item.(map[string]any)
		if !ok {
			continue
		}
		id, _ := connector["tunnel_forward_id"].(string)
		if id == "" {
			id, _ = connector["tunnelForwardId"].(string)
		}
		if id != "" {
			result = append(result, id)
		}
	}
	return result
}
