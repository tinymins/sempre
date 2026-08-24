package main

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func TestSetupDisablesTransparentProxyForServiceSmoke(t *testing.T) {
	root := t.TempDir()
	core := filepath.Join(t.TempDir(), "testcore")
	if err := os.WriteFile(core, []byte("test core"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := setup(root, core); err != nil {
		t.Fatal(err)
	}

	paths := layout.At(root)
	document, err := state.New(paths).Read()
	if err != nil {
		t.Fatal(err)
	}
	catalog, err := subscriptions.NewStore(paths).Read()
	if err != nil {
		t.Fatal(err)
	}
	profile, err := subscriptions.FindProfile(&catalog, document.ActiveProfileID)
	if err != nil {
		t.Fatal(err)
	}
	if profile.TransparentProxy.Mode != subscriptions.TransparentProxyDisabled {
		t.Fatalf("transparent proxy mode = %q", profile.TransparentProxy.Mode)
	}
}
