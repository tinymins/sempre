package app

import (
	"bytes"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync"
	"testing"

	"github.com/tinymins/sempre/internal/controlplane"
	"github.com/tinymins/sempre/internal/state"
)

func TestManagedRuntimeStatusExplainsMissingDeployment(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	status, err := manager.ManagedRuntimeStatus()
	if err != nil {
		t.Fatal(err)
	}
	if status.DesiredState != state.DesiredRunning || status.RuntimeState != "idle" {
		t.Fatalf("status = %#v", status)
	}
	if status.Actions.Start.Allowed || status.Actions.Start.Reason == "" || status.Actions.Restart.Allowed {
		t.Fatalf("actions = %#v", status.Actions)
	}
	if _, err := manager.ManagedRuntimeAction(RuntimeStart); err == nil {
		t.Fatal("start without an active deployment succeeded")
	} else {
		var actionError *RuntimeActionError
		if !errors.As(err, &actionError) || actionError.Code != "RUNTIME_NOT_READY" {
			t.Fatalf("start error = %v", err)
		}
	}
}

func TestManagedRuntimeActionsPersistAndSerializeIntent(t *testing.T) {
	t.Parallel()
	manager := readyRuntimeManager(t)
	status, err := manager.ManagedRuntimeAction(RuntimeStop)
	if err != nil {
		t.Fatal(err)
	}
	if status.DesiredState != state.DesiredStopped || status.RuntimeState != "stopped" {
		t.Fatalf("stopped status = %#v", status)
	}

	reopened, err := New(manager.paths, os.Stdout, os.Stderr)
	if err != nil {
		t.Fatal(err)
	}
	status, err = reopened.ManagedRuntimeStatus()
	if err != nil {
		t.Fatal(err)
	}
	if status.DesiredState != state.DesiredStopped || status.RuntimeState != "stopped" {
		t.Fatalf("reopened status = %#v", status)
	}

	status, err = manager.ManagedRuntimeAction(RuntimeRestart)
	if err != nil {
		t.Fatal(err)
	}
	if status.DesiredState != state.DesiredRunning || status.RuntimeState != "restarting" {
		t.Fatalf("restarting status = %#v", status)
	}
	repeated, err := manager.ManagedRuntimeAction(RuntimeRestart)
	if err != nil {
		t.Fatal(err)
	}
	if repeated.RuntimeState != "restarting" || repeated.DesiredState != state.DesiredRunning {
		t.Fatalf("repeated restart was not idempotent = %#v", repeated)
	}
	status, err = manager.ManagedRuntimeAction(RuntimeStop)
	if err != nil {
		t.Fatal(err)
	}
	if status.DesiredState != state.DesiredStopped || status.RuntimeState != "stopping" {
		t.Fatalf("stop did not override restart = %#v", status)
	}
}

func TestManagedRuntimeRestartIsConcurrentAndIdempotent(t *testing.T) {
	t.Parallel()
	manager := readyRuntimeManager(t)
	const callers = 16
	errors := make(chan error, callers)
	var group sync.WaitGroup
	for range callers {
		group.Add(1)
		go func() {
			defer group.Done()
			_, err := manager.ManagedRuntimeAction(RuntimeRestart)
			errors <- err
		}()
	}
	group.Wait()
	close(errors)
	for err := range errors {
		if err != nil {
			t.Fatalf("concurrent restart: %v", err)
		}
	}
	status, err := manager.ManagedRuntimeStatus()
	if err != nil {
		t.Fatal(err)
	}
	if status.DesiredState != state.DesiredRunning || status.RuntimeState != "restarting" {
		t.Fatalf("status = %#v", status)
	}
	select {
	case <-manager.reload:
	default:
		t.Fatal("restart did not wake the supervisor")
	}
	select {
	case <-manager.reload:
		t.Fatal("idempotent restarts queued duplicate supervisor wakeups")
	default:
	}
}

func TestManagedRuntimeStartRecoversStaleRunningRecord(t *testing.T) {
	t.Parallel()
	manager := readyRuntimeManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Runtime.State = "running"
		document.Runtime.PID = 2147483647
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	before, err := manager.ManagedRuntimeStatus()
	if err != nil {
		t.Fatal(err)
	}
	if before.RuntimeState != "failed" {
		t.Fatalf("stale status = %#v", before)
	}
	after, err := manager.ManagedRuntimeAction(RuntimeStart)
	if err != nil {
		t.Fatal(err)
	}
	if after.RuntimeState != "starting" || after.PID != 0 {
		t.Fatalf("start status = %#v", after)
	}
}

func TestRequestReloadIfRunningHonorsStoppedIntent(t *testing.T) {
	t.Parallel()
	manager := readyRuntimeManager(t)
	if _, err := manager.ManagedRuntimeAction(RuntimeStop); err != nil {
		t.Fatal(err)
	}
	select {
	case <-manager.reload:
	default:
		t.Fatal("stop did not wake supervisor")
	}
	reloaded, err := manager.RequestReloadIfRunning()
	if err != nil {
		t.Fatal(err)
	}
	if reloaded {
		t.Fatal("stopped runtime was scheduled for reload")
	}
	select {
	case <-manager.reload:
		t.Fatal("stopped runtime received a reload wakeup")
	default:
	}
}

func TestRuntimeAPIUsesSessionOrLoopbackDaemonToken(t *testing.T) {
	t.Parallel()
	manager := readyRuntimeManager(t)
	admin := newAdminServer(manager, "daemon-secret")

	request := httptest.NewRequest(http.MethodGet, "/api/v1/runtime/status", nil)
	recorder := httptest.NewRecorder()
	admin.handler.ServeHTTP(recorder, request)
	if recorder.Code != http.StatusUnauthorized {
		t.Fatalf("unauthenticated status = %d", recorder.Code)
	}

	request = httptest.NewRequest(http.MethodPost, "/api/v1/runtime/stop", nil)
	request.RemoteAddr = "127.0.0.1:12345"
	request.Header.Set(controlplane.TokenHeader, "daemon-secret")
	recorder = httptest.NewRecorder()
	admin.handler.ServeHTTP(recorder, request)
	if recorder.Code != http.StatusAccepted {
		t.Fatalf("daemon stop = %d, body = %s", recorder.Code, recorder.Body.String())
	}
	var result struct {
		Status RuntimeStatus `json:"status"`
	}
	if err := json.Unmarshal(recorder.Body.Bytes(), &result); err != nil {
		t.Fatal(err)
	}
	if result.Status.DesiredState != state.DesiredStopped {
		t.Fatalf("stop result = %#v", result.Status)
	}

	request = httptest.NewRequest(http.MethodGet, "/api/v1/runtime/status", nil)
	request.RemoteAddr = "192.0.2.10:12345"
	request.Header.Set(controlplane.TokenHeader, "daemon-secret")
	recorder = httptest.NewRecorder()
	admin.handler.ServeHTTP(recorder, request)
	if recorder.Code != http.StatusUnauthorized {
		t.Fatalf("remote daemon token status = %d", recorder.Code)
	}
}

func TestRuntimeAPIConfigChangeDoesNotStartStoppedCore(t *testing.T) {
	t.Parallel()
	manager := readyRuntimeManager(t)
	if _, err := manager.ManagedRuntimeAction(RuntimeStop); err != nil {
		t.Fatal(err)
	}
	select {
	case <-manager.reload:
	default:
		t.Fatal("stop did not wake supervisor")
	}
	admin := newAdminServer(manager, "daemon-secret")
	body := bytes.NewBufferString(`{"content":"{\"log\":{\"level\":\"debug\"}}"}`)
	request := httptest.NewRequest(http.MethodPut, "/api/v1/configs/current", body)
	request.RemoteAddr = "127.0.0.1:12345"
	request.Header.Set(controlplane.TokenHeader, "daemon-secret")
	request.Header.Set("Content-Type", "application/json")
	recorder := httptest.NewRecorder()
	admin.handler.ServeHTTP(recorder, request)
	if recorder.Code != http.StatusOK || !strings.Contains(recorder.Body.String(), "next time the managed core starts") {
		t.Fatalf("config response = %d, %s", recorder.Code, recorder.Body.String())
	}
	select {
	case <-manager.reload:
		t.Fatal("stopped configuration change woke the supervisor")
	default:
	}
	status, err := manager.ManagedRuntimeStatus()
	if err != nil {
		t.Fatal(err)
	}
	if status.DesiredState != state.DesiredStopped || status.RuntimeState != "stopped" {
		t.Fatalf("status = %#v", status)
	}
}

func readyRuntimeManager(t *testing.T) *Manager {
	t.Helper()
	manager := newTestManager(t)
	if err := state.WriteAtomic(manager.paths.Config("sing-box", testHashA), []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := manager.store.Update(func(document *state.Document) error {
		document.Configs["sing-box"] = testHashA
		document.Active = &state.Deployment{
			Core:       "sing-box",
			Ref:        "stable",
			Version:    "1.2.3",
			ConfigHash: testHashA,
		}
		document.Runtime = state.Runtime{
			State:      "stopped",
			Core:       "sing-box",
			Ref:        "stable",
			Version:    "1.2.3",
			ConfigHash: testHashA,
		}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	return manager
}
