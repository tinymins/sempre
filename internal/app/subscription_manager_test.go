package app

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func TestRenameSubscriptionProfileOnlyChangesName(t *testing.T) {
	manager := newTestManager(t)
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	current, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		t.Fatal(err)
	}
	before := *current

	renamed, err := manager.RenameSubscriptionProfile(active, "  Primary  ")
	if err != nil {
		t.Fatal(err)
	}
	if renamed.Name != "Primary" {
		t.Fatalf("renamed profile name = %q", renamed.Name)
	}
	renamed.Name = before.Name
	if !reflect.DeepEqual(renamed, before) {
		t.Fatalf("rename changed profile fields: before=%#v after=%#v", before, renamed)
	}

	stored, storedActive, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	if storedActive != active {
		t.Fatalf("active profile changed from %q to %q", active, storedActive)
	}
	profile, err := subscriptions.FindProfile(&stored, active)
	if err != nil {
		t.Fatal(err)
	}
	if profile.Name != "Primary" {
		t.Fatalf("stored profile name = %q", profile.Name)
	}
}

func TestProfileRuntimeKeyTracksOnlyRuntimePlan(t *testing.T) {
	profile := subscriptions.NewProfile("runtime")
	original := profileRuntimeKey(profile)
	profile.Remark = "does not affect runtime"
	if profileRuntimeKey(profile) != original {
		t.Fatal("remark changed the runtime key")
	}
	profile.LocalProxy.SOCKSPort++
	if profileRuntimeKey(profile) == original {
		t.Fatal("local proxy change did not change the runtime key")
	}
	profile.LocalProxy.SOCKSPort--
	profile.TransparentProxy.CaptureHost = true
	if profileRuntimeKey(profile) == original {
		t.Fatal("transparent proxy change did not change the runtime key")
	}
	transparentKey := profileRuntimeKey(profile)
	profile.ManagementAPI = subscriptions.ManagementAPIConfig{Enabled: true, ExternalController: "127.0.0.1:9090", Secret: "secret", AllowOrigins: []string{}}
	if profileRuntimeKey(profile) == transparentKey {
		t.Fatal("management API change did not change the runtime key")
	}
}

func TestManagementAPIChangeStagesRuntimeWithoutChangingCoreConfig(t *testing.T) {
	manager := newTestManager(t)
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		t.Fatal(err)
	}
	profile.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: testSubscription}}
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, *profile); err != nil {
		t.Fatal(err)
	}
	before, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}

	catalog, _, _, _, err = manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, err = subscriptions.FindProfile(&catalog, active)
	if err != nil {
		t.Fatal(err)
	}
	profile.ManagementAPI = subscriptions.ManagementAPIConfig{
		Enabled: true, ExternalController: "127.0.0.1:9090", Secret: "fixed-secret", AllowOrigins: []string{},
	}
	change, _, err := manager.SaveSubscriptionProfile(context.Background(), active, *profile)
	if err != nil {
		t.Fatal(err)
	}
	after, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed || !change.NeedsRestart {
		t.Fatalf("management API change was not staged: %#v", change)
	}
	if before.Configs["sing-box"] != after.Configs["sing-box"] {
		t.Fatalf("management API changed core config hash: before=%q after=%q", before.Configs["sing-box"], after.Configs["sing-box"])
	}
	if before.ConfigBuilds["sing-box"].RuntimeKey == after.ConfigBuilds["sing-box"].RuntimeKey {
		t.Fatal("management API did not change the runtime key")
	}
}

func TestRenameSubscriptionProfileRejectsInvalidNames(t *testing.T) {
	manager := newTestManager(t)
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	secondary, err := manager.CreateSubscriptionProfile("Secondary")
	if err != nil {
		t.Fatal(err)
	}
	for _, test := range []struct {
		name    string
		id      string
		value   string
		message string
	}{
		{name: "empty", id: active, value: "  ", message: "profile name is required"},
		{name: "duplicate", id: active, value: " secondary ", message: "already used"},
		{name: "missing", id: "missing", value: "Renamed", message: "was not found"},
	} {
		t.Run(test.name, func(t *testing.T) {
			_, renameErr := manager.RenameSubscriptionProfile(test.id, test.value)
			if renameErr == nil || !strings.Contains(renameErr.Error(), test.message) {
				t.Fatalf("rename error = %v, want message containing %q", renameErr, test.message)
			}
		})
	}
	stored, _, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	if len(stored.Profiles) != len(catalog.Profiles)+1 || stored.Profiles[1].ID != secondary.ID || stored.Profiles[1].Name != "Secondary" {
		t.Fatalf("failed rename changed catalog: %#v", stored.Profiles)
	}
}

func TestSubscriptionProfileAndCustomNodeConstraints(t *testing.T) {
	manager := newTestManager(t)
	created, err := manager.CreateSubscriptionProfile("secondary")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := manager.RemoveSubscriptionProfile(created.ID); err != nil {
		t.Fatal(err)
	}
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	if _, err := manager.RemoveSubscriptionProfile(active); err == nil {
		t.Fatal("active profile was removed")
	}

	node, err := manager.SaveCustomNode(subscriptions.CustomNode{Name: "edge", Proxy: map[string]any{"name": "edge", "type": "socks5", "server": "edge.example.com", "port": 1080}})
	if err != nil {
		t.Fatal(err)
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		t.Fatal(err)
	}
	candidate := *profile
	candidate.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: testSubscription}}
	candidate.CustomNodeIDs = []string{node.ID}
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, candidate); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.RemoveCustomNode(node.ID); err == nil {
		t.Fatal("referenced custom node was removed")
	}
	node.Name = "renamed"
	saved, err := manager.SaveCustomNode(node)
	if err != nil {
		t.Fatal(err)
	}
	if saved.Proxy["name"] != "renamed" {
		t.Fatalf("custom node names diverged: %#v", saved)
	}
	node.ID = subscriptions.NewID()
	if _, err := manager.SaveCustomNode(node); err == nil {
		t.Fatal("updating a missing custom node created it")
	}
}

func TestActiveProfileCanBeSavedBeforeCoreSelection(t *testing.T) {
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Selected = nil
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		t.Fatal(err)
	}
	candidate := *profile
	candidate.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: testSubscription}}
	change, result, err := manager.SaveSubscriptionProfile(context.Background(), active, candidate)
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed || change.NeedsRestart || result.RuntimeValidated {
		t.Fatalf("change = %#v, render = %#v", change, result)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Active != nil || document.Pending {
		t.Fatalf("configuration was staged without a core: %#v", document)
	}
	stored, _, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	saved, err := subscriptions.FindProfile(&stored, active)
	if err != nil {
		t.Fatal(err)
	}
	if saved.LastConfigHash != "" || result.Content != "" {
		t.Fatalf("profile was compiled without a selected core: profile = %#v, render = %#v", saved, result)
	}
	if len(saved.Sources) != 1 || saved.LastResult != "profile saved; select a core to compile a runtime configuration" {
		t.Fatalf("profile was not persisted before core selection: %#v", saved)
	}
}

func TestSubscriptionSaveRejectsStaleConfigurationContext(t *testing.T) {
	manager := newTestManager(t)
	configurationContext, err := manager.SubscriptionConfigurationContext()
	if err != nil {
		t.Fatal(err)
	}
	if configurationContext.Target == nil || configurationContext.Key == "" {
		t.Fatalf("configuration context = %#v", configurationContext)
	}
	if err := manager.store.Update(func(document *state.Document) error {
		document.Selected = nil
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		t.Fatal(err)
	}
	profile.Remark = "stale edit"
	_, _, err = manager.SaveSubscriptionProfileForContext(context.Background(), active, *profile, configurationContext.Key)
	if !errors.Is(err, errSubscriptionConfigurationContextChanged) {
		t.Fatalf("save error = %v", err)
	}
	stored, _, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	unchanged, _ := subscriptions.FindProfile(&stored, active)
	if unchanged.Remark == "stale edit" {
		t.Fatal("stale configuration context changed the profile")
	}
}

func TestScheduledSubscriptionUpdatePreservesLinuxRuntimeSettings(t *testing.T) {
	manager := newTestManager(t)
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		t.Fatal(err)
	}
	candidate := *profile
	candidate.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: testSubscription}}
	candidate.TransparentProxy = subscriptions.TransparentProxyConfig{
		Mode:                   subscriptions.TransparentProxyTUN,
		InterfaceMode:          "all",
		Interfaces:             []string{},
		RouteExclusions:        []string{"10.10.10.0/24", "10.23.0.0/21"},
		AutoExcludeLocalRoutes: true,
		AutoExcludeVPNRoutes:   true,
		CaptureHost:            true,
		LANInterfaces:          []string{"vmbr1"},
		TUN: subscriptions.TUNConfig{
			InterfaceName: "sempre-tun", Address: "172.30.0.1/30",
		},
		TProxy: subscriptions.TProxyConfig{
			ListenPort: 17893, DNSListenPort: 11053,
		},
		EBPF: subscriptions.EBPFConfig{WANInterface: "auto"},
	}
	candidate.ManagementAPI = subscriptions.ManagementAPIConfig{
		Enabled: true, ExternalController: "127.0.0.1:9090", Secret: "fixed-secret",
		ExternalUI: "/srv/metacubex", AllowOrigins: []string{"https://dashboard.example"}, AllowPrivateNetwork: true,
	}
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, candidate); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.UpdateSubscription(context.Background()); err != nil {
		t.Fatal(err)
	}

	updatedCatalog, _, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	updated, err := subscriptions.FindProfile(&updatedCatalog, active)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(updated.TransparentProxy, candidate.TransparentProxy) {
		t.Fatalf("transparent proxy settings changed during update: got %#v, want %#v", updated.TransparentProxy, candidate.TransparentProxy)
	}
	if !reflect.DeepEqual(updated.ManagementAPI, candidate.ManagementAPI) {
		t.Fatalf("external management API settings changed during update: got %#v, want %#v", updated.ManagementAPI, candidate.ManagementAPI)
	}
}

func TestEmptyActiveProfileSaveRetainsCompiledConfiguration(t *testing.T) {
	manager := newTestManager(t)
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		t.Fatal(err)
	}
	withSource := *profile
	withSource.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: testSubscription}}
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, withSource); err != nil {
		t.Fatal(err)
	}
	before, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	empty := withSource
	empty.Sources = []subscriptions.Source{}
	change, result, err := manager.SaveSubscriptionProfile(context.Background(), active, empty)
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed || result.Content != "" {
		t.Fatalf("change = %#v, result = %#v", change, result)
	}
	after, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if after.Configs["sing-box"] != before.Configs["sing-box"] || after.Pending != before.Pending {
		t.Fatalf("empty profile changed the active configuration: before=%#v after=%#v", before, after)
	}
}

func TestSharedActiveProfileRecompilesStalePerCoreConfigurations(t *testing.T) {
	manager := newTestManager(t)
	mihomo := fakeMihomoAdapter{}
	manager.registry = core.NewRegistry(fakeAdapter{}, mihomo)
	mihomoDirectory := manager.paths.CoreVersionDir("mihomo", "", "1.2.3")
	if err := os.MkdirAll(mihomoDirectory, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(coreBinaryPath(manager.paths, mihomo, "", "1.2.3"), []byte("fake"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := manager.store.Update(func(document *state.Document) error {
		source := document.Core("mihomo").Source("")
		source.Channels["stable"] = "1.2.3"
		source.Installed["1.2.3"] = &state.Installation{}
		return nil
	}); err != nil {
		t.Fatal(err)
	}

	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		t.Fatal(err)
	}
	candidate := *profile
	candidate.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: testSubscription}}
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, candidate); err != nil {
		t.Fatal(err)
	}
	afterSingBox, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	singBoxBuild := afterSingBox.ConfigBuilds["sing-box"]
	if singBoxBuild.ProfileID != active || singBoxBuild.TargetKey != "sing-box-v13|13|default" {
		t.Fatalf("sing-box build = %#v", singBoxBuild)
	}

	if _, err := manager.UseCore(context.Background(), "mihomo@stable"); err != nil {
		t.Fatal(err)
	}
	afterMihomo, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	mihomoBuild := afterMihomo.ConfigBuilds["mihomo"]
	if mihomoBuild.ProfileRevision != singBoxBuild.ProfileRevision || mihomoBuild.TargetKey != "clash-meta||" {
		t.Fatalf("mihomo build = %#v, sing-box build = %#v", mihomoBuild, singBoxBuild)
	}
	if afterMihomo.Configs["mihomo"] == "" || afterMihomo.Configs["mihomo"] == afterMihomo.Configs["sing-box"] {
		t.Fatalf("per-core configurations = %#v", afterMihomo.Configs)
	}
	mihomoConfig, err := os.ReadFile(manager.paths.Config("mihomo", afterMihomo.Configs["mihomo"]))
	if err != nil || !strings.Contains(string(mihomoConfig), "proxies:") {
		t.Fatalf("mihomo configuration = %q, %v", mihomoConfig, err)
	}

	updatedCatalog, _, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	updatedProfile, err := subscriptions.FindProfile(&updatedCatalog, active)
	if err != nil {
		t.Fatal(err)
	}
	edited := *updatedProfile
	edited.Remark = "revision changed"
	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, edited); err != nil {
		t.Fatal(err)
	}
	staleSingBox, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if staleSingBox.ConfigBuilds["mihomo"].ProfileRevision <= staleSingBox.ConfigBuilds["sing-box"].ProfileRevision {
		t.Fatalf("build revisions were not separated: %#v", staleSingBox.ConfigBuilds)
	}

	if _, err := manager.UseCore(context.Background(), "sing-box@stable"); err != nil {
		t.Fatal(err)
	}
	final, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if final.ConfigBuilds["sing-box"].ProfileRevision != final.ConfigBuilds["mihomo"].ProfileRevision || final.ConfigBuilds["sing-box"].TargetKey != "sing-box-v13|13|default" {
		t.Fatalf("final builds = %#v", final.ConfigBuilds)
	}
	if filepath.Base(coreBinaryPath(manager.paths, mihomo, "", "1.2.3")) == "mihomo" {
		t.Fatal("test did not exercise an adapter-specific executable name")
	}
}

func TestCoreSelectionRollsBackWhenActiveProfileCannotCompile(t *testing.T) {
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Selected = nil
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	if err := manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
		profile, findErr := subscriptions.FindProfile(catalog, active)
		if findErr != nil {
			return findErr
		}
		profile.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: "not a subscription"}}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.UseCore(context.Background(), "sing-box@1.2.3"); err == nil {
		t.Fatal("invalid active profile did not reject core selection")
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Selected != nil {
		t.Fatalf("failed core selection was retained: %#v; catalog=%#v", document.Selected, catalog)
	}
}
