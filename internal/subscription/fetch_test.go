package subscription

import (
	"context"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"testing"

	"github.com/tinymins/sempre/internal/layout"
)

func TestFetcherPersistsLastKnownGoodSnapshot(t *testing.T) {
	body := "proxies:\n- name: edge\n  type: socks5\n  server: edge.example.com\n  port: 1080\n"
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		_, _ = writer.Write([]byte(body))
	}))
	defer server.Close()
	paths := layout.At(filepath.Join(t.TempDir(), "root"))
	if err := paths.Ensure(); err != nil {
		t.Fatal(err)
	}
	store := NewStore(paths)
	if err := store.Initialize(""); err != nil {
		t.Fatal(err)
	}
	fetcher := NewFetcher(store)
	source := Source{ID: NewID(), Type: SourceURL, Enabled: true, URL: server.URL, UserAgent: DefaultUserAgent, FetchMode: FetchAuto}
	first, updated, cached, err := fetcher.LoadValidated(context.Background(), source, true, validateSubscriptionContent)
	if err != nil || cached || len(first) == 0 {
		t.Fatalf("initial fetch = %q, cached=%t, err=%v", first, cached, err)
	}
	body = "account expired"
	second, fallback, cached, err := fetcher.LoadValidated(context.Background(), updated, true, validateSubscriptionContent)
	if err != nil || !cached || string(second) != string(first) {
		t.Fatalf("fallback = %q, cached=%t, err=%v", second, cached, err)
	}
	if fallback.LastStatus != "last-known-good cache" || fallback.LastError == "" {
		t.Fatalf("fallback metadata = %#v", fallback)
	}
}
