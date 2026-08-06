//go:build !windows

package cli

import (
	"bytes"
	"runtime"
	"strings"
	"testing"
)

func TestOpenBrowserFallsBackInHeadlessLinux(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("headless fallback is specific to xdg-open on Linux")
	}
	t.Setenv("DISPLAY", "")
	t.Setenv("WAYLAND_DISPLAY", "")
	t.Setenv("BROWSER", "")

	var output bytes.Buffer
	if err := openBrowser("http://127.0.0.1:3000", &output); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), "Open this URL in a browser: http://127.0.0.1:3000") {
		t.Fatalf("fallback output = %q", output.String())
	}
}
