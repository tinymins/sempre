package app

import (
	"bytes"
	"testing"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func TestSubscriptionInstallationPreservesExistingCatalog(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	if _, err := source.CreateSubscriptionProfile("portable"); err != nil {
		t.Fatal(err)
	}
	target := layout.SystemAt(t.TempDir())
	targetManager, err := New(target, bytes.NewBuffer(nil), bytes.NewBuffer(nil))
	if err != nil {
		t.Fatal(err)
	}
	systemProfile, err := targetManager.CreateSubscriptionProfile("system")
	if err != nil {
		t.Fatal(err)
	}
	existing := state.NewDocument()
	existing.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
	operation, err := source.stageSubscriptionInstallation(target, existing, true)
	if err != nil {
		t.Fatal(err)
	}
	defer operation.cleanup()
	if err := operation.activate(); err != nil {
		t.Fatal(err)
	}
	if err := operation.commit(); err != nil {
		t.Fatal(err)
	}
	catalog, err := subscriptions.NewStore(target).Read()
	if err != nil {
		t.Fatal(err)
	}
	if _, err := subscriptions.FindProfile(&catalog, systemProfile.ID); err != nil {
		t.Fatalf("existing system profile was not preserved: %v", err)
	}
	for _, profile := range catalog.Profiles {
		if profile.Name == "portable" {
			t.Fatal("portable catalog replaced the existing system catalog")
		}
	}
}

func TestSubscriptionOnlyInstallationPreservesExistingCatalog(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	if _, err := source.CreateSubscriptionProfile("portable"); err != nil {
		t.Fatal(err)
	}
	target := layout.SystemAt(t.TempDir())
	targetManager, err := New(target, bytes.NewBuffer(nil), bytes.NewBuffer(nil))
	if err != nil {
		t.Fatal(err)
	}
	catalog, _, _, _, err := targetManager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	systemProfile := catalog.Profiles[0]
	systemProfile.Name = "system"
	if err := targetManager.subscriptions.Update(func(candidate *subscriptions.Catalog) error {
		candidate.Profiles[0] = systemProfile
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	existing := state.NewDocument()
	meaningful, err := source.meaningfulSubscriptionData(target, existing)
	if err != nil {
		t.Fatal(err)
	}
	if !meaningful {
		t.Fatal("configured subscription catalog was treated as empty without a selected core")
	}
	operation, err := source.stageSubscriptionInstallation(target, existing, meaningful)
	if err != nil {
		t.Fatal(err)
	}
	defer operation.cleanup()
	if err := operation.activate(); err != nil {
		t.Fatal(err)
	}
	if err := operation.commit(); err != nil {
		t.Fatal(err)
	}
	preserved, err := subscriptions.NewStore(target).Read()
	if err != nil {
		t.Fatal(err)
	}
	if len(preserved.Profiles) != 1 || preserved.Profiles[0].Name != "system" {
		t.Fatalf("subscription-only system catalog was not preserved: %#v", preserved.Profiles)
	}
}

func TestSubscriptionInstallationMigratesLegacySystemURL(t *testing.T) {
	t.Parallel()
	source := newTestManager(t)
	target := layout.SystemAt(t.TempDir())
	existing := state.NewDocument()
	existing.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
	existing.Subscription.URL = "https://system.example/subscription"
	operation, err := source.stageSubscriptionInstallation(target, existing, true)
	if err != nil {
		t.Fatal(err)
	}
	defer operation.cleanup()
	if err := operation.activate(); err != nil {
		t.Fatal(err)
	}
	if err := operation.commit(); err != nil {
		t.Fatal(err)
	}
	catalog, err := subscriptions.NewStore(target).Read()
	if err != nil {
		t.Fatal(err)
	}
	if len(catalog.Profiles) != 1 || len(catalog.Profiles[0].Sources) != 1 ||
		catalog.Profiles[0].Sources[0].URL != existing.Subscription.URL {
		t.Fatalf("legacy subscription was not migrated: %#v", catalog.Profiles)
	}
}

func TestEmptySystemStateDoesNotRequireConfirmation(t *testing.T) {
	t.Parallel()
	empty := state.NewDocument()
	if meaningfulState(empty) {
		t.Fatalf("empty state is meaningful: %#v", empty)
	}
	empty.LastError = "old diagnostic"
	if meaningfulState(empty) {
		t.Fatalf("diagnostics made empty state meaningful: %#v", empty)
	}
}

func TestInstallMergeCopiesDeploymentIntoEmptySystemState(t *testing.T) {
	t.Parallel()
	source := state.NewDocument()
	sourceState := source.Core("sing-box").Source("")
	sourceState.Channels["stable"] = "1.2.3"
	sourceState.Installed["1.2.3"] = &state.Installation{}
	source.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
	source.Active = &state.Deployment{
		Core:       "sing-box",
		Ref:        "stable",
		Version:    "1.2.3",
		ConfigHash: testHashA,
	}
	source.Configs["sing-box"] = testHashA
	source.ActiveProfileID = "portable-profile"
	source.AutoRestart = false

	merged := mergeInstallDocument(source, state.NewDocument(), false)
	if merged.Selected == nil || *merged.Selected != *source.Selected {
		t.Fatalf("selected = %#v", merged.Selected)
	}
	if merged.Active == nil || *merged.Active != *source.Active {
		t.Fatalf("active = %#v", merged.Active)
	}
	if merged.ActiveProfileID != source.ActiveProfileID || merged.AutoRestart != source.AutoRestart {
		t.Fatalf("subscription settings = %q, %t", merged.ActiveProfileID, merged.AutoRestart)
	}
}

func TestInstallMergeCopiesDeploymentButPreservesStoppedIntent(t *testing.T) {
	t.Parallel()
	source := state.NewDocument()
	sourceState := source.Core("sing-box").Source("")
	sourceState.Channels["stable"] = "1.2.3"
	sourceState.Installed["1.2.3"] = &state.Installation{}
	source.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
	source.Active = &state.Deployment{
		Core:       "sing-box",
		Ref:        "stable",
		Version:    "1.2.3",
		ConfigHash: testHashA,
	}
	source.Configs["sing-box"] = testHashA

	existing := state.NewDocument()
	existing.DesiredState = state.DesiredStopped
	merged := mergeInstallDocument(source, existing, false)
	if merged.Active == nil || *merged.Active != *source.Active {
		t.Fatalf("active = %#v", merged.Active)
	}
	if merged.DesiredState != state.DesiredStopped {
		t.Fatalf("desired state = %q", merged.DesiredState)
	}
}

func TestMeaningfulSystemStateRequiresConfirmation(t *testing.T) {
	t.Parallel()
	document := state.NewDocument()
	document.Selected = &state.Selection{Core: "sing-box", Ref: "stable"}
	if !meaningfulState(document) {
		t.Fatal("selected system state was treated as empty")
	}
}
