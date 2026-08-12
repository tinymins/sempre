package app

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	subscriptions "github.com/tinymins/sempre/internal/subscription"
	"github.com/tinymins/sempre/internal/tunnel"
)

func TestTunnelAPIStoresPoolAndChangesDesiredState(t *testing.T) {
	manager := newTestManager(t)
	admin := newAdminServer(manager)
	config := tunnel.Config{Schema: tunnel.SchemaVersion, Instances: []tunnel.Instance{{
		ID: "hz", Name: "Hangzhou", DesiredState: tunnel.DesiredStopped, ServerURL: "wss://hz.example.com",
		WebsocketPing: "15s", ConnectionRetryMaxBackoff: "30s",
		Forwards: []tunnel.Forward{{ID: "hz-wg", Name: "WireGuard", ListenPort: 52001, RemoteHost: "127.0.0.1", RemotePort: 31088}},
	}}}
	body, err := json.Marshal(config)
	if err != nil {
		t.Fatal(err)
	}
	put := httptest.NewRequest(http.MethodPut, "/api/v1/tunnels", bytes.NewReader(body))
	put.Header.Set("Content-Type", "application/json")
	recorder := httptest.NewRecorder()
	admin.tunnelsPut(recorder, put)
	if recorder.Code != http.StatusOK {
		t.Fatalf("PUT tunnels = %d: %s", recorder.Code, recorder.Body.String())
	}
	action := httptest.NewRequest(http.MethodPost, "/api/v1/tunnels/hz/start", nil)
	action.SetPathValue("id", "hz")
	action.SetPathValue("action", "start")
	recorder = httptest.NewRecorder()
	admin.tunnelAction(recorder, action)
	if recorder.Code != http.StatusAccepted {
		t.Fatalf("start tunnel = %d: %s", recorder.Code, recorder.Body.String())
	}
	saved, err := manager.tunnels.Read()
	if err != nil {
		t.Fatal(err)
	}
	if saved.Instances[0].DesiredState != tunnel.DesiredRunning || saved.Instances[0].Forwards[0].ListenPort != 52001 {
		t.Fatalf("saved tunnels = %#v", saved)
	}
}

func TestTunnelAPIRejectsCleartextServer(t *testing.T) {
	manager := newTestManager(t)
	admin := newAdminServer(manager)
	request := httptest.NewRequest(http.MethodPut, "/api/v1/tunnels", bytes.NewBufferString(`{"schema":1,"instances":[{"id":"hz","name":"Hangzhou","server_url":"ws://hz.example.com","forwards":[]}]}`))
	request.Header.Set("Content-Type", "application/json")
	recorder := httptest.NewRecorder()
	admin.tunnelsPut(recorder, request)
	if recorder.Code < 400 || recorder.Code >= 500 {
		t.Fatalf("cleartext tunnel status = %d: %s", recorder.Code, recorder.Body.String())
	}
}

func TestUpdateTunnelsProtectsReferencedForward(t *testing.T) {
	manager := newTestManager(t)
	config := tunnel.Config{Schema: tunnel.SchemaVersion, Instances: []tunnel.Instance{{ID: "hz", Name: "Hangzhou", DesiredState: tunnel.DesiredStopped, ServerURL: "wss://hz.example.com", WebsocketPing: "15s", ConnectionRetryMaxBackoff: "30s", Forwards: []tunnel.Forward{{ID: "hz-wg", Name: "WireGuard", ListenPort: 52001, RemoteHost: "127.0.0.1", RemotePort: 31088}}}}}
	if _, _, err := manager.UpdateTunnels(t.Context(), config); err != nil {
		t.Fatal(err)
	}
	if err := manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
		catalog.Profiles[0].PrivateAccess = map[string]any{"connectors": []any{map[string]any{"tunnel_forward_id": "hz-wg"}}}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if _, _, err := manager.UpdateTunnels(t.Context(), tunnel.DefaultConfig()); err == nil {
		t.Fatal("referenced tunnel forward was deleted")
	}
}
