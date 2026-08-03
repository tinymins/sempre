package app

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/state"
	"github.com/tinymins/sempre/internal/webconfig"
)

func TestAdminServerAuthenticationBoundary(t *testing.T) {
	manager := newTestManager(t)
	admin := newAdminServer(manager)
	server := httptest.NewServer(admin.handler)
	defer server.Close()

	response := testJSONRequest(t, http.MethodGet, server.URL+"/api/v1/health", "", "", nil)
	if response.StatusCode != http.StatusOK {
		t.Fatalf("health status = %d", response.StatusCode)
	}
	response.Body.Close()
	response = testJSONRequest(t, http.MethodGet, server.URL+"/api/v1/system", "", "", nil)
	if response.StatusCode != http.StatusUnauthorized {
		t.Fatalf("unauthenticated system status = %d", response.StatusCode)
	}
	response.Body.Close()

	response = testJSONRequest(t, http.MethodPost, server.URL+"/api/v1/auth/login", server.URL, "", map[string]string{"password": ""})
	if response.StatusCode != http.StatusOK {
		t.Fatalf("same-origin empty-password login = %d", response.StatusCode)
	}
	var login struct {
		Token string `json:"token"`
	}
	if err := json.NewDecoder(response.Body).Decode(&login); err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if login.Token == "" {
		t.Fatal("login returned no token")
	}
	response = testJSONRequest(t, http.MethodGet, server.URL+"/api/v1/system", server.URL, login.Token, nil)
	if response.StatusCode != http.StatusOK {
		t.Fatalf("authenticated system status = %d", response.StatusCode)
	}
	response.Body.Close()
	if err := manager.store.Update(func(document *state.Document) error {
		document.Cores = map[string]*state.CoreState{}
		document.Selected = nil
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	response = testJSONRequest(t, http.MethodGet, server.URL+"/api/v1/cores", server.URL, login.Token, nil)
	var cores struct {
		Installed json.RawMessage `json:"installed"`
	}
	if err := json.NewDecoder(response.Body).Decode(&cores); err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if string(cores.Installed) != "[]" {
		t.Fatalf("empty installed cores = %s", cores.Installed)
	}

	response = testJSONRequest(t, http.MethodPost, server.URL+"/api/v1/auth/login", "https://console.example", "", map[string]string{"password": ""})
	if response.StatusCode != http.StatusForbidden {
		t.Fatalf("cross-origin empty-password login = %d", response.StatusCode)
	}
	response.Body.Close()
	if _, err := manager.web.SetPassword("administrator"); err != nil {
		t.Fatal(err)
	}
	response = testJSONRequest(t, http.MethodPost, server.URL+"/api/v1/auth/login", "https://console.example", "", map[string]string{"password": "administrator"})
	if response.StatusCode != http.StatusOK || response.Header.Get("Access-Control-Allow-Origin") != "https://console.example" {
		t.Fatalf("password-protected cross-origin login = %d, CORS %q", response.StatusCode, response.Header.Get("Access-Control-Allow-Origin"))
	}
	response.Body.Close()
}

func TestControlPlaneStaysAvailableWithoutCore(t *testing.T) {
	manager := newTestManager(t)
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	address := listener.Addr().String()
	listener.Close()
	if _, err := manager.web.Update(func(config *webconfig.Config) error {
		config.Listen = address
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() {
		done <- manager.runControlPlane(ctx, func(runContext context.Context) error {
			<-runContext.Done()
			return nil
		})
	}()
	deadline := time.Now().Add(5 * time.Second)
	for {
		response, requestErr := http.Get("http://" + address + "/api/v1/health")
		if requestErr == nil {
			response.Body.Close()
			if response.StatusCode == http.StatusOK {
				break
			}
		}
		if time.Now().After(deadline) {
			cancel()
			t.Fatal("control plane did not become ready")
		}
		time.Sleep(20 * time.Millisecond)
	}
	cancel()
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("control plane did not stop")
	}
}

func testJSONRequest(t *testing.T, method, target, origin, token string, body any) *http.Response {
	t.Helper()
	var input io.Reader
	if body != nil {
		data, err := json.Marshal(body)
		if err != nil {
			t.Fatal(err)
		}
		input = bytes.NewReader(data)
	}
	request, err := http.NewRequest(method, target, input)
	if err != nil {
		t.Fatal(err)
	}
	if body != nil {
		request.Header.Set("Content-Type", "application/json")
	}
	if origin != "" {
		request.Header.Set("Origin", origin)
	}
	if token != "" {
		request.Header.Set("Authorization", "Bearer "+token)
	}
	response, err := http.DefaultClient.Do(request)
	if err != nil {
		t.Fatal(err)
	}
	return response
}
