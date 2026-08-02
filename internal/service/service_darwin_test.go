//go:build darwin

package service

import (
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
