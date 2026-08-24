package app

import (
	"context"
	"testing"

	"github.com/tinymins/sempre/internal/core"
)

func TestDiagnoseCoreConfigurationPrefersMacOSSingBoxV12(t *testing.T) {
	manager := newTestManager(t)
	manager.registry = core.NewRegistry(singBoxAutoConfigAdapter{}, mihomoAutoConfigAdapter{})
	report, err := manager.diagnoseCoreConfiguration(core.Target{OS: "darwin", Arch: "arm64"})
	if err != nil {
		t.Fatal(err)
	}
	if report.Recommendation == nil || report.Recommendation.Reference != "sing-box@1.12.20" || report.Recommendation.ConfigurationMode != "macos-tun-real-ip" {
		t.Fatalf("recommendation = %#v", report.Recommendation)
	}
	if len(report.Candidates) != 3 || report.Checks[1].ID != "system-dns-boundary" {
		t.Fatalf("report = %#v", report)
	}
}

func TestApplyCoreConfigurationRejectsUnavailableCandidate(t *testing.T) {
	manager := newTestManager(t)
	if _, err := manager.ApplyCoreConfiguration(context.Background(), "missing"); err == nil {
		t.Fatal("missing candidate was accepted")
	}
}

func TestApplyCoreConfigurationUsesRegisteredCandidate(t *testing.T) {
	manager := newTestManager(t)
	manager.registry = core.NewRegistry(stableAutoConfigAdapter{})
	result, err := manager.ApplyCoreConfiguration(context.Background(), "sing-box/stable")
	if err != nil {
		t.Fatal(err)
	}
	if result.Recommendation.Reference != "sing-box@stable" || len(result.Changes) != 2 {
		t.Fatalf("apply result = %#v", result)
	}
}

type singBoxAutoConfigAdapter struct{ fakeAdapter }

type stableAutoConfigAdapter struct{ singBoxAutoConfigAdapter }

func (stableAutoConfigAdapter) AutoConfigCandidates(core.AutoConfigContext) []core.AutoConfigCandidate {
	return []core.AutoConfigCandidate{{ID: "sing-box/stable", Reference: "sing-box@stable", ConfigurationMode: "platform-tun", Score: 100}}
}

func (singBoxAutoConfigAdapter) Resolve(context.Context, string, string, core.Target) (core.Package, error) {
	return core.Package{Version: "1.2.3"}, nil
}

func (singBoxAutoConfigAdapter) AutoConfigCandidates(context core.AutoConfigContext) []core.AutoConfigCandidate {
	if context.Target.OS != "darwin" {
		return []core.AutoConfigCandidate{{ID: "sing-box/stable", Reference: "sing-box@stable", ConfigurationMode: "platform-tun", Score: 100}}
	}
	return []core.AutoConfigCandidate{
		{ID: "sing-box/macos-v12", Reference: "sing-box@1.12.20", ConfigurationMode: "macos-tun-real-ip", Score: 100},
		{ID: "sing-box/macos-stable", Reference: "sing-box@stable", ConfigurationMode: "macos-tun-external-dns", Score: 55},
	}
}

type mihomoAutoConfigAdapter struct{ fakeMihomoAdapter }

func (mihomoAutoConfigAdapter) AutoConfigCandidates(context core.AutoConfigContext) []core.AutoConfigCandidate {
	if context.Target.OS != "darwin" {
		return nil
	}
	return []core.AutoConfigCandidate{{ID: "mihomo/stable", Reference: "mihomo@stable", ConfigurationMode: "mihomo-tun", Score: 70}}
}
