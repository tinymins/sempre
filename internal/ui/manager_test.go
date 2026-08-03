package ui

import (
	"archive/zip"
	"os"
	"path/filepath"
	"testing"
)

func TestInstallFileActivatesValidatedArchive(t *testing.T) {
	t.Parallel()
	root := filepath.Join(t.TempDir(), "ui")
	manager := New(root, filepath.Join(root, "current"))
	archive := writeTestArchive(t, map[string]string{
		"index.html":     "<main>Sempre</main>",
		ManifestName:     `{"schema":1,"name":"Test UI","version":"1.2.3","entry":"index.html","api":{"major":1}}`,
		"assets/app.css": "main { color: black; }",
	})
	metadata, err := manager.InstallFile(archive, "local", "test.zip", "")
	if err != nil {
		t.Fatal(err)
	}
	if metadata.Manifest.Version != "1.2.3" || metadata.SourceType != "local" || len(metadata.Digest) != 64 {
		t.Fatalf("metadata = %#v", metadata)
	}
	data, err := os.ReadFile(filepath.Join(root, "current", "index.html"))
	if err != nil || string(data) != "<main>Sempre</main>" {
		t.Fatalf("installed entry = %q, %v", data, err)
	}
}

func TestInstallFileRejectsTraversal(t *testing.T) {
	t.Parallel()
	root := filepath.Join(t.TempDir(), "ui")
	manager := New(root, filepath.Join(root, "current"))
	archive := writeTestArchive(t, map[string]string{
		"index.html": "ok",
		ManifestName: `{"schema":1,"name":"Test UI","version":"1","entry":"index.html","api":{"major":1}}`,
		"../escape":  "no",
	})
	if _, err := manager.InstallFile(archive, "local", "bad.zip", ""); err == nil {
		t.Fatal("archive traversal was accepted")
	}
}

func TestFailedInstallRetainsCurrentUI(t *testing.T) {
	t.Parallel()
	root := filepath.Join(t.TempDir(), "ui")
	manager := New(root, filepath.Join(root, "current"))
	valid := writeTestArchive(t, map[string]string{
		"index.html": "version one",
		ManifestName: `{"schema":1,"name":"Test UI","version":"1","entry":"index.html","api":{"major":1}}`,
	})
	if _, err := manager.InstallFile(valid, "local", "one.zip", ""); err != nil {
		t.Fatal(err)
	}
	invalid := writeTestArchive(t, map[string]string{
		"index.html": "version two",
		ManifestName: `{"schema":1,"name":"Test UI","version":"2","entry":"index.html","api":{"major":2}}`,
	})
	if _, err := manager.InstallFile(invalid, "local", "two.zip", ""); err == nil {
		t.Fatal("incompatible UI was accepted")
	}
	data, err := os.ReadFile(filepath.Join(root, "current", "index.html"))
	if err != nil || string(data) != "version one" {
		t.Fatalf("current UI changed after failed install: %q, %v", data, err)
	}
}

func writeTestArchive(t *testing.T, entries map[string]string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "ui.zip")
	file, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	writer := zip.NewWriter(file)
	for name, content := range entries {
		entry, err := writer.Create(name)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := entry.Write([]byte(content)); err != nil {
			t.Fatal(err)
		}
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}
	return path
}
