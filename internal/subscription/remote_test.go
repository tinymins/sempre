package subscription

import (
	"context"
	"crypto/sha256"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestRemoteClientRendersVerifiedArtifact(t *testing.T) {
	content := `{"outbounds":[]}`
	hash := fmt.Sprintf("%x", sha256.Sum256([]byte(content)))
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/share":
			if request.URL.Query().Get("target") != "sing-box-v13" {
				t.Fatalf("target query = %q", request.URL.Query().Get("target"))
			}
			fmt.Fprintf(writer, `{"schema":1,"service":"sempre","profile":{"name":"Shared","revision":7,"updated_at":"2026-08-24T00:00:00Z"},"target":{"format":"sing-box-v13","version":"13","platform":"default"},"artifact":{"url":"/artifact","sha256":"%s","content_type":"application/json","node_count":3,"created_at":"2026-08-24T00:01:00Z"},"edit_url":"%s/subscriptions/id","read_only":true}`, hash, serverURL(request))
		case "/artifact":
			fmt.Fprint(writer, content)
		default:
			http.NotFound(writer, request)
		}
	}))
	defer server.Close()

	profile := NewProfile("Remote")
	profile.Mode = ProfileRemote
	profile.Remote = &RemoteProfile{ManifestURL: server.URL + "/share"}
	result, updated, err := NewRemoteClient(server.Client()).Render(context.Background(), profile, Target{Format: "sing-box-v13", Version: "13", Platform: "default"})
	if err != nil {
		t.Fatal(err)
	}
	if result.Content != content || result.NodeCount != 3 || result.Format != "sing-box-v13" {
		t.Fatalf("unexpected render: %+v", result)
	}
	if updated.Remote.ServerProfile != "Shared" || updated.Remote.ServerRevision != 7 || updated.Remote.ArtifactSHA256 != hash || updated.Remote.LastSyncedAt.IsZero() {
		t.Fatalf("unexpected remote metadata: %+v", updated.Remote)
	}
}

func TestRemoteClientRejectsUntrustedArtifact(t *testing.T) {
	other := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) { fmt.Fprint(writer, "payload") }))
	defer other.Close()
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		fmt.Fprintf(writer, `{"schema":1,"service":"sempre","profile":{"name":"Shared","revision":1,"updated_at":"2026-08-24T00:00:00Z"},"target":{"format":"sing-box-v13"},"artifact":{"url":"%s","sha256":"%064d","node_count":1,"created_at":"2026-08-24T00:00:00Z"},"read_only":true}`, other.URL, 0)
	}))
	defer server.Close()

	_, _, err := NewRemoteClient(server.Client()).Render(context.Background(), remoteTestProfile(server.URL), Target{Format: "sing-box-v13"})
	if err == nil || !strings.Contains(err.Error(), "manifest origin") {
		t.Fatalf("expected same-origin error, got %v", err)
	}
}

func TestRemoteClientRejectsHashAndTargetMismatch(t *testing.T) {
	for _, test := range []struct {
		name, target, hash, expected string
	}{
		{name: "target", target: "clash-meta", hash: fmt.Sprintf("%x", sha256.Sum256([]byte("payload"))), expected: "does not match"},
		{name: "hash", target: "sing-box-v13", hash: strings.Repeat("0", 64), expected: "does not match its manifest"},
	} {
		t.Run(test.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
				if request.URL.Path == "/artifact" {
					fmt.Fprint(writer, "payload")
					return
				}
				fmt.Fprintf(writer, `{"schema":1,"service":"sempre","profile":{"name":"Shared","revision":1,"updated_at":"2026-08-24T00:00:00Z"},"target":{"format":"%s"},"artifact":{"url":"/artifact","sha256":"%s","node_count":1,"created_at":"2026-08-24T00:00:00Z"},"read_only":true}`, test.target, test.hash)
			}))
			defer server.Close()
			_, _, err := NewRemoteClient(server.Client()).Render(context.Background(), remoteTestProfile(server.URL), Target{Format: "sing-box-v13"})
			if err == nil || !strings.Contains(err.Error(), test.expected) {
				t.Fatalf("expected %q error, got %v", test.expected, err)
			}
		})
	}
}

func TestValidateRemoteManifestURL(t *testing.T) {
	for _, value := range []string{"", "file:///tmp/a", "https://user:secret@example.com/share"} {
		if ValidateRemoteManifestURL(value) == nil {
			t.Fatalf("expected %q to be rejected", value)
		}
	}
	if err := ValidateRemoteManifestURL("https://example.com/share"); err != nil {
		t.Fatal(err)
	}
}

func TestRemoteClientDoesNotFollowRedirects(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		http.Redirect(writer, request, "/elsewhere", http.StatusFound)
	}))
	defer server.Close()
	_, _, err := NewRemoteClient(server.Client()).Render(context.Background(), remoteTestProfile(server.URL), Target{Format: "sing-box-v13"})
	if err == nil || !strings.Contains(err.Error(), "HTTP 302") {
		t.Fatalf("expected redirect rejection, got %v", err)
	}
}

func remoteTestProfile(baseURL string) Profile {
	profile := NewProfile("Remote")
	profile.Mode = ProfileRemote
	profile.Remote = &RemoteProfile{ManifestURL: baseURL + "/share"}
	return profile
}

func serverURL(request *http.Request) string {
	scheme := "http"
	if request.TLS != nil {
		scheme = "https"
	}
	return scheme + "://" + request.Host
}
