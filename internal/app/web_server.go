package app

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"sync"
	"time"

	"github.com/tinymins/sempre/internal/buildinfo"
	"github.com/tinymins/sempre/internal/controlplane"
	"github.com/tinymins/sempre/internal/webconfig"
)

const (
	apiPrefix       = "/api/v1"
	sessionLifetime = 12 * time.Hour
	loginWindow     = time.Minute
	loginLimit      = 5
)

type apiError struct {
	Error apiErrorBody `json:"error"`
}

type apiErrorBody struct {
	Code    string `json:"code"`
	Message string `json:"message"`
	Details any    `json:"details,omitempty"`
}

type session struct {
	expires time.Time
}

type loginAttempts struct {
	window time.Time
	count  int
}

type authStore struct {
	mu       sync.Mutex
	sessions map[string]session
	attempts map[string]loginAttempts
}

func newAuthStore() *authStore {
	return &authStore{sessions: map[string]session{}, attempts: map[string]loginAttempts{}}
}

func (store *authStore) issue() (string, time.Time, error) {
	data := make([]byte, 32)
	if _, err := rand.Read(data); err != nil {
		return "", time.Time{}, err
	}
	token := hex.EncodeToString(data)
	expires := time.Now().UTC().Add(sessionLifetime)
	store.mu.Lock()
	store.sessions[token] = session{expires: expires}
	store.cleanupLocked(time.Now().UTC())
	store.mu.Unlock()
	return token, expires, nil
}

func (store *authStore) valid(token string) bool {
	store.mu.Lock()
	defer store.mu.Unlock()
	now := time.Now().UTC()
	store.cleanupLocked(now)
	item, ok := store.sessions[token]
	return ok && now.Before(item.expires)
}

func (store *authStore) invalidate() {
	store.mu.Lock()
	store.sessions = map[string]session{}
	store.mu.Unlock()
}

func (store *authStore) allowLogin(address string) bool {
	now := time.Now().UTC()
	store.mu.Lock()
	defer store.mu.Unlock()
	attempt := store.attempts[address]
	if attempt.window.IsZero() || now.Sub(attempt.window) >= loginWindow {
		attempt = loginAttempts{window: now}
	}
	attempt.count++
	store.attempts[address] = attempt
	return attempt.count <= loginLimit
}

func (store *authStore) cleanupLocked(now time.Time) {
	for token, item := range store.sessions {
		if !now.Before(item.expires) {
			delete(store.sessions, token)
		}
	}
	for address, item := range store.attempts {
		if now.Sub(item.window) >= loginWindow {
			delete(store.attempts, address)
		}
	}
}

type adminServer struct {
	manager     *Manager
	auth        *authStore
	runtime     *webRuntime
	handler     http.Handler
	daemonToken string
}

type webRuntime struct {
	manager  *Manager
	server   *http.Server
	mu       sync.Mutex
	listener net.Listener
	address  string
	done     chan error
	token    string
}

func newAdminServer(manager *Manager, daemonToken ...string) *adminServer {
	admin := &adminServer{manager: manager, auth: newAuthStore()}
	if len(daemonToken) > 0 {
		admin.daemonToken = daemonToken[0]
	}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /api/v1/health", admin.health)
	mux.HandleFunc("POST /api/v1/auth/login", admin.login)
	mux.HandleFunc("GET /api/v1/system", admin.system)
	mux.HandleFunc("GET /api/v1/system/network", admin.systemNetwork)
	mux.HandleFunc("POST /api/v1/network/test", admin.networkTest)
	mux.HandleFunc("GET /api/v1/tunnels", admin.tunnelsGet)
	mux.HandleFunc("PUT /api/v1/tunnels", admin.tunnelsPut)
	mux.HandleFunc("POST /api/v1/tunnels/install", admin.tunnelInstall)
	mux.HandleFunc("POST /api/v1/tunnels/{id}/{action}", admin.tunnelAction)
	mux.HandleFunc("GET /api/v1/tunnels/{id}/log", admin.tunnelLog)
	mux.HandleFunc("GET /api/v1/gateway", admin.gatewayGet)
	mux.HandleFunc("PUT /api/v1/gateway", admin.gatewayPut)
	mux.HandleFunc("POST /api/v1/gateway/validate", admin.gatewayValidate)
	mux.HandleFunc("POST /api/v1/gateway/host-plan", admin.gatewayHostPlan)
	mux.HandleFunc("POST /api/v1/gateway/host-apply", admin.gatewayHostApply)
	mux.HandleFunc("POST /api/v1/gateway/dns/query", admin.gatewayDNSQuery)
	mux.HandleFunc("POST /api/v1/gateway/dhcp/leases/revoke", admin.gatewayLeaseRevoke)
	mux.HandleFunc("POST /api/v1/service/action", admin.serviceAction)
	mux.HandleFunc("GET /api/v1/bundle/export", admin.bundleExport)
	mux.HandleFunc("GET /api/v1/cores", admin.cores)
	mux.HandleFunc("POST /api/v1/cores/install", admin.coreInstall)
	mux.HandleFunc("POST /api/v1/cores/update", admin.coreUpdate)
	mux.HandleFunc("POST /api/v1/cores/use", admin.coreUse)
	mux.HandleFunc("POST /api/v1/cores/remove", admin.coreRemove)
	mux.HandleFunc("POST /api/v1/cores/auto/diagnose", admin.autoConfigDiagnose)
	mux.HandleFunc("POST /api/v1/cores/auto/apply", admin.autoConfigApply)
	mux.HandleFunc("GET /api/v1/subscription", admin.subscriptionGet)
	mux.HandleFunc("PATCH /api/v1/subscription", admin.subscriptionPatch)
	mux.HandleFunc("POST /api/v1/subscription/update", admin.subscriptionUpdate)
	mux.HandleFunc("GET /api/v1/subscriptions", admin.subscriptionsGet)
	mux.HandleFunc("POST /api/v1/subscriptions", admin.subscriptionsCreate)
	mux.HandleFunc("GET /api/v1/subscriptions/defaults", admin.subscriptionDefaults)
	mux.HandleFunc("POST /api/v1/subscriptions/cache/clear", admin.subscriptionCacheClear)
	mux.HandleFunc("GET /api/v1/subscriptions/{id}", admin.subscriptionProfileGet)
	mux.HandleFunc("PATCH /api/v1/subscriptions/{id}", admin.subscriptionProfilePatch)
	mux.HandleFunc("PUT /api/v1/subscriptions/{id}", admin.subscriptionProfilePut)
	mux.HandleFunc("DELETE /api/v1/subscriptions/{id}", admin.subscriptionProfileDelete)
	mux.HandleFunc("POST /api/v1/subscriptions/{id}/activate", admin.subscriptionProfileActivate)
	mux.HandleFunc("POST /api/v1/subscriptions/{id}/refresh", admin.subscriptionProfileRefresh)
	mux.HandleFunc("POST /api/v1/subscriptions/{id}/render", admin.subscriptionProfileRender)
	mux.HandleFunc("POST /api/v1/subscriptions/{id}/preview", admin.subscriptionProfileRender)
	mux.HandleFunc("POST /api/v1/subscriptions/{id}/preview-nodes", admin.subscriptionProfilePreviewNodes)
	mux.HandleFunc("POST /api/v1/subscriptions/{id}/trace", admin.subscriptionProfileTrace)
	mux.HandleFunc("POST /api/v1/subscriptions/{id}/trace-node", admin.subscriptionProfileTraceNode)
	mux.HandleFunc("POST /api/v1/subscriptions/source/test", admin.subscriptionSourceTest)
	mux.HandleFunc("POST /api/v1/subscriptions/source/debug", admin.subscriptionSourceDebug)
	mux.HandleFunc("POST /api/v1/subscriptions/{id}/debug", admin.subscriptionProfileDebug)
	mux.HandleFunc("GET /api/v1/custom-nodes", admin.customNodesGet)
	mux.HandleFunc("POST /api/v1/custom-nodes", admin.customNodePost)
	mux.HandleFunc("PUT /api/v1/custom-nodes/{id}", admin.customNodePut)
	mux.HandleFunc("DELETE /api/v1/custom-nodes/{id}", admin.customNodeDelete)
	mux.HandleFunc("GET /api/v1/configs/current", admin.configGet)
	mux.HandleFunc("PUT /api/v1/configs/current", admin.configWriteRemoved)
	mux.HandleFunc("POST /api/v1/configs/validate", admin.configValidate)
	mux.HandleFunc("PATCH /api/v1/configs/common", admin.configWriteRemoved)
	mux.HandleFunc("GET /api/v1/web", admin.webGet)
	mux.HandleFunc("PATCH /api/v1/web", admin.webPatch)
	mux.HandleFunc("GET /api/v1/ui", admin.uiGet)
	mux.HandleFunc("POST /api/v1/ui/install", admin.uiInstall)
	mux.HandleFunc("POST /api/v1/ui/upload", admin.uiUpload)
	mux.HandleFunc("POST /api/v1/ui/update", admin.uiUpdate)
	mux.HandleFunc("DELETE /api/v1/ui", admin.uiRemove)
	mux.HandleFunc("GET /api/v1/runtime/capabilities", admin.runtimeCapabilities)
	mux.HandleFunc("GET /api/v1/runtime/overview", admin.runtimeOverview)
	mux.HandleFunc("GET /api/v1/runtime/config", admin.runtimeConfig)
	mux.HandleFunc("PATCH /api/v1/runtime/config", admin.runtimeConfigPatch)
	mux.HandleFunc("GET /api/v1/runtime/proxies", admin.runtimeProxies)
	mux.HandleFunc("POST /api/v1/runtime/proxies/select", admin.runtimeProxySelect)
	mux.HandleFunc("POST /api/v1/runtime/proxies/delay", admin.runtimeProxyDelay)
	mux.HandleFunc("GET /api/v1/runtime/providers", admin.runtimeProviders)
	mux.HandleFunc("POST /api/v1/runtime/providers/update", admin.runtimeProviderUpdate)
	mux.HandleFunc("POST /api/v1/runtime/providers/healthcheck", admin.runtimeProviderHealthcheck)
	mux.HandleFunc("GET /api/v1/runtime/rules", admin.runtimeRules)
	mux.HandleFunc("GET /api/v1/runtime/rule-providers", admin.runtimeRuleProviders)
	mux.HandleFunc("POST /api/v1/runtime/rule-providers/update", admin.runtimeRuleProviderUpdate)
	mux.HandleFunc("GET /api/v1/runtime/connections", admin.runtimeConnections)
	mux.HandleFunc("POST /api/v1/runtime/connections/close", admin.runtimeConnectionClose)
	mux.HandleFunc("POST /api/v1/runtime/dns/query", admin.runtimeDNSQuery)
	mux.HandleFunc("POST /api/v1/runtime/cache/flush", admin.runtimeCacheFlush)
	mux.HandleFunc("GET /api/v1/runtime/status", admin.runtimeStatus)
	mux.HandleFunc("POST /api/v1/runtime/start", admin.runtimeStart)
	mux.HandleFunc("POST /api/v1/runtime/stop", admin.runtimeStop)
	mux.HandleFunc("POST /api/v1/runtime/restart", admin.runtimeRestart)
	mux.HandleFunc("POST /api/v1/runtime/reload", admin.runtimeReload)
	mux.HandleFunc("GET /api/v1/runtime/events", admin.runtimeEvents)
	mux.HandleFunc("/", admin.static)
	admin.handler = admin.middleware(mux)
	return admin
}

func (manager *Manager) runControlPlane(ctx context.Context, runCore func(context.Context) error) error {
	token, err := controlplane.NewToken()
	if err != nil {
		return err
	}
	admin := newAdminServer(manager, token)
	runtime := &webRuntime{
		manager: manager,
		done:    make(chan error, 4),
		token:   token,
	}
	runtime.server = &http.Server{
		Handler:           admin.handler,
		ReadHeaderTimeout: 10 * time.Second,
		IdleTimeout:       60 * time.Second,
	}
	admin.runtime = runtime
	config, err := manager.web.Read()
	if err != nil {
		return err
	}
	if err := runtime.start(config.Listen); err != nil {
		return err
	}
	coreDone := make(chan error, 1)
	go func() { coreDone <- runCore(ctx) }()
	defer func() {
		shutdownCtx, cancel := context.WithTimeout(context.WithoutCancel(ctx), 10*time.Second)
		defer cancel()
		_ = runtime.server.Shutdown(shutdownCtx)
		_ = os.Remove(manager.paths.Endpoint)
		_ = os.Remove(manager.paths.DaemonControl)
	}()
	for {
		select {
		case <-ctx.Done():
			return nil
		case err := <-runtime.done:
			if err != nil && !errors.Is(err, net.ErrClosed) && !errors.Is(err, http.ErrServerClosed) {
				return err
			}
		case err := <-coreDone:
			if ctx.Err() != nil {
				return nil
			}
			if err != nil {
				fmt.Fprintln(manager.errors, "core supervisor stopped:", err)
			}
			go func() {
				timer := time.NewTimer(time.Second)
				defer timer.Stop()
				select {
				case <-ctx.Done():
					coreDone <- nil
				case <-timer.C:
					coreDone <- runCore(ctx)
				}
			}()
		}
	}
}

func (runtime *webRuntime) start(address string) error {
	listener, err := net.Listen("tcp", address)
	if err != nil {
		return fmt.Errorf("listen on %s: %w", address, err)
	}
	runtime.listener = listener
	runtime.address = address
	if err := runtime.writeEndpoint(address); err != nil {
		listener.Close()
		return err
	}
	go func() { runtime.done <- runtime.server.Serve(listener) }()
	return nil
}

func (runtime *webRuntime) rebind(address string) error {
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	if address == runtime.address {
		return nil
	}
	if err := webconfig.ValidateListen(address); err != nil {
		return err
	}
	listener, err := net.Listen("tcp", address)
	if err != nil {
		return fmt.Errorf("listen on %s: %w", address, err)
	}
	previous := runtime.listener
	if _, err := runtime.manager.web.Update(func(config *webconfig.Config) error {
		config.Listen = address
		return nil
	}); err != nil {
		listener.Close()
		return err
	}
	if err := runtime.writeEndpoint(address); err != nil {
		listener.Close()
		_, rollbackErr := runtime.manager.web.Update(func(config *webconfig.Config) error {
			config.Listen = runtime.address
			return nil
		})
		return errors.Join(err, rollbackErr)
	}
	runtime.listener = listener
	runtime.address = address
	go func() { runtime.done <- runtime.server.Serve(listener) }()
	if previous != nil {
		_ = previous.Close()
	}
	return nil
}

func (runtime *webRuntime) writeEndpoint(address string) error {
	localURL, err := webconfig.LocalURL(address)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(runtime.manager.paths.Root, 0o755); err != nil {
		return err
	}
	if err := webconfig.WriteEndpoint(runtime.manager.paths.Endpoint, webconfig.Endpoint{
		Version:  buildinfo.Version,
		Bind:     address,
		LocalURL: localURL,
	}); err != nil {
		return err
	}
	if err := controlplane.WriteEndpoint(runtime.manager.paths.DaemonControl, localURL, runtime.token); err != nil {
		_ = os.Remove(runtime.manager.paths.Endpoint)
		return err
	}
	return nil
}
