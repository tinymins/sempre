package controlplane

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestEndpointRoundTripUsesRestrictedFile(t *testing.T) {
	t.Parallel()
	path := filepath.Join(t.TempDir(), "sempre-control.json")
	token, err := NewToken()
	if err != nil {
		t.Fatal(err)
	}
	if err := WriteEndpoint(path, "http://127.0.0.1:33211/", token); err != nil {
		t.Fatal(err)
	}
	endpoint, err := ReadEndpoint(path)
	if err != nil {
		t.Fatal(err)
	}
	if endpoint.BaseURL != "http://127.0.0.1:33211" || endpoint.Token != token {
		t.Fatalf("endpoint = %#v", endpoint)
	}
	if runtime.GOOS != "windows" {
		info, err := os.Stat(path)
		if err != nil {
			t.Fatal(err)
		}
		if permission := info.Mode().Perm(); permission != 0o600 {
			t.Fatalf("endpoint permissions = %o", permission)
		}
	}
	other, err := NewToken()
	if err != nil {
		t.Fatal(err)
	}
	if token == other || !EqualToken(token, token) || EqualToken(token, other) || EqualToken("", "") {
		t.Fatal("daemon token generation or comparison is invalid")
	}
}

func TestClientAuthenticatesAndDecodesErrors(t *testing.T) {
	t.Parallel()
	const token = "daemon-token"
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Header.Get(TokenHeader) != token {
			http.Error(writer, "unauthorized", http.StatusUnauthorized)
			return
		}
		if request.URL.Path == "/failure" {
			writer.Header().Set("Content-Type", "application/json")
			writer.WriteHeader(http.StatusConflict)
			_ = json.NewEncoder(writer).Encode(map[string]any{
				"error": map[string]any{"code": "RUNTIME_NOT_READY", "message": "select a core first"},
			})
			return
		}
		_ = json.NewEncoder(writer).Encode(map[string]string{"state": "running"})
	}))
	defer server.Close()

	path := filepath.Join(t.TempDir(), "sempre-control.json")
	if err := WriteEndpoint(path, server.URL, token); err != nil {
		t.Fatal(err)
	}
	client, err := Discover(path)
	if err != nil {
		t.Fatal(err)
	}
	var result map[string]string
	if err := client.Get(context.Background(), "/status", &result); err != nil {
		t.Fatal(err)
	}
	if result["state"] != "running" {
		t.Fatalf("result = %#v", result)
	}
	var ignored any
	err = client.Post(context.Background(), "/failure", nil, &ignored)
	failure, ok := err.(*HTTPError)
	if !ok || failure.Status != http.StatusConflict || failure.Code != "RUNTIME_NOT_READY" {
		t.Fatalf("failure = %#v", err)
	}
}
