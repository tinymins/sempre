package control

import (
	"context"
	"net/http"
	"net/http/httptest"
	"reflect"
	"testing"
)

func TestProxiesPreservesCoreOrder(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/proxies" {
			http.NotFound(writer, request)
			return
		}
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"proxies":{"GLOBAL":{"type":"Fallback","all":["configured-second","alphabetically-first"]},"configured-second":{"type":"Selector","all":["node-b","node-a"]},"alphabetically-first":{"type":"Selector","all":["node-a"]}}}`))
	}))
	defer server.Close()

	proxies, err := New("sing-box", server.URL, "").Proxies(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	names := make([]string, 0, len(proxies))
	for _, proxy := range proxies {
		names = append(names, proxy.Name)
	}
	if expected := []string{"GLOBAL", "configured-second", "alphabetically-first"}; !reflect.DeepEqual(names, expected) {
		t.Fatalf("proxy order = %v, want %v", names, expected)
	}
	if expected := []string{"node-b", "node-a"}; !reflect.DeepEqual(proxies[1].All, expected) {
		t.Fatalf("node order = %v, want %v", proxies[1].All, expected)
	}
}

func TestOverviewReportsConfiguredCore(t *testing.T) {
	t.Parallel()
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.Header().Set("Content-Type", "application/json")
		switch request.URL.Path {
		case "/version":
			_, _ = writer.Write([]byte(`{"version":"1.0.0"}`))
		case "/configs":
			_, _ = writer.Write([]byte(`{"mode":"rule"}`))
		case "/connections":
			_, _ = writer.Write([]byte(`{"downloadTotal":0,"uploadTotal":0,"connections":[]}`))
		default:
			http.NotFound(writer, request)
		}
	}))
	defer server.Close()

	overview, err := New("mihomo", server.URL, "").Overview(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if overview.Core != "mihomo" {
		t.Fatalf("core = %q, want mihomo", overview.Core)
	}
}
