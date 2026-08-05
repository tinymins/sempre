package clashproxy

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/coder/websocket"
	"github.com/tinymins/sempre/internal/core"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func TestServerAuthenticatesAndForwardsClashAPI(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Header.Get("Authorization") != "Bearer internal-secret" {
			http.Error(writer, "bad internal auth", http.StatusUnauthorized)
			return
		}
		_, _ = io.WriteString(writer, request.URL.Path)
	}))
	defer upstream.Close()
	server, baseURL := startTestServer(t, Config{
		External: subscriptions.ManagementAPIConfig{
			Enabled: true, ExternalController: "127.0.0.1:0", Secret: "user-secret",
		},
		Upstream: core.ControlSpec{BaseURL: upstream.URL, Secret: "internal-secret"},
	})
	defer server.Stop(context.Background())

	response, err := http.Get(baseURL + "/proxies")
	if err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if response.StatusCode != http.StatusUnauthorized {
		t.Fatalf("unauthorized status = %d", response.StatusCode)
	}
	request, _ := http.NewRequest(http.MethodGet, baseURL+"/providers/proxies", nil)
	request.Header.Set("Authorization", "Bearer user-secret")
	response, err = http.DefaultClient.Do(request)
	if err != nil {
		t.Fatal(err)
	}
	defer response.Body.Close()
	body, err := io.ReadAll(response.Body)
	if err != nil {
		t.Fatal(err)
	}
	if response.StatusCode != http.StatusOK || string(body) != "/providers/proxies" {
		t.Fatalf("forwarded response = %d %q", response.StatusCode, body)
	}
}

func TestServerForwardsWebSocketConnections(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Header.Get("Authorization") != "Bearer internal-secret" {
			http.Error(writer, "bad internal auth", http.StatusUnauthorized)
			return
		}
		connection, err := websocket.Accept(writer, request, nil)
		if err != nil {
			return
		}
		defer connection.Close(websocket.StatusNormalClosure, "")
		messageType, data, err := connection.Read(request.Context())
		if err == nil {
			_ = connection.Write(request.Context(), messageType, data)
		}
	}))
	defer upstream.Close()
	server, baseURL := startTestServer(t, Config{
		External: subscriptions.ManagementAPIConfig{
			Enabled: true, ExternalController: "127.0.0.1:0", Secret: "user-secret",
		},
		Upstream: core.ControlSpec{BaseURL: upstream.URL, Secret: "internal-secret"},
	})
	defer server.Stop(context.Background())
	header := http.Header{}
	header.Set("Authorization", "Bearer user-secret")
	connection, _, err := websocket.Dial(context.Background(), strings.Replace(baseURL, "http://", "ws://", 1)+"/connections", &websocket.DialOptions{HTTPHeader: header})
	if err != nil {
		t.Fatal(err)
	}
	defer connection.Close(websocket.StatusNormalClosure, "")
	if err := connection.Write(context.Background(), websocket.MessageText, []byte("connected")); err != nil {
		t.Fatal(err)
	}
	_, data, err := connection.Read(context.Background())
	if err != nil || string(data) != "connected" {
		t.Fatalf("websocket response = %q, %v", data, err)
	}
}

func TestServerForwardsMetaCubeXSelectorChanges(t *testing.T) {
	type selectorChange struct {
		Method string
		Path   string
		Name   string
	}
	changes := make(chan selectorChange, 1)
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Header.Get("Authorization") != "Bearer internal-secret" {
			http.Error(writer, "bad internal auth", http.StatusUnauthorized)
			return
		}
		var body struct {
			Name string `json:"name"`
		}
		if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
			http.Error(writer, err.Error(), http.StatusBadRequest)
			return
		}
		changes <- selectorChange{Method: request.Method, Path: request.URL.Path, Name: body.Name}
		writer.WriteHeader(http.StatusNoContent)
	}))
	defer upstream.Close()
	server, baseURL := startTestServer(t, Config{
		External: subscriptions.ManagementAPIConfig{
			Enabled: true, ExternalController: "127.0.0.1:0", Secret: "user-secret",
		},
		Upstream: core.ControlSpec{BaseURL: upstream.URL, Secret: "internal-secret"},
	})
	defer server.Stop(context.Background())

	request, _ := http.NewRequest(http.MethodPut, baseURL+"/proxies/foreign", strings.NewReader(`{"name":"Japan"}`))
	request.Header.Set("Authorization", "Bearer user-secret")
	request.Header.Set("Content-Type", "application/json")
	response, err := http.DefaultClient.Do(request)
	if err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if response.StatusCode != http.StatusNoContent {
		t.Fatalf("selector update status = %d", response.StatusCode)
	}
	change := <-changes
	if change.Method != http.MethodPut || change.Path != "/proxies/foreign" || change.Name != "Japan" {
		t.Fatalf("forwarded selector change = %#v", change)
	}
}

func TestServerProvidesExternalUIAndCORS(t *testing.T) {
	directory := t.TempDir()
	if err := os.WriteFile(filepath.Join(directory, "index.html"), []byte("metacubex"), 0o600); err != nil {
		t.Fatal(err)
	}
	upstream := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
	defer upstream.Close()
	server, baseURL := startTestServer(t, Config{
		External: subscriptions.ManagementAPIConfig{
			Enabled:             true,
			ExternalController:  "127.0.0.1:0",
			Secret:              "user-secret",
			ExternalUI:          directory,
			AllowOrigins:        []string{"https://dashboard.example"},
			AllowPrivateNetwork: true,
		},
		Upstream: core.ControlSpec{BaseURL: upstream.URL, Secret: "internal-secret"},
	})
	defer server.Stop(context.Background())
	response, err := http.Get(baseURL + "/ui/")
	if err != nil {
		t.Fatal(err)
	}
	body, _ := io.ReadAll(response.Body)
	response.Body.Close()
	if response.StatusCode != http.StatusOK || string(body) != "metacubex" {
		t.Fatalf("UI response = %d %q", response.StatusCode, body)
	}
	request, _ := http.NewRequest(http.MethodOptions, baseURL+"/proxies", nil)
	request.Header.Set("Origin", "https://dashboard.example")
	request.Header.Set("Access-Control-Request-Private-Network", "true")
	response, err = http.DefaultClient.Do(request)
	if err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if response.StatusCode != http.StatusNoContent || response.Header.Get("Access-Control-Allow-Origin") != "https://dashboard.example" || response.Header.Get("Access-Control-Allow-Private-Network") != "true" {
		t.Fatalf("preflight response = %d %#v", response.StatusCode, response.Header)
	}
}

func startTestServer(t *testing.T, config Config) (*Server, string) {
	t.Helper()
	server := New()
	if err := server.Start(context.Background(), config); err != nil {
		t.Fatal(err)
	}
	return server, "http://" + server.Address()
}
