package supervisor

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestRollingWriterRotates(t *testing.T) {
	t.Parallel()
	path := filepath.Join(t.TempDir(), "core.log")
	writer := NewRollingWriter(path, 8, 2)
	if _, err := writer.Write([]byte("12345678")); err != nil {
		t.Fatal(err)
	}
	if _, err := writer.Write([]byte("abcdef")); err != nil {
		t.Fatal(err)
	}
	current, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	backup, err := os.ReadFile(path + ".1")
	if err != nil {
		t.Fatal(err)
	}
	if strings.TrimSpace(string(current)) != "abcdef" {
		t.Fatalf("current = %q", current)
	}
	if strings.TrimSpace(string(backup)) != "12345678" {
		t.Fatalf("backup = %q", backup)
	}
}
