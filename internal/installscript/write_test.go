package installscript

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestWriteForwardsArgumentsWithoutImplicitConfirmation(t *testing.T) {
	t.Parallel()
	tests := []struct {
		goos string
		file string
		want []string
	}{
		{goos: "windows", file: "restore.cmd", want: []string{"bundle restore %*", "pause"}},
		{goos: "linux", file: "restore.sh", want: []string{`bundle restore "$@"`}},
		{goos: "darwin", file: "restore.command", want: []string{`bundle restore "$@"`}},
	}
	for _, test := range tests {
		root := t.TempDir()
		if err := Write(root, "sempre", test.goos, "restore", "bundle", "restore"); err != nil {
			t.Fatal(err)
		}
		data, err := os.ReadFile(filepath.Join(root, test.file))
		if err != nil {
			t.Fatal(err)
		}
		for _, want := range test.want {
			if !strings.Contains(string(data), want) {
				t.Errorf("restorer %s does not contain %q: %q", test.file, want, data)
			}
		}
		if strings.Contains(string(data), "--yes") {
			t.Fatalf("restorer includes implicit confirmation: %q", data)
		}
	}
}

func TestWriteCreatesLinuxRestoreDesktopEntrypoint(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	if err := Write(root, "sempre", "linux", "restore", "bundle", "restore"); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(root, "restore.desktop")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"Name=Restore Sempre Snapshot", "Terminal=true", "sh restore.sh"} {
		if !strings.Contains(string(data), want) {
			t.Errorf("desktop entry does not contain %q: %q", want, data)
		}
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if runtime.GOOS != "windows" && info.Mode().Perm()&0o111 == 0 {
		t.Fatalf("desktop entry mode = %o", info.Mode().Perm())
	}
}

func TestWriteRejectsUnknownEntrypoint(t *testing.T) {
	t.Parallel()
	if err := Write(t.TempDir(), "sempre", "linux", "legacy", "bundle", "restore"); err == nil {
		t.Fatal("unknown entrypoint was accepted")
	}
}
