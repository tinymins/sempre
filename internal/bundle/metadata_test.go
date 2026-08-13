package bundle

import (
	"os"
	"path/filepath"
	"testing"
)

func TestMetadataRoundTrip(t *testing.T) {
	t.Parallel()
	for _, kind := range []Kind{Release, Snapshot} {
		root := t.TempDir()
		if err := Write(root, kind); err != nil {
			t.Fatal(err)
		}
		metadata, err := Read(root)
		if err != nil {
			t.Fatal(err)
		}
		if metadata.Schema != SchemaVersion || metadata.Kind != kind {
			t.Fatalf("metadata = %#v", metadata)
		}
	}
}

func TestMetadataRejectsUnsupportedKind(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, MetadataName), []byte(`{"schema":1,"kind":"other"}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := Read(root); err == nil {
		t.Fatal("unsupported bundle kind was accepted")
	}
}
