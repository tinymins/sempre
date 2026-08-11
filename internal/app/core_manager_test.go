package app

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
)

func TestImportConfigBootstrapsActiveDeployment(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	source := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(source, []byte(testSubscription), 0o600); err != nil {
		t.Fatal(err)
	}
	change, err := manager.ImportConfig(context.Background(), source)
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed || !change.NeedsRestart {
		t.Fatalf("change = %#v", change)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Active == nil || document.Active.Core != "sing-box" || document.Active.Version != "1.2.3" {
		t.Fatalf("active = %#v", document.Active)
	}
	if !document.Pending || document.Active.ConfigHash == "" {
		t.Fatalf("document = %#v", document)
	}
	if _, err := os.Stat(manager.paths.Config("sing-box", document.Active.ConfigHash)); err != nil {
		t.Fatal(err)
	}
}

func TestUseExactVersionPromotesExplicitReference(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Configs["sing-box"] = testHashA
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	change, err := manager.UseCore(context.Background(), "sing-box@1.2.3")
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed {
		t.Fatalf("change = %#v", change)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if !document.Cores["sing-box"].Default.Installed["1.2.3"].Explicit {
		t.Fatal("exact use did not create an explicit reference")
	}
}

func TestExactVersionCanBeSelectedBeforeConfiguration(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Selected = nil
		delete(document.Cores["sing-box"].Default.Channels, "stable")
		document.Cores["sing-box"].Default.Installed["1.2.3"].Explicit = true
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	change, err := manager.UseCore(context.Background(), "sing-box@1.2.3")
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed || change.NeedsRestart {
		t.Fatalf("selection change = %#v", change)
	}
	source := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(source, []byte(testSubscription), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.ImportConfig(context.Background(), source); err != nil {
		t.Fatal(err)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Active == nil || document.Active.Ref != "1.2.3" || document.Active.Version != "1.2.3" {
		t.Fatalf("active = %#v", document.Active)
	}
}

func TestRemoveCoreDeletesVersionAndAliases(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Selected = nil
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	change, err := manager.RemoveCore("sing-box@1.2.3")
	if err != nil {
		t.Fatal(err)
	}
	if !change.Changed {
		t.Fatalf("change = %#v", change)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Cores["sing-box"] != nil {
		t.Fatalf("core state = %#v", document.Cores["sing-box"])
	}
	if _, err := os.Stat(manager.paths.CoreVersionDir("sing-box", "", "1.2.3")); !os.IsNotExist(err) {
		t.Fatalf("version directory still exists: %v", err)
	}
}

func TestRemoveCoreRejectsSelectedVersion(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if _, err := manager.RemoveCore("sing-box@1.2.3"); err == nil || !strings.Contains(err.Error(), "selected") {
		t.Fatalf("selected version removal error = %v", err)
	}
}

func TestSameVersionCanCoexistAcrossRepositories(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	customRepository := "tinymins/sing-box"
	customDirectory := manager.paths.CoreVersionDir("sing-box", customRepository, "1.2.3")
	if err := os.MkdirAll(customDirectory, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(manager.paths.CoreBinary("sing-box", customRepository, "1.2.3"), []byte("custom"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := manager.store.Update(func(document *state.Document) error {
		document.Core("sing-box").Source(customRepository).Installed["1.2.3"] = &state.Installation{Explicit: true}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.UseCore(context.Background(), "sing-box:tinymins/sing-box@1.2.3"); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.RemoveCore("sing-box@1.2.3"); err != nil {
		t.Fatal(err)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Selected == nil || document.Selected.Repository != customRepository || document.Cores["sing-box"].Custom[customRepository].Installed["1.2.3"] == nil {
		t.Fatalf("custom installation was not preserved: %#v", document)
	}
	if _, err := os.Stat(manager.paths.CoreBinary("sing-box", customRepository, "1.2.3")); err != nil {
		t.Fatal(err)
	}
}

func TestExplicitDefaultRepositoryUsesDefaultSource(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	reference, _, err := manager.resolveReference("sing-box:SagerNet/sing-box@1.2.3")
	if err != nil {
		t.Fatal(err)
	}
	if reference.Repository != "" || reference.String() != "sing-box@1.2.3" {
		t.Fatalf("reference = %#v", reference)
	}
}

func TestStageCoresPreservesRepositoryIsolation(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	repository := "tinymins/sing-box"
	customBinary := manager.paths.CoreBinary("sing-box", repository, "1.2.3")
	if err := os.MkdirAll(filepath.Dir(customBinary), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(customBinary, []byte("custom"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := manager.store.Update(func(document *state.Document) error {
		document.Core("sing-box").Source(repository).Installed["1.2.3"] = &state.Installation{Explicit: true}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	target := layout.At(t.TempDir())
	operation, err := manager.stageCores(context.Background(), target, document, false)
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
	for _, binary := range []string{
		target.CoreBinary("sing-box", "", "1.2.3"),
		target.CoreBinary("sing-box", repository, "1.2.3"),
	} {
		if _, err := os.Stat(binary); err != nil {
			t.Fatalf("staged binary %q: %v", binary, err)
		}
	}
}

func TestCollectWeakVersionRemovesOnlyUnreferencedInstall(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	versionDir := manager.paths.CoreVersionDir("sing-box", "", "1.2.3")
	collected := false
	if err := manager.store.Update(func(document *state.Document) error {
		document.Selected = nil
		delete(document.Cores["sing-box"].Default.Channels, "stable")
		collected = manager.collectWeakVersion(document, "sing-box", "", "1.2.3")
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if collected {
		if err := os.RemoveAll(versionDir); err != nil {
			t.Fatal(err)
		}
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Cores["sing-box"].Default.Installed["1.2.3"] != nil {
		t.Fatal("weak unreferenced install was retained")
	}
	if _, err := os.Stat(versionDir); !os.IsNotExist(err) {
		t.Fatalf("version directory still exists: %v", err)
	}
}
