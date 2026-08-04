package app

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"mime"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/tinymins/sempre/internal/buildinfo"
	"github.com/tinymins/sempre/internal/control"
	"github.com/tinymins/sempre/internal/controlplane"
	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/release"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
	uiassets "github.com/tinymins/sempre/internal/ui"
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
	mux.HandleFunc("POST /api/v1/service/action", admin.serviceAction)
	mux.HandleFunc("GET /api/v1/cores", admin.cores)
	mux.HandleFunc("POST /api/v1/cores/install", admin.coreInstall)
	mux.HandleFunc("POST /api/v1/cores/update", admin.coreUpdate)
	mux.HandleFunc("POST /api/v1/cores/use", admin.coreUse)
	mux.HandleFunc("POST /api/v1/cores/remove", admin.coreRemove)
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

func (admin *adminServer) middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		origin := request.Header.Get("Origin")
		if origin != "" {
			allowed := admin.sameOrigin(request, origin)
			if !allowed {
				config, err := admin.manager.web.Read()
				allowed = err == nil && config.Password != ""
			}
			if !allowed {
				apiWriteError(writer, http.StatusForbidden, "ORIGIN_NOT_ALLOWED", "cross-origin access requires an administrator password", nil)
				return
			}
			writer.Header().Set("Access-Control-Allow-Origin", origin)
			writer.Header().Set("Vary", "Origin")
			writer.Header().Set("Access-Control-Allow-Headers", "Authorization, Content-Type, X-Sempre-UI-Name")
			writer.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, PATCH, DELETE, OPTIONS")
		}
		if request.Method == http.MethodOptions {
			writer.WriteHeader(http.StatusNoContent)
			return
		}
		if strings.HasPrefix(request.URL.Path, apiPrefix) &&
			request.URL.Path != apiPrefix+"/health" &&
			request.URL.Path != apiPrefix+"/auth/login" {
			token := strings.TrimPrefix(request.Header.Get("Authorization"), "Bearer ")
			if !admin.validDaemonRequest(request) && (token == "" || !admin.auth.valid(token)) {
				apiWriteError(writer, http.StatusUnauthorized, "UNAUTHORIZED", "a valid administrator session is required", nil)
				return
			}
		}
		next.ServeHTTP(writer, request)
	})
}

func (admin *adminServer) validDaemonRequest(request *http.Request) bool {
	value := request.Header.Get(controlplane.TokenHeader)
	if !controlplane.EqualToken(value, admin.daemonToken) {
		return false
	}
	host, _, err := net.SplitHostPort(request.RemoteAddr)
	if err != nil {
		return false
	}
	address := net.ParseIP(host)
	return address != nil && address.IsLoopback()
}

func (admin *adminServer) sameOrigin(request *http.Request, origin string) bool {
	if origin == "" {
		return true
	}
	parsed, err := url.Parse(origin)
	return err == nil && parsed.Scheme == "http" && strings.EqualFold(parsed.Host, request.Host)
}

func (admin *adminServer) health(writer http.ResponseWriter, request *http.Request) {
	config, _ := admin.manager.web.Read()
	localURL, _ := webconfig.LocalURL(config.Listen)
	document, _ := admin.manager.store.Read()
	apiWriteJSON(writer, http.StatusOK, map[string]any{
		"status": "ok", "version": buildinfo.Version, "api_major": 1,
		"listen": config.Listen, "local_url": localURL, "runtime": document.Runtime.State,
	})
}

func (admin *adminServer) login(writer http.ResponseWriter, request *http.Request) {
	address, _, _ := net.SplitHostPort(request.RemoteAddr)
	if address == "" {
		address = request.RemoteAddr
	}
	if !admin.auth.allowLogin(address) {
		apiWriteError(writer, http.StatusTooManyRequests, "LOGIN_RATE_LIMITED", "too many login attempts; retry later", nil)
		return
	}
	var input struct {
		Password string `json:"password"`
	}
	if err := apiDecodeJSON(request, &input, 64<<10); err != nil {
		apiWriteError(writer, http.StatusBadRequest, "INVALID_REQUEST", err.Error(), nil)
		return
	}
	config, err := admin.manager.web.Read()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	emptyPassword := config.Password == ""
	if emptyPassword && request.Header.Get("Origin") != "" && !admin.sameOrigin(request, request.Header.Get("Origin")) {
		apiWriteError(writer, http.StatusForbidden, "PASSWORD_REQUIRED", "set an administrator password before using a cross-origin UI", nil)
		return
	}
	if !webconfig.VerifyPassword(config.Password, input.Password) {
		apiWriteError(writer, http.StatusUnauthorized, "INVALID_PASSWORD", "administrator password is incorrect", nil)
		return
	}
	token, expires, err := admin.auth.issue()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{
		"token": token, "expires_at": expires,
		"warning": map[bool]string{true: "PASSWORD_EMPTY", false: ""}[emptyPassword],
	})
}

func (admin *adminServer) system(writer http.ResponseWriter, request *http.Request) {
	document, err := admin.manager.store.Read()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	serviceState, serviceErr := admin.manager.service.Status(request.Context())
	if serviceErr != nil {
		serviceState = service.Unknown
	}
	web, _ := admin.manager.web.Read()
	localURL, _ := webconfig.LocalURL(web.Listen)
	uiMetadata, uiErr := admin.manager.ui.Current()
	client, controlErr := admin.manager.controlClient()
	capabilities := control.CapabilitySet{}
	if controlErr == nil {
		capabilities = client.Capabilities(request.Context())
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{
		"version":       buildinfo.Version,
		"commit":        buildinfo.Commit,
		"date":          buildinfo.Date,
		"mode":          admin.manager.paths.Mode,
		"service":       serviceState,
		"desired_state": document.DesiredState,
		"runtime":       document.Runtime,
		"selected":      document.Selected,
		"active":        document.Active,
		"pending":       document.Pending,
		"last_error":    document.LastError,
		"web": map[string]any{
			"listen": web.Listen, "local_url": localURL,
			"password_set": web.Password != "", "password_warning": web.Password == "",
		},
		"ui":           map[string]any{"installed": uiErr == nil, "metadata": valueOrNil(uiMetadata, uiErr)},
		"capabilities": capabilities,
	})
}

func (admin *adminServer) serviceAction(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Action string `json:"action"`
	}
	if err := apiDecodeJSON(request, &input, 16<<10); err != nil {
		apiWriteError(writer, http.StatusBadRequest, "INVALID_REQUEST", err.Error(), nil)
		return
	}
	if input.Action != "restart" && input.Action != "stop" {
		apiWriteError(writer, http.StatusBadRequest, "INVALID_SERVICE_ACTION", "service action must be restart or stop", nil)
		return
	}
	apiWriteJSON(writer, http.StatusAccepted, map[string]string{"status": "scheduled", "action": input.Action})
	go func(action string) {
		time.Sleep(250 * time.Millisecond)
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()
		if action == "restart" {
			_ = admin.manager.service.Restart(ctx)
		} else {
			_ = admin.manager.service.Stop(ctx)
		}
	}(input.Action)
}

func (admin *adminServer) cores(writer http.ResponseWriter, request *http.Request) {
	document, err := admin.manager.store.Read()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	type installation struct {
		Core         string              `json:"core"`
		Repository   string              `json:"repository"`
		Reference    string              `json:"reference"`
		Official     bool                `json:"official"`
		Version      string              `json:"version"`
		Channels     []string            `json:"channels"`
		Installation *state.Installation `json:"installation"`
	}
	result := make([]installation, 0)
	for coreID, coreState := range document.Cores {
		adapter, err := admin.manager.registry.Get(coreID)
		if err != nil {
			admin.internalError(writer, err)
			return
		}
		for _, source := range coreState.SourceEntries() {
			repository := source.Repository
			official := repository == ""
			if official {
				repository = adapter.DefaultRepository()
			}
			for version, item := range source.State.Installed {
				reference := core.Ref{Core: coreID, Repository: source.Repository, Value: version}.String()
				entry := installation{Core: coreID, Repository: repository, Reference: reference, Official: official, Version: version, Channels: []string{}, Installation: item}
				for channel, target := range source.State.Channels {
					if target == version {
						entry.Channels = append(entry.Channels, channel)
					}
				}
				sort.Strings(entry.Channels)
				result = append(result, entry)
			}
		}
	}
	sort.Slice(result, func(i, j int) bool {
		if result[i].Core != result[j].Core {
			return result[i].Core < result[j].Core
		}
		if result[i].Repository != result[j].Repository {
			return result[i].Repository < result[j].Repository
		}
		return result[i].Version < result[j].Version
	})
	apiWriteJSON(writer, http.StatusOK, map[string]any{
		"supported": admin.manager.CoreIDs(), "installed": result,
		"selected": document.Selected, "active": document.Active,
	})
}

func (admin *adminServer) coreInstall(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Reference string `json:"reference"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	change, err := admin.manager.InstallCore(request.Context(), input.Reference)
	admin.writeChange(writer, change, err)
}

func (admin *adminServer) coreUpdate(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Reference string `json:"reference"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	changes, err := admin.manager.UpdateCores(request.Context(), input.Reference)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	for index := range changes {
		change := &changes[index]
		if change.NeedsRestart {
			reloaded, reloadErr := admin.manager.RequestReloadIfRunning()
			if reloadErr != nil {
				admin.internalError(writer, reloadErr)
				return
			}
			if !reloaded {
				change.Message += "; it will take effect the next time the managed core starts"
			}
		}
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"changes": changes})
}

func (admin *adminServer) coreUse(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Reference string `json:"reference"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	change, err := admin.manager.UseCore(request.Context(), input.Reference)
	admin.writeChange(writer, change, err)
}

func (admin *adminServer) coreRemove(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Reference string `json:"reference"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	change, err := admin.manager.RemoveCore(input.Reference)
	admin.writeChange(writer, change, err)
}

func (admin *adminServer) subscriptionGet(writer http.ResponseWriter, request *http.Request) {
	catalog, active, schedule, autoRestart, err := admin.manager.SubscriptionCatalog()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"profile": profile, "interval": schedule.Interval, "last_check": schedule.LastCheck, "last_change": schedule.LastChange, "last_result": schedule.LastResult, "auto_restart": autoRestart})
}

func (admin *adminServer) subscriptionPatch(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		URL         *string `json:"url"`
		Interval    *string `json:"interval"`
		AutoRestart *bool   `json:"auto_restart"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	var changes []Change
	if input.URL != nil {
		change, err := admin.manager.SetSubscription(request.Context(), *input.URL)
		if err != nil {
			admin.operationError(writer, err)
			return
		}
		changes = append(changes, change)
	}
	if input.Interval != nil {
		change, err := admin.manager.SetSubscriptionSchedule(*input.Interval)
		if err != nil {
			admin.operationError(writer, err)
			return
		}
		changes = append(changes, change)
	}
	if input.AutoRestart != nil {
		change, err := admin.manager.SetSubscriptionAutoRestart(*input.AutoRestart)
		if err != nil {
			admin.operationError(writer, err)
			return
		}
		changes = append(changes, change)
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"changes": changes})
}

func (admin *adminServer) subscriptionUpdate(writer http.ResponseWriter, request *http.Request) {
	change, err := admin.manager.UpdateSubscription(request.Context())
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, change)
}

func (admin *adminServer) configGet(writer http.ResponseWriter, request *http.Request) {
	data, hash, err := admin.manager.CurrentConfigContent()
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"hash": hash, "content": string(data)})
}

func (admin *adminServer) configWriteRemoved(writer http.ResponseWriter, _ *http.Request) {
	apiWriteError(writer, http.StatusGone, "DIRECT_CONFIG_REMOVED", "generated configurations are read-only; edit a subscription profile instead", nil)
}

func (admin *adminServer) configValidate(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Content string `json:"content"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	if err := admin.manager.ValidateConfigContent(request.Context(), []byte(input.Content)); err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]bool{"valid": true})
}

func (admin *adminServer) webGet(writer http.ResponseWriter, request *http.Request) {
	config, err := admin.manager.web.Read()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	localURL, _ := webconfig.LocalURL(config.Listen)
	apiWriteJSON(writer, http.StatusOK, map[string]any{
		"listen": config.Listen, "local_url": localURL,
		"password_set": config.Password != "", "password_warning": config.Password == "",
	})
}

func (admin *adminServer) webPatch(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Listen   *string `json:"listen"`
		Password *string `json:"password"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	if input.Listen != nil {
		if admin.runtime == nil {
			apiWriteError(writer, http.StatusConflict, "WEB_RUNTIME_UNAVAILABLE", "web listener is not managed by this process", nil)
			return
		}
		if err := admin.runtime.rebind(*input.Listen); err != nil {
			admin.operationError(writer, err)
			return
		}
	}
	if input.Password != nil {
		if _, err := admin.manager.web.SetPassword(*input.Password); err != nil {
			admin.operationError(writer, err)
			return
		}
		admin.auth.invalidate()
	}
	config, _ := admin.manager.web.Read()
	localURL, _ := webconfig.LocalURL(config.Listen)
	apiWriteJSON(writer, http.StatusOK, map[string]any{
		"listen": config.Listen, "local_url": localURL,
		"password_set": config.Password != "", "reauthenticate": input.Password != nil,
	})
}

func (admin *adminServer) uiGet(writer http.ResponseWriter, request *http.Request) {
	metadata, err := admin.manager.ui.Current()
	if errors.Is(err, os.ErrNotExist) {
		apiWriteJSON(writer, http.StatusOK, map[string]bool{"installed": false})
		return
	}
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, map[string]any{"installed": true, "metadata": metadata})
}

func (admin *adminServer) uiInstall(writer http.ResponseWriter, request *http.Request) {
	var input struct {
		Source string `json:"source"`
		SHA256 string `json:"sha256"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	var (
		metadata uiassets.Metadata
		err      error
	)
	if input.Source == "" || input.Source == "official" {
		metadata, err = admin.manager.InstallOfficialUI(request.Context())
	} else {
		metadata, err = admin.manager.ui.InstallURL(request.Context(), input.Source, input.SHA256)
	}
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, metadata)
}

func (admin *adminServer) uiUpload(writer http.ResponseWriter, request *http.Request) {
	request.Body = http.MaxBytesReader(writer, request.Body, uiassets.MaxArchiveSize)
	file, err := os.CreateTemp(admin.manager.paths.Runtime, "ui-upload-*.zip")
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	path := file.Name()
	defer os.Remove(path)
	_, copyErr := io.Copy(file, request.Body)
	closeErr := file.Close()
	if copyErr != nil || closeErr != nil {
		admin.operationError(writer, errors.Join(copyErr, closeErr))
		return
	}
	name := strings.TrimSpace(request.Header.Get("X-Sempre-UI-Name"))
	if name == "" {
		name = "browser-upload.zip"
	}
	metadata, err := admin.manager.ui.InstallFile(path, "local", name, request.URL.Query().Get("sha256"))
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, metadata)
}

func (admin *adminServer) uiUpdate(writer http.ResponseWriter, request *http.Request) {
	current, err := admin.manager.ui.Current()
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	var metadata uiassets.Metadata
	switch current.SourceType {
	case "official":
		metadata, err = admin.manager.InstallOfficialUI(request.Context())
	case "url":
		metadata, err = admin.manager.ui.InstallURL(request.Context(), current.Source, "")
	default:
		err = fmt.Errorf("locally uploaded UI has no update source; install another archive")
	}
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, metadata)
}

func (admin *adminServer) uiRemove(writer http.ResponseWriter, request *http.Request) {
	if err := admin.manager.ui.Remove(); err != nil {
		admin.operationError(writer, err)
		return
	}
	writer.WriteHeader(http.StatusNoContent)
}

func (manager *Manager) InstallOfficialUI(ctx context.Context) (uiassets.Metadata, error) {
	if metadata, found, err := manager.installBundledUI(); found || err != nil {
		return metadata, err
	}

	client := release.NewClient()
	var item release.GitHubRelease
	var err error
	if buildinfo.Version != "" && buildinfo.Version != "dev" && !strings.Contains(buildinfo.Version, "dirty") {
		item, err = client.Version(ctx, "tinymins/sempre", buildinfo.Version)
	} else {
		item, err = client.LatestStable(ctx, "tinymins/sempre")
	}
	if err != nil {
		return uiassets.Metadata{}, err
	}
	for _, asset := range item.Assets {
		if asset.Name == "sempre-ui.zip" {
			return manager.ui.InstallRemote(ctx, asset.URL, asset.Digest, "official")
		}
	}
	return uiassets.Metadata{}, fmt.Errorf("release %s has no sempre-ui.zip", item.Tag)
}

func (manager *Manager) installBundledUI() (uiassets.Metadata, bool, error) {
	archive := filepath.Join(manager.paths.Resources, "sempre-ui.zip")
	info, err := os.Stat(archive)
	if errors.Is(err, os.ErrNotExist) {
		return uiassets.Metadata{}, false, nil
	}
	if err != nil {
		return uiassets.Metadata{}, true, fmt.Errorf("inspect bundled UI: %w", err)
	}
	if !info.Mode().IsRegular() {
		return uiassets.Metadata{}, true, fmt.Errorf("bundled UI is not a regular file: %s", archive)
	}
	digest, err := checksumFromFile(filepath.Join(manager.paths.Resources, "SHA256SUMS"), "sempre-ui.zip")
	if err != nil {
		return uiassets.Metadata{}, true, fmt.Errorf("verify bundled UI: %w", err)
	}
	metadata, err := manager.ui.InstallFile(archive, "official", "bundle", digest)
	return metadata, true, err
}

func checksumFromFile(path, name string) (string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	for _, line := range strings.Split(string(data), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 2 && strings.TrimPrefix(fields[1], "*") == name {
			if len(fields[0]) != 64 {
				return "", fmt.Errorf("invalid SHA-256 for %s", name)
			}
			return fields[0], nil
		}
	}
	return "", fmt.Errorf("%s is absent from SHA256SUMS", name)
}

func (admin *adminServer) runtimeCapabilities(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	apiWriteJSON(writer, http.StatusOK, client.Capabilities(request.Context()))
}

func (admin *adminServer) runtimeOverview(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Overview(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeConfig(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Config(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeConfigPatch(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input map[string]any
	if !admin.decode(writer, request, &input) {
		return
	}
	err := client.PatchConfig(request.Context(), input)
	admin.writeRuntimeResult(writer, map[string]bool{"updated": err == nil, "persistent": false}, err)
}

func (admin *adminServer) runtimeProxies(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Proxies(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeProxySelect(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct{ Group, Proxy string }
	if !admin.decode(writer, request, &input) {
		return
	}
	err := client.SelectProxy(request.Context(), input.Group, input.Proxy)
	admin.writeRuntimeResult(writer, map[string]bool{"selected": err == nil}, err)
}

func (admin *adminServer) runtimeProxyDelay(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct {
		Name    string `json:"name"`
		URL     string `json:"url"`
		Timeout int    `json:"timeout"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	if input.URL == "" {
		input.URL = "https://www.gstatic.com/generate_204"
	}
	if input.Timeout == 0 {
		input.Timeout = 5000
	}
	delay, err := client.ProxyDelay(request.Context(), input.Name, input.URL, input.Timeout)
	admin.writeRuntimeResult(writer, map[string]int{"delay": delay}, err)
}

func (admin *adminServer) runtimeProviders(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Providers(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeProviderUpdate(writer http.ResponseWriter, request *http.Request) {
	admin.runtimeProviderAction(writer, request, false)
}

func (admin *adminServer) runtimeProviderHealthcheck(writer http.ResponseWriter, request *http.Request) {
	admin.runtimeProviderAction(writer, request, true)
}

func (admin *adminServer) runtimeProviderAction(writer http.ResponseWriter, request *http.Request, healthcheck bool) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct {
		Name string `json:"name"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	var err error
	if healthcheck {
		err = client.HealthcheckProvider(request.Context(), input.Name)
	} else {
		err = client.UpdateProvider(request.Context(), input.Name)
	}
	admin.writeRuntimeResult(writer, map[string]bool{"updated": err == nil}, err)
}

func (admin *adminServer) runtimeRules(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Rules(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeRuleProviders(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.RuleProviders(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeRuleProviderUpdate(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct {
		Name string `json:"name"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	err := client.UpdateRuleProvider(request.Context(), input.Name)
	admin.writeRuntimeResult(writer, map[string]bool{"updated": err == nil}, err)
}

func (admin *adminServer) runtimeConnections(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	result, err := client.Connections(request.Context())
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeConnectionClose(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct {
		ID string `json:"id"`
	}
	if !admin.decode(writer, request, &input) {
		return
	}
	err := client.CloseConnection(request.Context(), input.ID)
	admin.writeRuntimeResult(writer, map[string]bool{"closed": err == nil}, err)
}

func (admin *adminServer) runtimeDNSQuery(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	var input struct{ Name, Type string }
	if !admin.decode(writer, request, &input) {
		return
	}
	if input.Type == "" {
		input.Type = "A"
	}
	result, err := client.DNSQuery(request.Context(), input.Name, input.Type)
	admin.writeRuntimeResult(writer, result, err)
}

func (admin *adminServer) runtimeCacheFlush(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	err := client.FlushFakeIP(request.Context())
	admin.writeRuntimeResult(writer, map[string]bool{"flushed": err == nil}, err)
}

func (admin *adminServer) runtimeStatus(writer http.ResponseWriter, _ *http.Request) {
	status, err := admin.manager.ManagedRuntimeStatus()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusOK, status)
}

func (admin *adminServer) runtimeStart(writer http.ResponseWriter, _ *http.Request) {
	admin.runtimeAction(writer, RuntimeStart)
}

func (admin *adminServer) runtimeStop(writer http.ResponseWriter, _ *http.Request) {
	admin.runtimeAction(writer, RuntimeStop)
}

func (admin *adminServer) runtimeRestart(writer http.ResponseWriter, _ *http.Request) {
	admin.runtimeAction(writer, RuntimeRestart)
}

func (admin *adminServer) runtimeAction(writer http.ResponseWriter, action string) {
	status, err := admin.manager.ManagedRuntimeAction(action)
	if err != nil {
		var actionError *RuntimeActionError
		if errors.As(err, &actionError) {
			apiWriteError(writer, http.StatusConflict, actionError.Code, actionError.Message, map[string]any{"status": status})
			return
		}
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusAccepted, map[string]any{"action": action, "status": status})
}

func (admin *adminServer) runtimeReload(writer http.ResponseWriter, request *http.Request) {
	admin.manager.RequestReload()
	status, err := admin.manager.ManagedRuntimeStatus()
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	apiWriteJSON(writer, http.StatusAccepted, map[string]any{"scheduled": true, "status": status})
}

type runtimeEvent struct {
	Topic string
	Data  json.RawMessage
	Error error
}

func (admin *adminServer) runtimeEvents(writer http.ResponseWriter, request *http.Request) {
	client, ok := admin.runtimeClient(writer)
	if !ok {
		return
	}
	flusher, ok := writer.(http.Flusher)
	if !ok {
		apiWriteError(writer, http.StatusInternalServerError, "STREAM_UNAVAILABLE", "streaming is unavailable", nil)
		return
	}
	allowed := map[string]bool{"traffic": true, "memory": true, "connections": true, "logs": true}
	seen := map[string]bool{}
	var topics []string
	for _, topic := range strings.Split(request.URL.Query().Get("topics"), ",") {
		topic = strings.TrimSpace(topic)
		if topic != "" && allowed[topic] && !seen[topic] {
			seen[topic] = true
			topics = append(topics, topic)
		}
	}
	if len(topics) == 0 {
		topics = []string{"traffic", "memory", "connections", "logs"}
	}
	writer.Header().Set("Content-Type", "text/event-stream")
	writer.Header().Set("Cache-Control", "no-cache")
	writer.Header().Set("X-Accel-Buffering", "no")
	writer.WriteHeader(http.StatusOK)
	flusher.Flush()
	ctx, cancel := context.WithCancel(request.Context())
	defer cancel()
	events := make(chan runtimeEvent, 64)
	for _, topic := range topics {
		go streamRuntimeTopic(ctx, client, topic, events)
	}
	heartbeat := time.NewTicker(15 * time.Second)
	defer heartbeat.Stop()
	var sequence atomic.Uint64
	for {
		select {
		case <-ctx.Done():
			return
		case <-heartbeat.C:
			_, _ = io.WriteString(writer, ": keepalive\n\n")
			flusher.Flush()
		case event := <-events:
			payload := map[string]any{
				"topic": event.Topic, "timestamp": time.Now().UTC(), "sequence": sequence.Add(1),
			}
			if event.Error != nil {
				payload["error"] = event.Error.Error()
			} else {
				payload["data"] = event.Data
			}
			data, _ := json.Marshal(payload)
			if _, err := fmt.Fprintf(writer, "event: %s\ndata: %s\n\n", event.Topic, data); err != nil {
				return
			}
			flusher.Flush()
		}
	}
}

func streamRuntimeTopic(ctx context.Context, client *control.Client, topic string, events chan<- runtimeEvent) {
	for ctx.Err() == nil {
		err := client.Stream(ctx, topic, func(data json.RawMessage) error {
			select {
			case events <- runtimeEvent{Topic: topic, Data: data}:
				return nil
			case <-ctx.Done():
				return ctx.Err()
			}
		})
		if ctx.Err() != nil {
			return
		}
		select {
		case events <- runtimeEvent{Topic: topic, Error: err}:
		case <-ctx.Done():
			return
		}
		timer := time.NewTimer(time.Second)
		select {
		case <-ctx.Done():
			timer.Stop()
			return
		case <-timer.C:
		}
	}
}

func (admin *adminServer) static(writer http.ResponseWriter, request *http.Request) {
	if request.Method != http.MethodGet && request.Method != http.MethodHead {
		apiWriteError(writer, http.StatusMethodNotAllowed, "METHOD_NOT_ALLOWED", "method is not allowed", nil)
		return
	}
	if strings.HasPrefix(request.URL.Path, apiPrefix) {
		apiWriteError(writer, http.StatusNotFound, "NOT_FOUND", "API route was not found", nil)
		return
	}
	relative := strings.TrimPrefix(filepath.Clean("/"+request.URL.Path), string(filepath.Separator))
	if relative == "." || relative == "" {
		relative = "index.html"
	}
	if strings.HasPrefix(filepath.Base(relative), ".") || relative == uiassets.ManifestName {
		http.NotFound(writer, request)
		return
	}
	target := filepath.Join(admin.manager.paths.UICurrent, relative)
	if !pathWithin(admin.manager.paths.UICurrent, target) {
		http.NotFound(writer, request)
		return
	}
	info, err := os.Stat(target)
	if err != nil || !info.Mode().IsRegular() {
		target = filepath.Join(admin.manager.paths.UICurrent, "index.html")
		info, err = os.Stat(target)
	}
	if err != nil || !info.Mode().IsRegular() {
		writer.Header().Set("Content-Type", "text/plain; charset=utf-8")
		writer.Header().Set("Cache-Control", "no-store")
		writer.WriteHeader(http.StatusServiceUnavailable)
		_, _ = io.WriteString(writer, "Sempre UI is not installed. Run: sempre ui install official\n")
		return
	}
	if filepath.Base(target) == "index.html" {
		writer.Header().Set("Cache-Control", "no-cache")
	} else {
		writer.Header().Set("Cache-Control", "public, max-age=31536000, immutable")
	}
	if contentType := mime.TypeByExtension(filepath.Ext(target)); contentType != "" {
		writer.Header().Set("Content-Type", contentType)
	}
	http.ServeFile(writer, request, target)
}

func pathWithin(root, target string) bool {
	relative, err := filepath.Rel(root, target)
	return err == nil && relative != ".." && !strings.HasPrefix(relative, ".."+string(filepath.Separator))
}

func (admin *adminServer) runtimeClient(writer http.ResponseWriter) (*control.Client, bool) {
	client, err := admin.manager.controlClient()
	if err != nil {
		apiWriteError(writer, http.StatusConflict, "CORE_UNAVAILABLE", err.Error(), nil)
		return nil, false
	}
	return client, true
}

func (admin *adminServer) writeRuntimeResult(writer http.ResponseWriter, value any, err error) {
	if err != nil {
		var coreError *control.HTTPError
		if errors.As(err, &coreError) {
			apiWriteError(writer, http.StatusBadGateway, "CORE_API_ERROR", "the managed core rejected the operation", map[string]any{
				"status": coreError.Status, "response": coreError.Body,
			})
			return
		}
		apiWriteError(writer, http.StatusBadGateway, "CORE_UNAVAILABLE", err.Error(), nil)
		return
	}
	apiWriteJSON(writer, http.StatusOK, value)
}

func (admin *adminServer) writeChange(writer http.ResponseWriter, change Change, err error) {
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	if change.NeedsRestart {
		reloaded, reloadErr := admin.manager.RequestReloadIfRunning()
		if reloadErr != nil {
			admin.internalError(writer, reloadErr)
			return
		}
		if !reloaded {
			change.Message += "; it will take effect the next time the managed core starts"
		}
	}
	apiWriteJSON(writer, http.StatusOK, change)
}

func (admin *adminServer) decode(writer http.ResponseWriter, request *http.Request, target any) bool {
	if err := apiDecodeJSON(request, target, MaxConfigSize+64<<10); err != nil {
		apiWriteError(writer, http.StatusBadRequest, "INVALID_REQUEST", err.Error(), nil)
		return false
	}
	return true
}

func (admin *adminServer) operationError(writer http.ResponseWriter, err error) {
	apiWriteError(writer, http.StatusBadRequest, "OPERATION_FAILED", err.Error(), nil)
}

func (admin *adminServer) internalError(writer http.ResponseWriter, err error) {
	fmt.Fprintln(admin.manager.errors, "web API error:", err)
	apiWriteError(writer, http.StatusInternalServerError, "INTERNAL_ERROR", "the operation failed inside Sempre", nil)
}

func apiDecodeJSON(request *http.Request, target any, limit int64) error {
	decoder := json.NewDecoder(io.LimitReader(request.Body, limit+1))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		return fmt.Errorf("decode JSON request: %w", err)
	}
	if decoder.Decode(&struct{}{}) != io.EOF {
		return fmt.Errorf("request must contain one JSON value")
	}
	return nil
}

func apiWriteJSON(writer http.ResponseWriter, status int, value any) {
	writer.Header().Set("Content-Type", "application/json; charset=utf-8")
	writer.Header().Set("Cache-Control", "no-store")
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(value)
}

func apiWriteError(writer http.ResponseWriter, status int, code, message string, details any) {
	apiWriteJSON(writer, status, apiError{Error: apiErrorBody{Code: code, Message: message, Details: details}})
}

func valueOrNil[T any](value T, err error) any {
	if err != nil {
		return nil
	}
	return value
}

func parsePositiveInt(value string, fallback int) int {
	parsed, err := strconv.Atoi(value)
	if err != nil || parsed <= 0 {
		return fallback
	}
	return parsed
}
