//go:build windows

package cli

import (
	"fmt"

	"golang.org/x/sys/windows"
)

func openBrowser(address string) error {
	operation, _ := windows.UTF16PtrFromString("open")
	target, err := windows.UTF16PtrFromString(address)
	if err != nil {
		return err
	}
	if err := windows.ShellExecute(0, operation, target, nil, nil, windows.SW_SHOWNORMAL); err != nil {
		return fmt.Errorf("open browser: %w", err)
	}
	return nil
}
