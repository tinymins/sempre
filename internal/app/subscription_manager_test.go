package app

import (
	"context"
	"testing"

	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

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
	compiled, err := manager.subscriptions.ReadBlob(saved.LastConfigHash)
	if err != nil {
		t.Fatal(err)
	}
	if string(compiled) != result.Content {
		t.Fatal("persisted compiled configuration does not match the render result")
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
