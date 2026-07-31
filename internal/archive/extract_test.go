package archive

import (
	"archive/zip"
	"os"
	"path/filepath"
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
	if err := Extract(archivePath, filepath.Join(root, "extract"), "zip"); err == nil {
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
	if err := Extract(archivePath, destination, "zip"); err != nil {
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
