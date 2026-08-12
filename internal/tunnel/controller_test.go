package tunnel

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/layout"
)

func TestControllerRunsOneWorkerPerRemoteInstance(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("test helper is a POSIX executable")
	}
	paths := layout.At(t.TempDir())
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	binary := BinaryPath(paths)
	if err := os.MkdirAll(filepath.Dir(binary), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(binary, []byte("#!/bin/sh\ntrap 'exit 0' TERM INT\nwhile :; do sleep 1; done\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	controller, err := New(paths)
	if err != nil {
		t.Fatal(err)
	}
	config := validConfig()
	config.Instances = append(config.Instances, Instance{ID: "sh", Name: "Shanghai", DesiredState: DesiredRunning, ServerURL: "wss://sh.example.com", WebsocketPing: "15s", ConnectionRetryMaxBackoff: "30s", Forwards: []Forward{{ID: "sh-wg", Name: "WG", ListenPort: 52002, RemoteHost: "127.0.0.1", RemotePort: 31088}}})
	if _, err := controller.Update(config); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	if err := controller.Start(ctx); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(controller.Stop)
	waitForStates(t, controller, map[string]string{"hz": "running", "sh": "running"})
	if _, err := controller.Action("hz", "stop"); err != nil {
		t.Fatal(err)
	}
	waitForStates(t, controller, map[string]string{"hz": "stopped", "sh": "running"})
}

func waitForStates(t *testing.T, controller *Controller, expected map[string]string) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		status, err := controller.Status()
		if err != nil {
			t.Fatal(err)
		}
		matched := len(status.Instances) == len(expected)
		for _, item := range status.Instances {
			matched = matched && expected[item.ID] == item.State
		}
		if matched {
			return
		}
		time.Sleep(25 * time.Millisecond)
	}
	t.Fatalf("tunnel states did not become %#v", expected)
}
