//go:build windows

package app

import (
	"errors"
	"os"
	"testing"
	"time"

	"golang.org/x/sys/windows"
)

func TestRetryWindowsRenameSucceedsAfterTransientErrors(t *testing.T) {
	transient := []error{
		windows.ERROR_ACCESS_DENIED,
		windows.ERROR_SHARING_VIOLATION,
		windows.ERROR_LOCK_VIOLATION,
	}
	for _, problem := range transient {
		problem := problem
		t.Run(problem.Error(), func(t *testing.T) {
			calls := 0
			var delays []time.Duration
			err := retryWindowsRename(func() error {
				calls++
				if calls < 3 {
					return &os.LinkError{Op: "rename", Old: "source", New: "target", Err: problem}
				}
				return nil
			}, func(delay time.Duration) {
				delays = append(delays, delay)
			})
			if err != nil {
				t.Fatal(err)
			}
			if calls != 3 {
				t.Fatalf("rename calls = %d, want 3", calls)
			}
			if len(delays) != 2 || delays[0] != windowsRenameDelay || delays[1] != 2*windowsRenameDelay {
				t.Fatalf("retry delays = %v", delays)
			}
		})
	}
}

func TestRetryWindowsRenameStopsOnPermanentError(t *testing.T) {
	calls := 0
	want := errors.New("permanent")
	err := retryWindowsRename(func() error {
		calls++
		return want
	}, func(time.Duration) {
		t.Fatal("permanent error was retried")
	})
	if !errors.Is(err, want) || calls != 1 {
		t.Fatalf("rename error = %v, calls = %d", err, calls)
	}
}

func TestRetryWindowsRenameReturnsLastTransientError(t *testing.T) {
	calls := 0
	err := retryWindowsRename(func() error {
		calls++
		return &os.LinkError{Op: "rename", Old: "source", New: "target", Err: windows.ERROR_ACCESS_DENIED}
	}, func(time.Duration) {})
	if !errors.Is(err, windows.ERROR_ACCESS_DENIED) || calls != windowsRenameAttempts {
		t.Fatalf("rename error = %v, calls = %d", err, calls)
	}
}
