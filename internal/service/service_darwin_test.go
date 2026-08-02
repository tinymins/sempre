//go:build darwin

package service

import (
	"context"
	"errors"
	"strings"
	"testing"
)

func TestRenderLaunchdPlistIsValidXML(t *testing.T) {
	t.Parallel()
	plist, err := renderLaunchdPlist(
		`/Library/Application Support/Sempre/bin/sempre`,
		`/Library/Application Support/Sempre/data & state`,
	)
	if err != nil {
		t.Fatal(err)
	}
	text := string(plist)
	if strings.Contains(text, `\"`) {
		t.Fatalf("plist contains escaped quote literals: %s", text)
	}
	if !strings.Contains(text, "data &amp; state") {
		t.Fatalf("plist did not escape XML content: %s", text)
	}
}

func TestRetryLaunchdBootstrapRetriesTransitionError(t *testing.T) {
	attempts := 0
	err := retryLaunchdBootstrap(context.Background(), func(context.Context) error {
		attempts++
		if attempts < 3 {
			return errors.New("launchctl bootstrap system: exit status 5: Bootstrap failed: 5: Input/output error")
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if attempts != 3 {
		t.Fatalf("bootstrap attempts = %d, want 3", attempts)
	}
}

func TestRetryLaunchdBootstrapReturnsOtherErrors(t *testing.T) {
	attempts := 0
	want := errors.New("launchctl bootstrap system: permission denied")
	err := retryLaunchdBootstrap(context.Background(), func(context.Context) error {
		attempts++
		return want
	})
	if !errors.Is(err, want) {
		t.Fatalf("bootstrap error = %v, want %v", err, want)
	}
	if attempts != 1 {
		t.Fatalf("bootstrap attempts = %d, want 1", attempts)
	}
}
