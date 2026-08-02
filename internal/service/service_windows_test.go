//go:build windows

package service

import "testing"

func TestWindowsServiceCommandEscapesExecutable(t *testing.T) {
	t.Parallel()
	got := windowsServiceCommand(`C:\Program Files\Sempre Lab\sempre.exe`)
	want := `"C:\Program Files\Sempre Lab\sempre.exe" --system daemon`
	if got != want {
		t.Fatalf("command = %q, want %q", got, want)
	}
}
