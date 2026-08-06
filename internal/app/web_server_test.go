package app

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/controlplane"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
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
	response = testJSONRequest(t, http.MethodGet, server.URL+"/api/v1/system/network", server.URL, login.Token, nil)
	if response.StatusCode != http.StatusOK {
		t.Fatalf("network inventory status = %d", response.StatusCode)
	}
	var inventory struct {
		Supported bool `json:"supported"`
	}
	if err := json.NewDecoder(response.Body).Decode(&inventory); err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	response = testJSONRequest(t, http.MethodGet, server.URL+"/api/v1/bundle/export", server.URL, login.Token, nil)
	if response.StatusCode != http.StatusOK || response.Header.Get("Content-Type") != "application/zip" {
		t.Fatalf("bundle export = %d, %q", response.StatusCode, response.Header.Get("Content-Type"))
	}
	response.Body.Close()
	probeServer := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		_, _ = writer.Write([]byte("ok"))
	}))
	defer probeServer.Close()
	oldProbes := defaultNetworkTestProbes
	defaultNetworkTestProbes = []networkTestProbe{{ID: "local", Name: "Local", Region: "domestic", Category: "reachability", URL: probeServer.URL, Success: status2xx3xx}}
	defer func() { defaultNetworkTestProbes = oldProbes }()
	response = testJSONRequest(t, http.MethodPost, server.URL+"/api/v1/network/test", server.URL, login.Token, nil)
	if response.StatusCode != http.StatusOK {
		t.Fatalf("network test = %d", response.StatusCode)
	}
	var networkReport NetworkTestReport
	if err := json.NewDecoder(response.Body).Decode(&networkReport); err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if len(networkReport.Results) != 1 || !networkReport.Results[0].OK {
		t.Fatalf("network report = %#v", networkReport)
	}
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
	client, err := controlplane.Discover(manager.paths.DaemonControl)
	if err != nil {
		cancel()
		t.Fatal(err)
	}
	var runtimeStatus RuntimeStatus
	if err := client.Get(ctx, "/api/v1/runtime/status", &runtimeStatus); err != nil {
		cancel()
		t.Fatal(err)
	}
	if runtimeStatus.RuntimeState != "idle" {
		cancel()
		t.Fatalf("runtime status = %#v", runtimeStatus)
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
	for _, path := range []string{manager.paths.Endpoint, manager.paths.DaemonControl} {
		if _, err := os.Stat(path); !os.IsNotExist(err) {
			t.Fatalf("control endpoint %q remains after shutdown: %v", path, err)
		}
	}
}

func TestDevelopmentServerRejectsSystemServiceActions(t *testing.T) {
	manager, err := NewDevelopment(layout.At(t.TempDir()), io.Discard, io.Discard)
	if err != nil {
		t.Fatal(err)
	}
	admin := newAdminServer(manager)
	server := httptest.NewServer(admin.handler)
	defer server.Close()

	response := testJSONRequest(t, http.MethodPost, server.URL+"/api/v1/auth/login", server.URL, "", map[string]string{"password": ""})
	if response.StatusCode != http.StatusOK {
		t.Fatalf("login status = %d", response.StatusCode)
	}
	var login struct {
		Token string `json:"token"`
	}
	if err := json.NewDecoder(response.Body).Decode(&login); err != nil {
		t.Fatal(err)
	}
	response.Body.Close()

	response = testJSONRequest(t, http.MethodPost, server.URL+"/api/v1/service/action", server.URL, login.Token, map[string]string{"action": "stop"})
	defer response.Body.Close()
	if response.StatusCode != http.StatusConflict {
		t.Fatalf("service action status = %d", response.StatusCode)
	}
	var body struct {
		Error struct {
			Code string `json:"code"`
		} `json:"error"`
	}
	if err := json.NewDecoder(response.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}
	if body.Error.Code != "SERVICE_NOT_INSTALLED" {
		t.Fatalf("service action error = %q", body.Error.Code)
	}
}

func TestCoresAPIDistinguishesSameVersionRepositories(t *testing.T) {
	t.Parallel()
	manager := newTestManager(t)
	if err := manager.store.Update(func(document *state.Document) error {
		document.Core("sing-box").Source("tinymins/sing-box").Installed["1.2.3"] = &state.Installation{Explicit: true}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	recorder := httptest.NewRecorder()
	newAdminServer(manager).cores(recorder, httptest.NewRequest(http.MethodGet, "/api/v1/cores", nil))
	if recorder.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", recorder.Code, recorder.Body.String())
	}
	var result struct {
		Installed []struct {
			Repository string `json:"repository"`
			Reference  string `json:"reference"`
			Official   bool   `json:"official"`
			Version    string `json:"version"`
		} `json:"installed"`
	}
	if err := json.Unmarshal(recorder.Body.Bytes(), &result); err != nil {
		t.Fatal(err)
	}
	if len(result.Installed) != 2 {
		t.Fatalf("installations = %#v", result.Installed)
	}
	byReference := map[string]bool{}
	for _, item := range result.Installed {
		byReference[item.Reference] = true
		if item.Version != "1.2.3" {
			t.Fatalf("version = %q", item.Version)
		}
	}
	if !byReference["sing-box@1.2.3"] || !byReference["sing-box:tinymins/sing-box@1.2.3"] {
		t.Fatalf("references = %#v", byReference)
	}
}

func TestSubscriptionPreviewAndTraceHTTPContracts(t *testing.T) {
	manager := newTestManager(t)
	var profileID string
	if err := manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
		profile := &catalog.Profiles[0]
		profileID = profile.ID
		profile.UseSystemFilters = false
		profile.Filters = []string{"日本"}
		profile.Editor.Filter = `["日本"]`
		profile.Sources = []subscriptions.Source{{
			ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true,
			Content: "proxies:\n- name: 日本节点\n  type: socks5\n  server: edge.example.com\n  port: 1080\n",
		}}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	admin := newAdminServer(manager)

	previewRequest := httptest.NewRequest(http.MethodPost, "/api/v1/subscriptions/"+profileID+"/preview-nodes", strings.NewReader(`{"format":"clash-meta"}`))
	previewRequest.SetPathValue("id", profileID)
	previewRecorder := httptest.NewRecorder()
	admin.subscriptionProfilePreviewNodes(previewRecorder, previewRequest)
	if previewRecorder.Code != http.StatusOK {
		t.Fatalf("preview status = %d, body = %s", previewRecorder.Code, previewRecorder.Body.String())
	}
	var preview struct {
		Nodes []subscriptions.PreviewNode `json:"nodes"`
	}
	if err := json.Unmarshal(previewRecorder.Body.Bytes(), &preview); err != nil {
		t.Fatal(err)
	}
	if len(preview.Nodes) != 1 || preview.Nodes[0].Name != "🇯🇵 日本节点" || !preview.Nodes[0].Filtered || preview.Nodes[0].SourceIndex != 1 {
		t.Fatalf("preview = %#v", preview)
	}

	traceRequest := httptest.NewRequest(http.MethodPost, "/api/v1/subscriptions/"+profileID+"/trace-node", strings.NewReader(`{"format":"clash-meta","name":"🇯🇵 日本节点"}`))
	traceRequest.SetPathValue("id", profileID)
	traceRecorder := httptest.NewRecorder()
	admin.subscriptionProfileTraceNode(traceRecorder, traceRequest)
	if traceRecorder.Code != http.StatusOK {
		t.Fatalf("trace status = %d, body = %s", traceRecorder.Code, traceRecorder.Body.String())
	}
	var trace struct {
		NodeName string `json:"nodeName"`
		Steps    []struct {
			Type string         `json:"type"`
			Data map[string]any `json:"data"`
		} `json:"steps"`
	}
	if err := json.Unmarshal(traceRecorder.Body.Bytes(), &trace); err != nil {
		t.Fatal(err)
	}
	if trace.NodeName != "🇯🇵 日本节点" || len(trace.Steps) != 4 || trace.Steps[3].Type != "enrich" || trace.Steps[3].Data["originalName"] != "日本节点" {
		t.Fatalf("trace = %#v", trace)
	}
}

func TestSubscriptionProfileDebugIncludesSelectedCustomNodes(t *testing.T) {
	manager := newTestManager(t)
	var profileID string
	if err := manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
		profile := &catalog.Profiles[0]
		profileID = profile.ID
		profile.CustomNodeIDs = []string{"custom-local"}
		catalog.CustomNodes = []subscriptions.CustomNode{{
			ID: "custom-local", Name: "Local node",
			Proxy: map[string]any{"name": "Local node", "type": "socks5", "server": "127.0.0.1", "port": 1080},
		}}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	admin := newAdminServer(manager)
	request := httptest.NewRequest(http.MethodPost, "/api/v1/subscriptions/"+profileID+"/debug", strings.NewReader(`{"format":"clash-meta"}`))
	request.SetPathValue("id", profileID)
	recorder := httptest.NewRecorder()

	admin.subscriptionProfileDebug(recorder, request)

	if recorder.Code != http.StatusOK {
		t.Fatalf("debug status = %d, body = %s", recorder.Code, recorder.Body.String())
	}
	var localStep struct {
		Type string `json:"type"`
		Data struct {
			Count int                         `json:"count"`
			Nodes []subscriptions.PreviewNode `json:"nodes"`
		} `json:"data"`
	}
	found := false
	for _, line := range strings.Split(recorder.Body.String(), "\n") {
		if !strings.HasPrefix(line, "data: ") {
			continue
		}
		var step struct {
			Type string `json:"type"`
		}
		payload := strings.TrimPrefix(line, "data: ")
		if err := json.Unmarshal([]byte(payload), &step); err != nil {
			t.Fatal(err)
		}
		if step.Type != "manual-servers" {
			continue
		}
		if err := json.Unmarshal([]byte(payload), &localStep); err != nil {
			t.Fatal(err)
		}
		found = true
		break
	}
	if !found {
		t.Fatalf("manual-servers event missing: %s", recorder.Body.String())
	}
	if localStep.Data.Count != 1 || len(localStep.Data.Nodes) != 1 {
		t.Fatalf("local server data = %#v", localStep.Data)
	}
	if localStep.Data.Nodes[0].Name != "Local node" || localStep.Data.Nodes[0].SourceURL != "custom-node:custom-local" {
		t.Fatalf("custom node = %#v", localStep.Data.Nodes[0])
	}
}

func TestSubscriptionProfilePatchRenamesProfile(t *testing.T) {
	manager := newTestManager(t)
	catalog, activeProfileID, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	profileID := activeProfileID
	admin := newAdminServer(manager, "daemon-secret")
	request := httptest.NewRequest(http.MethodPatch, "/api/v1/subscriptions/"+profileID, strings.NewReader(`{"name":"  Primary set  "}`))
	request.RemoteAddr = "127.0.0.1:12345"
	request.Header.Set(controlplane.TokenHeader, "daemon-secret")
	request.Header.Set("Content-Type", "application/json")
	recorder := httptest.NewRecorder()
	admin.handler.ServeHTTP(recorder, request)
	if recorder.Code != http.StatusOK {
		t.Fatalf("status = %d, body = %s", recorder.Code, recorder.Body.String())
	}
	var profile subscriptions.Profile
	if err := json.Unmarshal(recorder.Body.Bytes(), &profile); err != nil {
		t.Fatal(err)
	}
	if profile.ID != profileID || profile.Name != "Primary set" {
		t.Fatalf("profile = %#v", profile)
	}
	updated, updatedActiveProfileID, _, _, err := manager.SubscriptionCatalog()
	if err != nil {
		t.Fatal(err)
	}
	if updatedActiveProfileID != profileID || updated.Profiles[0].Name != "Primary set" || len(updated.Profiles) != len(catalog.Profiles) {
		t.Fatalf("catalog = %#v", updated)
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
