//go:build windows

package app

import (
	"errors"
	"os"
	"time"

	"golang.org/x/sys/windows"
)

const (
	windowsRenameAttempts = 20
	windowsRenameDelay    = 50 * time.Millisecond
	windowsRenameMaxDelay = 500 * time.Millisecond
)

func renamePath(source, target string) error {
	return retryWindowsRename(func() error {
		return os.Rename(source, target)
	}, time.Sleep)
}

func retryWindowsRename(action func() error, wait func(time.Duration)) error {
	delay := windowsRenameDelay
	var err error
	for attempt := 0; attempt < windowsRenameAttempts; attempt++ {
		err = action()
		if err == nil || !retryableWindowsRenameError(err) {
			return err
		}
		if attempt == windowsRenameAttempts-1 {
			break
		}
		wait(delay)
		delay *= 2
		if delay > windowsRenameMaxDelay {
			delay = windowsRenameMaxDelay
		}
	}
	return err
}

func retryableWindowsRenameError(err error) bool {
	return errors.Is(err, windows.ERROR_ACCESS_DENIED) ||
		errors.Is(err, windows.ERROR_SHARING_VIOLATION) ||
		errors.Is(err, windows.ERROR_LOCK_VIOLATION)
}
