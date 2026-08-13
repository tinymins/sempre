package app

import (
	"archive/zip"
	"context"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	bundleinfo "github.com/tinymins/sempre/internal/bundle"
	"github.com/tinymins/sempre/internal/gateway"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	"github.com/tinymins/sempre/internal/webconfig"
)

func TestExportBundleClearsPasswordAndIncludesRecordedCores(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if _, err := manager.web.SetPassword("administrator"); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.web.Update(func(config *webconfig.Config) error {
		config.Listen = "127.0.0.1:44111"
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	gatewayConfig, err := manager.gateway.Read()
	if err != nil {
		t.Fatal(err)
	}
	gatewayConfig.PVE.Host = "portable.example"
	if _, err := manager.gateway.Update(gatewayConfig); err != nil {
		t.Fatal(err)
	}
	writeTestUI(t, manager.paths.UICurrent, "Custom Console")
	repository := "acme/sing-box"
	if err := os.MkdirAll(manager.paths.CoreVersionDir("sing-box", repository, "1.2.3"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(manager.paths.CoreBinary("sing-box", repository, "1.2.3"), []byte("custom"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := manager.store.Update(func(document *state.Document) error {
		source := document.Core("sing-box").Source(repository)
		source.Installed["1.2.3"] = &state.Installation{Explicit: true}
		return nil
	}); err != nil {
		t.Fatal(err)
	}

	result, err := manager.ExportBundle(context.Background(), t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(result.Archive); err != nil {
		t.Fatal(err)
	}
	packagePaths := layout.At(result.Directory)
	metadata, err := bundleinfo.Read(result.Directory)
	if err != nil {
		t.Fatal(err)
	}
	if metadata.Kind != bundleinfo.Snapshot {
		t.Fatalf("bundle kind = %q", metadata.Kind)
	}
	web, err := webconfig.New(packagePaths.WebConfig).Read()
	if err != nil {
		t.Fatal(err)
	}
	if web.Listen != "127.0.0.1:44111" || web.Password != "" {
		t.Fatalf("exported web config = %#v", web)
	}
	exportedGateway, err := gateway.NewStore(packagePaths).Read()
	if err != nil {
		t.Fatal(err)
	}
	if exportedGateway.PVE.Host != "portable.example" {
		t.Fatalf("exported gateway config = %#v", exportedGateway)
	}
	for _, path := range []string{
		packagePaths.CoreBinary("sing-box", "", "1.2.3"),
		packagePaths.CoreBinary("sing-box", repository, "1.2.3"),
		filepath.Join(packagePaths.UICurrent, "index.html"),
	} {
		if _, err := os.Stat(path); err != nil {
			t.Fatalf("%s: %v", path, err)
		}
	}
	assertBundleInstallers(t, result.Directory, runtime.GOOS)
	installerName := "install.sh"
	if runtime.GOOS == "windows" {
		installerName = "install.cmd"
	}
	installer, err := os.ReadFile(filepath.Join(result.Directory, installerName))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(installer), "bundle restore") || strings.Contains(string(installer), "--yes") {
		t.Fatalf("snapshot installer = %q", installer)
	}
	archive, err := zip.OpenReader(result.Archive)
	if err != nil {
		t.Fatal(err)
	}
	defer archive.Close()
	corePath, err := filepath.Rel(result.Directory, packagePaths.CoreBinary("sing-box", repository, "1.2.3"))
	if err != nil {
		t.Fatal(err)
	}
	corePath = filepath.ToSlash(filepath.Join(filepath.Base(result.Directory), corePath))
	found := false
	foundMetadata := false
	for _, file := range archive.File {
		if filepath.ToSlash(file.Name) == corePath {
			found = true
		}
		if filepath.ToSlash(file.Name) == filepath.ToSlash(filepath.Join(filepath.Base(result.Directory), bundleinfo.MetadataName)) {
			foundMetadata = true
		}
	}
	if !found {
		t.Fatal("custom core binary was not archived")
	}
	if !foundMetadata {
		t.Fatal("bundle metadata was not archived")
	}
}

func assertBundleInstallers(t *testing.T, directory, goos string) {
	t.Helper()
	want := map[string]bool{}
	switch goos {
	case "windows":
		want["install.cmd"] = true
	case "darwin":
		want["install.command"] = true
		want["install.sh"] = true
	default:
		want["install.sh"] = true
		want["install.desktop"] = true
	}
	for _, name := range []string{"install.cmd", "install.sh", "install.command", "install.desktop"} {
		_, err := os.Stat(filepath.Join(directory, name))
		if want[name] && err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		if !want[name] && !os.IsNotExist(err) {
			t.Fatalf("%s should not exist: %v", name, err)
		}
	}
}
