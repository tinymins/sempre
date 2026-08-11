package app

import (
	"context"
	"fmt"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

type AutoConfigCandidate struct {
	core.AutoConfigCandidate
	Installed bool `json:"installed"`
	Selected  bool `json:"selected"`
}

type AutoConfigCheck struct {
	ID     string `json:"id"`
	Status string `json:"status"`
	Detail string `json:"detail,omitempty"`
}

type AutoConfigReport struct {
	CheckedAt      time.Time             `json:"checked_at"`
	Platform       string                `json:"platform"`
	Architecture   string                `json:"architecture"`
	Recommendation *AutoConfigCandidate  `json:"recommendation,omitempty"`
	Candidates     []AutoConfigCandidate `json:"candidates"`
	Checks         []AutoConfigCheck     `json:"checks"`
}

type AutoConfigApplyResult struct {
	Recommendation AutoConfigCandidate `json:"recommendation"`
	Changes        []Change            `json:"changes"`
}

func (manager *Manager) DiagnoseCoreConfiguration() (AutoConfigReport, error) {
	return manager.diagnoseCoreConfiguration(core.CurrentTarget())
}

func (manager *Manager) diagnoseCoreConfiguration(target core.Target) (AutoConfigReport, error) {
	document, err := manager.store.Read()
	if err != nil {
		return AutoConfigReport{}, err
	}
	registered, err := manager.registry.AutoConfigCandidates(core.AutoConfigContext{Target: target})
	if err != nil {
		return AutoConfigReport{}, err
	}
	report := AutoConfigReport{
		CheckedAt: time.Now().UTC(), Platform: target.OS, Architecture: target.Arch,
		Candidates: []AutoConfigCandidate{}, Checks: []AutoConfigCheck{{ID: "platform", Status: "pass", Detail: target.OS + "/" + target.Arch}},
	}
	if target.OS == "darwin" {
		report.Checks = append(report.Checks, AutoConfigCheck{ID: "system-dns-boundary", Status: "info", Detail: "Sempre does not modify macOS system DNS"})
	}
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return AutoConfigReport{}, err
	}
	profile, profileErr := subscriptions.FindProfile(&catalog, document.ActiveProfileID)
	if profileErr != nil || !subscriptionProfileHasInputs(*profile) {
		report.Checks = append(report.Checks, AutoConfigCheck{ID: "active-profile", Status: "warning", Detail: "configure a subscription before applying the recommendation"})
	} else {
		report.Checks = append(report.Checks, AutoConfigCheck{ID: "active-profile", Status: "pass", Detail: profile.Name})
	}
	for _, candidate := range registered {
		reference, _ := core.ParseRef(candidate.Reference)
		value := AutoConfigCandidate{
			AutoConfigCandidate: candidate,
			Installed:           coreReferenceInstalled(document, reference),
			Selected:            selectionMatches(document.Selected, reference),
		}
		report.Candidates = append(report.Candidates, value)
	}
	if len(report.Candidates) > 0 {
		recommendation := report.Candidates[0]
		report.Recommendation = &recommendation
	}
	return report, nil
}

func (manager *Manager) ApplyCoreConfiguration(ctx context.Context, candidateID string) (AutoConfigApplyResult, error) {
	report, err := manager.DiagnoseCoreConfiguration()
	if err != nil {
		return AutoConfigApplyResult{}, err
	}
	if candidateID == "" && report.Recommendation != nil {
		candidateID = report.Recommendation.ID
	}
	var candidate *AutoConfigCandidate
	for index := range report.Candidates {
		if report.Candidates[index].ID == candidateID {
			candidate = &report.Candidates[index]
			break
		}
	}
	if candidate == nil {
		return AutoConfigApplyResult{}, fmt.Errorf("automatic configuration candidate %q is not available for this host", candidateID)
	}
	installed, err := manager.InstallCore(ctx, candidate.Reference)
	if err != nil {
		return AutoConfigApplyResult{}, err
	}
	selected, err := manager.UseCore(ctx, candidate.Reference)
	if err != nil {
		return AutoConfigApplyResult{}, err
	}
	return AutoConfigApplyResult{Recommendation: *candidate, Changes: []Change{installed, selected}}, nil
}

func coreReferenceInstalled(document state.Document, reference core.Ref) bool {
	coreState := document.Cores[reference.Core]
	if coreState == nil {
		return false
	}
	source := coreState.LookupSource(reference.Repository)
	if source == nil {
		return false
	}
	version := reference.Value
	if reference.IsChannel() {
		version = source.Channels[reference.Value]
	}
	return version != "" && source.Installed[version] != nil
}
