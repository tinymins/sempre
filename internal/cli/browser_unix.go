//go:build !windows

package cli

import (
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"runtime"
)

func openBrowser(address string, output io.Writer) error {
	command := "xdg-open"
	if runtime.GOOS == "darwin" {
		command = "open"
	}
	if browserUnavailable(command) {
		writeBrowserFallback(output, address)
		return nil
	}
	if err := exec.Command(command, address).Start(); err != nil {
		if browserOpenFailureCanFallback(command, err) {
			writeBrowserFallback(output, address)
			return nil
		}
		return fmt.Errorf("open browser: %w", err)
	}
	return nil
}

func browserUnavailable(command string) bool {
	if command != "xdg-open" {
		return false
	}
	if os.Getenv("DISPLAY") == "" && os.Getenv("WAYLAND_DISPLAY") == "" && os.Getenv("BROWSER") == "" {
		return true
	}
	_, err := exec.LookPath(command)
	return err != nil
}

func browserOpenFailureCanFallback(command string, err error) bool {
	if command != "xdg-open" {
		return false
	}
	return os.IsNotExist(err) || errors.Is(err, exec.ErrNotFound)
}

func writeBrowserFallback(output io.Writer, address string) {
	if output == nil {
		return
	}
	fmt.Fprintln(output, "Open this URL in a browser:", address)
}
