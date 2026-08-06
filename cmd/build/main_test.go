package main

import (
	"archive/zip"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/layout"
)

func TestBuildReleaseStateInstallsStableCoresAndSelectsSingBox(t *testing.T) {
	t.Parallel()
	installedAt := time.Date(2026, 8, 6, 10, 0, 0, 0, time.UTC)
	document, err := buildReleaseState(installedAt, []releaseCoreInstallation{
		releaseCore("sing-box", "1.13.0"),
		releaseCore("mihomo", "1.19.0"),
		releaseCore("xray", "25.8.3"),
		releaseCore("v2ray", "5.37.0"),
	})
	if err != nil {
		t.Fatal(err)
	}
	if document.Selected == nil || document.Selected.Core != "sing-box" || document.Selected.Ref != core.Stable {
		t.Fatalf("selected = %#v", document.Selected)
	}
	for _, item := range []struct {
		core    string
		version string
	}{
		{"sing-box", "1.13.0"},
		{"mihomo", "1.19.0"},
		{"xray", "25.8.3"},
		{"v2ray", "5.37.0"},
	} {
		source := document.Cores[item.core].Default
		if source.Channels[core.Stable] != item.version {
			t.Fatalf("%s stable channel = %q", item.core, source.Channels[core.Stable])
		}
		installation := source.Installed[item.version]
		if installation == nil {
			t.Fatalf("%s@%s was not installed", item.core, item.version)
		}
		if installation.Source == "" || installation.Digest == "" || !installation.InstalledAt.Equal(installedAt) {
			t.Fatalf("%s@%s installation = %#v", item.core, item.version, installation)
		}
	}
}

func TestReleaseTargetUsesGenericAMD64(t *testing.T) {
	t.Parallel()
	got := releaseCoreTarget(target{os: "linux", arch: "amd64"})
	if got.OS != "linux" || got.Arch != "amd64" || got.AMD64Level != 0 {
		t.Fatalf("target = %#v", got)
	}
}

func TestBundleArchiveUsesReleaseDirectoryPrefix(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	packageDir := filepath.Join(root, "sempre-linux-amd64")
	if err := os.MkdirAll(filepath.Join(packageDir, ".sempre"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(packageDir, "sempre"), []byte("binary"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(packageDir, ".sempre", "state.json"), []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := stateMarker(packageDir); err != nil {
		t.Fatal(err)
	}
	if err := writeBundleInstallers(packageDir, "sempre", "linux"); err != nil {
		t.Fatal(err)
	}
	archivePath := filepath.Join(root, "sempre-bundle-linux-amd64.zip")
	if err := zipDirectoryWithPrefix(archivePath, packageDir, "sempre-linux-amd64"); err != nil {
		t.Fatal(err)
	}
	archive, err := zip.OpenReader(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	defer archive.Close()
	names := map[string]bool{}
	for _, file := range archive.File {
		names[file.Name] = true
	}
	for _, name := range []string{
		"sempre-linux-amd64/sempre",
		"sempre-linux-amd64/.sempre-portable",
		"sempre-linux-amd64/.sempre/state.json",
		"sempre-linux-amd64/install.sh",
		"sempre-linux-amd64/install.desktop",
	} {
		if !names[name] {
			t.Fatalf("%s was not archived", name)
		}
	}
	for _, name := range []string{
		"sempre-linux-amd64/install.cmd",
		"sempre-linux-amd64/install.command",
	} {
		if names[name] {
			t.Fatalf("%s should not be archived", name)
		}
	}
}

func TestWriteBundleInstallersUsesTargetOS(t *testing.T) {
	t.Parallel()
	for _, test := range []struct {
		goos string
		want []string
	}{
		{"windows", []string{"install.cmd"}},
		{"linux", []string{"install.sh", "install.desktop"}},
		{"darwin", []string{"install.command", "install.sh"}},
	} {
		t.Run(test.goos, func(t *testing.T) {
			t.Parallel()
			root := t.TempDir()
			if err := writeBundleInstallers(root, "sempre", test.goos); err != nil {
				t.Fatal(err)
			}
			want := map[string]bool{}
			for _, name := range test.want {
				want[name] = true
			}
			for _, name := range []string{"install.cmd", "install.sh", "install.command", "install.desktop"} {
				_, err := os.Stat(filepath.Join(root, name))
				if want[name] && err != nil {
					t.Fatalf("%s: %v", name, err)
				}
				if !want[name] && !os.IsNotExist(err) {
					t.Fatalf("%s should not exist: %v", name, err)
				}
			}
		})
	}
}

func TestFindReleaseBinaryAcceptsSingleWindowsExecutable(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	binary := filepath.Join(root, "mihomo-windows-amd64-compatible.exe")
	if err := os.WriteFile(binary, []byte("binary"), 0o600); err != nil {
		t.Fatal(err)
	}
	got, err := findReleaseBinary(root, "mihomo.exe", target{os: "windows", arch: "amd64"})
	if err != nil {
		t.Fatal(err)
	}
	if got != binary {
		t.Fatalf("binary = %s", got)
	}
}

func TestCleanupBundleWorkRemovesEmptyParent(t *testing.T) {
	t.Parallel()
	workDir := filepath.Join(t.TempDir(), ".bundle-work", "sempre-linux-amd64")
	if err := os.MkdirAll(workDir, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := cleanupBundleWork(workDir); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Dir(workDir)); !os.IsNotExist(err) {
		t.Fatalf("work parent still exists: %v", err)
	}
}

func releaseCore(coreID, version string) releaseCoreInstallation {
	return releaseCoreInstallation{
		Core: coreID,
		Package: core.Package{
			Version: version,
			URL:     "https://example.com/" + coreID,
			Digest:  "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
		},
	}
}

func stateMarker(packageDir string) error {
	return os.WriteFile(layout.PortableMarkerPath(filepath.Join(packageDir, "sempre")), []byte{}, 0o600)
}
