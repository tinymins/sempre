package installscript

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestWriteForwardsArgumentsWithoutImplicitConfirmation(t *testing.T) {
	t.Parallel()
	tests := []struct {
		goos string
		file string
		want string
	}{
		{goos: "windows", file: "install.cmd", want: "bundle restore %*"},
		{goos: "linux", file: "install.sh", want: `bundle restore "$@"`},
		{goos: "darwin", file: "install.command", want: `bundle restore "$@"`},
	}
	for _, test := range tests {
		root := t.TempDir()
		if err := Write(root, "sempre", test.goos, "bundle", "restore"); err != nil {
			t.Fatal(err)
		}
		data, err := os.ReadFile(filepath.Join(root, test.file))
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(data), test.want) || strings.Contains(string(data), "--yes") {
			t.Fatalf("installer = %q", data)
		}
	}
}
