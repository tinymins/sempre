package clashproxy

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/tinymins/sempre/internal/core"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

type Config struct {
	External subscriptions.ClashAPIConfig
	Upstream core.ControlSpec
}

type Server struct {
	mu       sync.Mutex
	server   *http.Server
	listener net.Listener
}

func New() *Server {
	return &Server{}
}

func (server *Server) Start(ctx context.Context, config Config) error {
	if !config.External.Enabled {
		return server.Stop(context.Background())
	}
	if config.Upstream.BaseURL == "" || config.Upstream.Secret == "" {
		return fmt.Errorf("internal Clash API is unavailable")
	}
	if err := validateExternalUI(config.External.ExternalUI); err != nil {
		return err
	}
	if err := server.Stop(context.Background()); err != nil {
		return err
	}
	listener, err := net.Listen("tcp", config.External.ExternalController)
	if err != nil {
		return fmt.Errorf("listen on external Clash API %s: %w", config.External.ExternalController, err)
	}
	handler, err := newHandler(config)
	if err != nil {
		_ = listener.Close()
		return err
	}
	httpServer := &http.Server{
		Handler:           handler,
		ReadHeaderTimeout: 10 * time.Second,
		IdleTimeout:       90 * time.Second,
	}
	server.mu.Lock()
	server.server = httpServer
	server.listener = listener
	server.mu.Unlock()
	go func() {
		_ = httpServer.Serve(listener)
	}()
	go func() {
		<-ctx.Done()
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = server.stopInstance(shutdownCtx, httpServer)
	}()
	return nil
}

func (server *Server) Stop(ctx context.Context) error {
	server.mu.Lock()
	httpServer := server.server
	server.server = nil
	server.listener = nil
	server.mu.Unlock()
	if httpServer == nil {
		return nil
	}
	shutdownCtx := ctx
	cancel := func() {}
	if _, hasDeadline := ctx.Deadline(); !hasDeadline {
		shutdownCtx, cancel = context.WithTimeout(ctx, 5*time.Second)
	}
	defer cancel()
	err := httpServer.Shutdown(shutdownCtx)
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return httpServer.Close()
	}
	return err
}

func (server *Server) Address() string {
	server.mu.Lock()
	defer server.mu.Unlock()
	if server.listener == nil {
		return ""
	}
	return server.listener.Addr().String()
}

func (server *Server) stopInstance(ctx context.Context, instance *http.Server) error {
	server.mu.Lock()
	if server.server != instance {
		server.mu.Unlock()
		return nil
	}
	server.server = nil
	server.listener = nil
	server.mu.Unlock()
	err := instance.Shutdown(ctx)
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return instance.Close()
	}
	return err
}

func newHandler(config Config) (http.Handler, error) {
	target, err := url.Parse(config.Upstream.BaseURL)
	if err != nil {
		return nil, fmt.Errorf("parse internal Clash API endpoint: %w", err)
	}
	proxy := httputil.NewSingleHostReverseProxy(target)
	director := proxy.Director
	proxy.Director = func(request *http.Request) {
		director(request)
		request.Host = target.Host
		request.Header.Set("Authorization", "Bearer "+config.Upstream.Secret)
	}
	proxy.ErrorHandler = func(writer http.ResponseWriter, _ *http.Request, failure error) {
		writeJSONError(writer, http.StatusBadGateway, failure.Error())
	}
	var ui http.Handler
	if config.External.ExternalUI != "" {
		ui = http.StripPrefix("/ui/", http.FileServer(http.Dir(config.External.ExternalUI)))
	}
	return http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if !applyCORS(writer, request, config.External) {
			return
		}
		if request.Method == http.MethodOptions {
			writer.WriteHeader(http.StatusNoContent)
			return
		}
		if ui != nil && (request.URL.Path == "/ui" || strings.HasPrefix(request.URL.Path, "/ui/")) {
			if request.URL.Path == "/ui" {
				http.Redirect(writer, request, "/ui/", http.StatusTemporaryRedirect)
				return
			}
			ui.ServeHTTP(writer, request)
			return
		}
		provided := []byte(request.Header.Get("Authorization"))
		expected := []byte("Bearer " + config.External.Secret)
		if subtle.ConstantTimeCompare(provided, expected) != 1 {
			writer.Header().Set("WWW-Authenticate", `Bearer realm="Sempre Clash API"`)
			writeJSONError(writer, http.StatusUnauthorized, "invalid Clash API secret")
			return
		}
		proxy.ServeHTTP(writer, request)
	}), nil
}

func applyCORS(writer http.ResponseWriter, request *http.Request, config subscriptions.ClashAPIConfig) bool {
	origin := request.Header.Get("Origin")
	if origin == "" {
		return true
	}
	allowed := false
	for _, candidate := range config.AllowOrigins {
		if candidate == "*" || candidate == origin {
			allowed = true
			break
		}
	}
	if !allowed {
		if request.Method == http.MethodOptions {
			writeJSONError(writer, http.StatusForbidden, "origin is not allowed")
			return false
		}
		return true
	}
	writer.Header().Add("Vary", "Origin")
	writer.Header().Set("Access-Control-Allow-Origin", origin)
	writer.Header().Set("Access-Control-Allow-Headers", "Authorization, Content-Type")
	writer.Header().Set("Access-Control-Allow-Methods", "GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS")
	if config.AllowPrivateNetwork && request.Header.Get("Access-Control-Request-Private-Network") == "true" {
		writer.Header().Set("Access-Control-Allow-Private-Network", "true")
	}
	return true
}

func validateExternalUI(path string) error {
	if path == "" {
		return nil
	}
	clean, err := filepath.Abs(path)
	if err != nil {
		return fmt.Errorf("resolve external UI path: %w", err)
	}
	info, err := os.Stat(clean)
	if err != nil {
		return fmt.Errorf("inspect external UI path: %w", err)
	}
	if !info.IsDir() {
		return fmt.Errorf("external UI path %s is not a directory", path)
	}
	return nil
}

func writeJSONError(writer http.ResponseWriter, status int, message string) {
	writer.Header().Set("Content-Type", "application/json")
	writer.WriteHeader(status)
	_ = json.NewEncoder(writer).Encode(map[string]string{"error": message})
}
