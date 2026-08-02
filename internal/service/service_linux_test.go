//go:build linux

package service

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestRenderSystemdUnitPassesSystemdAnalyze(t *testing.T) {
	t.Parallel()
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	workingDirectory := t.TempDir()
	unit, err := renderSystemdUnit(executable, workingDirectory)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(unit), `WorkingDirectory="`) {
		t.Fatalf("working directory was quoted: %s", unit)
	}
	validator, err := exec.LookPath("systemd-analyze")
	if err != nil {
		t.Skip("systemd-analyze is unavailable")
	}
	path := filepath.Join(t.TempDir(), "sempre.service")
	if err := os.WriteFile(path, unit, 0o600); err != nil {
		t.Fatal(err)
	}
	if output, err := exec.Command(validator, "verify", path).CombinedOutput(); err != nil {
		t.Fatalf("systemd-analyze verify failed: %v\n%s", err, output)
	}
}
