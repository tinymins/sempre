package app

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
	uiassets "github.com/tinymins/sempre/internal/ui"
)

func TestShouldBootstrapRuntimeOnlyForSetupIntent(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name     string
		options  BootstrapOptions
		previous service.State
		want     bool
	}{
		{name: "existing application repair", previous: service.Running},
		{name: "existing application UI repair", options: BootstrapOptions{UI: "official"}, previous: service.Running},
		{name: "fresh install", previous: service.NotInstalled, want: true},
		{name: "explicit core", options: BootstrapOptions{Core: "sing-box@stable"}, previous: service.Running, want: true},
		{name: "explicit subscription", options: BootstrapOptions{Subscription: "https://example.com/subscription"}, previous: service.Running, want: true},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := shouldBootstrapRuntime(test.options, test.previous); got != test.want {
				t.Fatalf("shouldBootstrapRuntime(%#v, %q) = %t, want %t", test.options, test.previous, got, test.want)
			}
		})
	}
}

func TestBootstrapCoreReferenceDefaultsOnlyWithoutSelection(t *testing.T) {
	t.Parallel()
	if got := bootstrapCoreReference(state.NewDocument(), ""); got != DefaultInstallCore {
		t.Fatalf("fresh default core = %q", got)
	}
	document := state.NewDocument()
	document.Selected = &state.Selection{Core: "sing-box", Ref: "1.2.3"}
	if got := bootstrapCoreReference(document, ""); got != "" {
		t.Fatalf("existing selection was replaced by %q", got)
	}
	if got := bootstrapCoreReference(document, " sing-box:tinymins/sing-box@13.11.2 "); got != "sing-box:tinymins/sing-box@13.11.2" {
		t.Fatalf("explicit core = %q", got)
	}
}

func TestPrepareDefaultSubscriptionCreatesActivatesAndPreservesProfiles(t *testing.T) {
	manager := newTestManager(t)
	var namedID string
	if err := manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
		catalog.Profiles[0].Name = "Primary"
		secondary := subscriptions.NewProfile("Secondary")
		namedID = secondary.ID
		catalog.Profiles = append(catalog.Profiles, secondary)
		return nil
	}); err != nil {
		t.Fatal(err)
	}

	profile, changed, err := manager.prepareDefaultSubscription("  https://example.com/subscription?token=secret  ")
	if err != nil {
		t.Fatal(err)
	}
	if !changed || profile.Name != "" || len(profile.Sources) != 1 || profile.Sources[0].URL != "https://example.com/subscription?token=secret" {
		t.Fatalf("prepared default profile = %#v, changed = %t", profile, changed)
	}
	catalog, active, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	if len(catalog.Profiles) != 3 || catalog.Profiles[0].ID != profile.ID || active != profile.ID {
		t.Fatalf("catalog = %#v, active = %q", catalog.Profiles, active)
	}
	if catalog.Profiles[2].ID != namedID || catalog.Profiles[1].Name != "Primary" {
		t.Fatalf("named profiles were not preserved: %#v", catalog.Profiles)
	}
}

func TestPrepareDefaultSubscriptionDeduplicatesAndEnablesURL(t *testing.T) {
	manager := newTestManager(t)
	value := "https://example.com/subscription?token=secret"
	other := "https://other.example/subscription"
	if err := manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
		catalog.Profiles[0].Sources = []subscriptions.Source{
			{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, URL: value, Enabled: false},
			{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, URL: other, Enabled: true},
			{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, URL: " " + value + " ", Enabled: true},
		}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	profile, changed, err := manager.prepareDefaultSubscription(value)
	if err != nil {
		t.Fatal(err)
	}
	if !changed || len(profile.Sources) != 2 {
		t.Fatalf("profile = %#v, changed = %t", profile, changed)
	}
	matches := 0
	for _, source := range profile.Sources {
		if strings.TrimSpace(source.URL) == value {
			matches++
			if !source.Enabled || source.UserAgent != subscriptions.DefaultUserAgent || source.FetchMode != subscriptions.FetchAuto {
				t.Fatalf("matching source was not normalized: %#v", source)
			}
		}
	}
	if matches != 1 || profile.Sources[1].URL != other {
		t.Fatalf("sources = %#v", profile.Sources)
	}
}

func TestValidateBootstrapUIOptions(t *testing.T) {
	manager := newTestManager(t)
	validDigest := strings.Repeat("a", 64)
	for _, options := range []BootstrapOptions{
		{},
		{UI: "official"},
		{UI: "https://example.com/sempre-ui.zip"},
		{UI: "https://example.com/sempre-ui.zip", UISHA256: validDigest},
		{UI: "tinymins/sempre-ui@stable"},
		{UI: "tinymins/sempre-ui@1.2.3"},
	} {
		if err := manager.validateBootstrapOptions(options); err != nil {
			t.Errorf("validateBootstrapOptions(%#v) = %v", options, err)
		}
	}
	for _, options := range []BootstrapOptions{
		{UI: "./local.zip"},
		{UI: "http://example.com/ui.zip"},
		{UI: "official", UISHA256: validDigest},
		{UI: "tinymins/sempre-ui@stable", UISHA256: validDigest},
		{UI: "https://example.com/ui.zip", UISHA256: "bad"},
	} {
		if err := manager.validateBootstrapOptions(options); err == nil {
			t.Errorf("validateBootstrapOptions(%#v) unexpectedly succeeded", options)
		}
	}
}

func TestBundledUIReplacementRequiresNonOfficialCurrentAndBundle(t *testing.T) {
	manager := newTestManager(t)
	metadata := uiassets.Metadata{
		Manifest:   uiassets.Manifest{Schema: 1, Name: "Custom UI", Version: "1.2.3", Entry: "index.html", API: uiassets.API{Major: 1}},
		SourceType: "local",
	}
	if err := os.MkdirAll(manager.paths.UICurrent, 0o700); err != nil {
		t.Fatal(err)
	}
	data, err := json.Marshal(metadata)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(manager.paths.UICurrent, uiassets.MetadataName), data, 0o600); err != nil {
		t.Fatal(err)
	}
	resources := t.TempDir()
	if err := os.WriteFile(filepath.Join(resources, "sempre-ui.zip"), []byte("bundle"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(resources, "SHA256SUMS"), []byte(strings.Repeat("a", 64)+"  sempre-ui.zip\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	replacement, err := manager.bundledUIReplacement(resources)
	if err != nil {
		t.Fatal(err)
	}
	if replacement == nil || replacement.Name != "Custom UI" || replacement.Version != "1.2.3" || replacement.SourceType != "local" {
		t.Fatalf("replacement = %#v", replacement)
	}
	if err := os.Remove(filepath.Join(resources, "sempre-ui.zip")); err != nil {
		t.Fatal(err)
	}
	if replacement, err := manager.bundledUIReplacement(resources); err != nil || replacement != nil {
		t.Fatalf("missing bundle replacement = %#v, %v", replacement, err)
	}
	if err := os.WriteFile(filepath.Join(resources, "sempre-ui.zip"), []byte("bundle"), 0o600); err != nil {
		t.Fatal(err)
	}

	metadata.SourceType = "official"
	data, _ = json.Marshal(metadata)
	if err := os.WriteFile(filepath.Join(manager.paths.UICurrent, uiassets.MetadataName), data, 0o600); err != nil {
		t.Fatal(err)
	}
	if replacement, err := manager.bundledUIReplacement(resources); err != nil || replacement != nil {
		t.Fatalf("official replacement = %#v, %v", replacement, err)
	}
}
