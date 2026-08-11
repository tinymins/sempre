package core

import (
	"fmt"
	"sort"
)

type AutoConfigContext struct {
	Target Target
}

type AutoConfigCandidate struct {
	ID                string   `json:"id"`
	Core              string   `json:"core"`
	Reference         string   `json:"reference"`
	ConfigurationMode string   `json:"configuration_mode"`
	Score             int      `json:"score"`
	Reasons           []string `json:"reasons"`
	Warnings          []string `json:"warnings,omitempty"`
}

type AutoConfigProvider interface {
	AutoConfigCandidates(AutoConfigContext) []AutoConfigCandidate
}

func (registry *Registry) AutoConfigCandidates(context AutoConfigContext) ([]AutoConfigCandidate, error) {
	result := []AutoConfigCandidate{}
	identifiers := map[string]struct{}{}
	for _, coreID := range registry.IDs() {
		adapter := registry.adapters[coreID]
		provider, ok := adapter.(AutoConfigProvider)
		if !ok {
			continue
		}
		for _, candidate := range provider.AutoConfigCandidates(context) {
			if candidate.ID == "" {
				return nil, fmt.Errorf("core %s registered an automatic configuration candidate without an ID", coreID)
			}
			if _, exists := identifiers[candidate.ID]; exists {
				return nil, fmt.Errorf("automatic configuration candidate %q is registered more than once", candidate.ID)
			}
			reference, err := ParseRef(candidate.Reference)
			if err != nil {
				return nil, fmt.Errorf("automatic configuration candidate %q: %w", candidate.ID, err)
			}
			if reference.Core != coreID {
				return nil, fmt.Errorf("automatic configuration candidate %q belongs to %s, not %s", candidate.ID, reference.Core, coreID)
			}
			candidate.Core = coreID
			candidate.Reasons = append([]string{}, candidate.Reasons...)
			candidate.Warnings = append([]string{}, candidate.Warnings...)
			identifiers[candidate.ID] = struct{}{}
			result = append(result, candidate)
		}
	}
	sort.Slice(result, func(left, right int) bool {
		if result[left].Score != result[right].Score {
			return result[left].Score > result[right].Score
		}
		return result[left].ID < result[right].ID
	})
	return result, nil
}
