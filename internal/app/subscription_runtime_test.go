package app

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func TestSaveSubscriptionProfileRejectsEmptyTUNInterface(t *testing.T) {
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
	candidate.TransparentProxy.TUN.InterfaceName = " "

	_, _, err = manager.SaveSubscriptionProfile(context.Background(), active, candidate)
	if err == nil {
		t.Fatal("empty TUN interface was accepted")
	}
	if !strings.Contains(err.Error(), "TUN interface name") {
		t.Fatalf("error = %v", err)
	}
}

func TestSaveSubscriptionProfileTrimsCustomTUNInterface(t *testing.T) {
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
	candidate.TransparentProxy.TUN.InterfaceName = " sing-box "

	if _, _, err := manager.SaveSubscriptionProfile(context.Background(), active, candidate); err != nil {
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
	if updated.TransparentProxy.TUN.InterfaceName != "sing-box" {
		t.Fatalf("TUN interface = %q", updated.TransparentProxy.TUN.InterfaceName)
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
