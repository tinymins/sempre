package app

import (
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func (manager *Manager) CustomNodes() ([]subscriptions.CustomNode, error) {
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return nil, err
	}
	sort.Slice(catalog.CustomNodes, func(i, j int) bool { return catalog.CustomNodes[i].Name < catalog.CustomNodes[j].Name })
	return catalog.CustomNodes, nil
}

func (manager *Manager) SaveCustomNode(candidate subscriptions.CustomNode) (subscriptions.CustomNode, error) {
	create := candidate.ID == ""
	if create {
		candidate.ID = subscriptions.NewID()
	}
	if candidate.Proxy == nil {
		return subscriptions.CustomNode{}, fmt.Errorf("custom node proxy is required")
	}
	candidate.Name = strings.TrimSpace(candidate.Name)
	if candidate.Name == "" {
		candidate.Name = strings.TrimSpace(stringValue(candidate.Proxy["name"]))
	}
	if candidate.Name == "" {
		return subscriptions.CustomNode{}, fmt.Errorf("custom node name is required")
	}
	candidate.Proxy["name"] = candidate.Name
	proxy, err := subscriptions.ProxyFromMap(candidate.Proxy)
	if err != nil {
		return subscriptions.CustomNode{}, err
	}
	if proxy.Port == 0 {
		return subscriptions.CustomNode{}, fmt.Errorf("custom node port must be greater than zero")
	}
	now := time.Now().UTC()
	candidate.UpdatedAt = now
	err = manager.withOperation(func() error {
		return manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
			for index, item := range catalog.CustomNodes {
				if item.ID == candidate.ID {
					candidate.CreatedAt = item.CreatedAt
					catalog.CustomNodes[index] = candidate
					incrementProfilesReferencingNode(catalog, candidate.ID)
					return nil
				}
			}
			if !create {
				return fmt.Errorf("custom node %q was not found", candidate.ID)
			}
			candidate.CreatedAt = now
			catalog.CustomNodes = append(catalog.CustomNodes, candidate)
			return nil
		})
	})
	return candidate, err
}

func (manager *Manager) RemoveCustomNode(id string) (Change, error) {
	change := Change{}
	err := manager.withOperation(func() error {
		return manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
			for _, profile := range catalog.Profiles {
				for _, nodeID := range profile.CustomNodeIDs {
					if nodeID == id {
						return fmt.Errorf("custom node is referenced by subscription profile %q", profile.Name)
					}
				}
			}
			for index, node := range catalog.CustomNodes {
				if node.ID == id {
					catalog.CustomNodes = append(catalog.CustomNodes[:index], catalog.CustomNodes[index+1:]...)
					change = Change{Changed: true, Message: "custom node removed"}
					return nil
				}
			}
			return fmt.Errorf("custom node %q was not found", id)
		})
	})
	return change, err
}

func (manager *Manager) SetSubscriptionAutoRestart(enabled bool) (Change, error) {
	change := Change{}
	err := manager.withOperation(func() error {
		return manager.store.Update(func(document *state.Document) error {
			if document.AutoRestart == enabled {
				return nil
			}
			document.AutoRestart = enabled
			change = Change{Changed: true, Message: fmt.Sprintf("subscription automatic restart set to %t", enabled)}
			return nil
		})
	})
	return change, err
}
