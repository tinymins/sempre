package app

import (
	"context"
	"errors"
	"os"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/state"
)

func TestStatusMarksDeadRuntimePIDAsStale(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Runtime = state.Runtime{State: "running", PID: 1 << 30}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	output, err := manager.Status(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output, "stale record") || !strings.Contains(output, "is not running") {
		t.Fatalf("status = %q", output)
	}
}

func TestResolveFailureRollsBackPendingDeploymentAndCollectsConfigs(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	oldDeployment := state.Deployment{
		Core:       "sing-box",
		Ref:        "stable",
		Version:    "1.2.3",
		ConfigHash: testHashA,
	}
	newDeployment := oldDeployment
	newDeployment.ConfigHash = testHashB
	if err := manager.store.Update(func(document *state.Document) error {
		document.Configs["sing-box"] = testHashB
		document.Active = &newDeployment
		document.Previous = &oldDeployment
		document.Pending = true
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	for _, hash := range []string{testHashA, testHashB, testHashC} {
		if err := state.WriteAtomic(manager.paths.Config("sing-box", hash), []byte("{}"), 0o600); err != nil {
			t.Fatal(err)
		}
	}

	retry, err := manager.rollbackPendingDeployment("resolve failed", errors.New("missing binary"))
	if err != nil {
		t.Fatal(err)
	}
	if !retry {
		t.Fatal("rollback did not request retry of the previous deployment")
	}
	document, err := manager.store.Read()
	if err != nil {
		t.Fatal(err)
	}
	if document.Pending || document.Previous != nil || !state.SameDeployment(document.Active, &oldDeployment) {
		t.Fatalf("document = %#v", document)
	}
	if document.Configs["sing-box"] != testHashA {
		t.Fatalf("active config = %q", document.Configs["sing-box"])
	}
	if _, err := os.Stat(manager.paths.Config("sing-box", testHashA)); err != nil {
		t.Fatal(err)
	}
	for _, hash := range []string{testHashB, testHashC} {
		if _, err := os.Stat(manager.paths.Config("sing-box", hash)); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("configuration %s was retained: %v", hash, err)
		}
	}
}
