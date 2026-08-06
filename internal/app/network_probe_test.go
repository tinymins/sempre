package app

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestRunNetworkTestClassifiesReachabilityAndIPResults(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/ok":
			writer.WriteHeader(http.StatusNoContent)
		case "/openai":
			writer.WriteHeader(http.StatusUnauthorized)
		case "/domestic-ip":
			_, _ = writer.Write([]byte("183.131.177.101\n"))
		case "/foreign-ip":
			_, _ = writer.Write([]byte(`{"ip":"144.34.229.119"}`))
		case "/bad-status":
			writer.WriteHeader(http.StatusServiceUnavailable)
		case "/bad-ip":
			_, _ = writer.Write([]byte("not an address"))
		default:
			writer.WriteHeader(http.StatusNotFound)
		}
	}))
	defer server.Close()

	report := runNetworkTest(context.Background(), []networkTestProbe{
		{ID: "ok", Name: "OK", Region: "foreign", Category: "reachability", URL: server.URL + "/ok", Success: statusIs(http.StatusNoContent)},
		{ID: "openai", Name: "OpenAI", Region: "foreign", Category: "reachability", URL: server.URL + "/openai", Success: statusIs(http.StatusUnauthorized)},
		{ID: "domestic-ip", Name: "Domestic IP", Region: "domestic", Category: "ip", URL: server.URL + "/domestic-ip", Success: status2xx3xx, ParseIP: parseTextIP},
		{ID: "foreign-ip", Name: "Foreign IP", Region: "foreign", Category: "ip", URL: server.URL + "/foreign-ip", Success: status2xx3xx, ParseIP: parseJSONIP},
		{ID: "bad-status", Name: "Bad Status", Region: "foreign", Category: "reachability", URL: server.URL + "/bad-status", Success: status2xx3xx},
		{ID: "bad-ip", Name: "Bad IP", Region: "foreign", Category: "ip", URL: server.URL + "/bad-ip", Success: status2xx3xx, ParseIP: parseTextIP},
	})

	if len(report.Results) != 6 || report.CheckedAt.IsZero() {
		t.Fatalf("report = %#v", report)
	}
	if !report.Results[0].OK || report.Results[0].HTTPStatus != http.StatusNoContent {
		t.Fatalf("ok result = %#v", report.Results[0])
	}
	if !report.Results[1].OK || report.Results[1].HTTPStatus != http.StatusUnauthorized {
		t.Fatalf("openai result = %#v", report.Results[1])
	}
	if !report.Results[2].OK || report.Results[2].IP != "183.131.177.101" {
		t.Fatalf("domestic IP result = %#v", report.Results[2])
	}
	if !report.Results[3].OK || report.Results[3].IP != "144.34.229.119" {
		t.Fatalf("foreign IP result = %#v", report.Results[3])
	}
	if report.Results[4].OK || !strings.Contains(report.Results[4].Detail, "HTTP 503") {
		t.Fatalf("bad status result = %#v", report.Results[4])
	}
	if report.Results[5].OK || !strings.Contains(report.Results[5].Detail, "IP address") {
		t.Fatalf("bad IP result = %#v", report.Results[5])
	}
}

func TestRunNetworkTestHonorsContextCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	report := runNetworkTest(ctx, []networkTestProbe{{
		ID: "cancelled", Name: "Cancelled", Region: "foreign", Category: "reachability",
		URL: "https://example.invalid", Success: status2xx3xx,
	}})
	if len(report.Results) != 1 || report.Results[0].OK || report.Results[0].Detail == "" {
		t.Fatalf("report = %#v", report)
	}
}

func TestParseTextIPRejectsMissingAddress(t *testing.T) {
	if _, err := parseTextIP([]byte("hello world")); err == nil {
		t.Fatal("missing IP was accepted")
	}
	if ip, err := parseTextIP([]byte("当前 IP：183.131.177.101 来自于：中国")); err != nil || ip != "183.131.177.101" {
		t.Fatalf("parse IP = %q, %v", ip, err)
	}
}

func TestNetworkTestTimeoutConstant(t *testing.T) {
	if networkTestTimeout != 15*time.Second {
		t.Fatalf("network test timeout = %s", networkTestTimeout)
	}
}
