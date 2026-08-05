package archive

import (
	"archive/zip"
	"compress/gzip"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestExtractZIPRejectsTraversal(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	archivePath := filepath.Join(root, "bad.zip")
	file, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	writer := zip.NewWriter(file)
	entry, err := writer.Create("../escape.txt")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := entry.Write([]byte("bad")); err != nil {
		t.Fatal(err)
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}
	if err := Extract(archivePath, filepath.Join(root, "extract"), ExtractOptions{Format: "zip"}); err == nil {
		t.Fatal("path traversal entry was accepted")
	}
}

func TestExtractZIPAndFind(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	archivePath := filepath.Join(root, "good.zip")
	file, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	writer := zip.NewWriter(file)
	entry, err := writer.Create("sing-box-1.2.3-linux-amd64/sing-box")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := entry.Write([]byte("binary")); err != nil {
		t.Fatal(err)
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}
	destination := filepath.Join(root, "extract")
	if err := Extract(archivePath, destination, ExtractOptions{Format: "zip"}); err != nil {
		t.Fatal(err)
	}
	found, err := Find(destination, "sing-box")
	if err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(found)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "binary" {
		t.Fatalf("content = %q", data)
	}
}

func TestExtractSingleFileGZIP(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	archivePath := filepath.Join(root, "core.gz")
	file, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	writer := gzip.NewWriter(file)
	if _, err := writer.Write([]byte("binary")); err != nil {
		t.Fatal(err)
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}
	destination := filepath.Join(root, "extract")
	if err := Extract(archivePath, destination, ExtractOptions{Format: "gz", SingleFileName: "mihomo"}); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(filepath.Join(destination, "mihomo"))
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "binary" {
		t.Fatalf("content = %q", data)
	}
}

func TestExtractSingleFileGZIPRejectsUnsafeName(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	archivePath := filepath.Join(root, "core.gz")
	if err := os.WriteFile(archivePath, []byte("not reached"), 0o600); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"", "../mihomo", "nested/mihomo"} {
		if err := Extract(archivePath, filepath.Join(root, "extract"), ExtractOptions{Format: "gz", SingleFileName: name}); err == nil {
			t.Fatalf("unsafe name %q was accepted", name)
		}
	}
}

func TestExtractSingleFileGZIPRejectsInvalidAndOversizedData(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	invalid := filepath.Join(root, "invalid.gz")
	if err := os.WriteFile(invalid, []byte("not gzip"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := Extract(invalid, filepath.Join(root, "invalid"), ExtractOptions{Format: "gz", SingleFileName: "mihomo"}); err == nil {
		t.Fatal("invalid gzip was accepted")
	}

	archivePath := filepath.Join(root, "large.gz")
	file, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	writer := gzip.NewWriter(file)
	if _, err := writer.Write([]byte("too large")); err != nil {
		t.Fatal(err)
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}
	destination := filepath.Join(root, "large")
	if err := os.MkdirAll(destination, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := extractGZWithLimit(archivePath, destination, "mihomo", 4); err == nil || !strings.Contains(err.Error(), "expands beyond") {
		t.Fatalf("oversized gzip error = %v", err)
	}
	if _, err := os.Stat(filepath.Join(destination, "mihomo")); !os.IsNotExist(err) {
		t.Fatalf("oversized output remains: %v", err)
	}
}
