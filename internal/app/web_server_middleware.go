package app

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/buildinfo"
	"github.com/tinymins/sempre/internal/control"
	"github.com/tinymins/sempre/internal/controlplane"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/webconfig"
)

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
	serviceState, err := admin.manager.service.Status(request.Context())
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	if serviceState == service.NotInstalled {
		apiWriteError(writer, http.StatusConflict, "SERVICE_NOT_INSTALLED", "system service is not installed", nil)
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

func (admin *adminServer) bundleExport(writer http.ResponseWriter, request *http.Request) {
	directory, err := os.MkdirTemp(admin.manager.paths.Runtime, "bundle-export-*")
	if err != nil {
		admin.internalError(writer, err)
		return
	}
	defer os.RemoveAll(directory)
	result, err := admin.manager.ExportBundle(request.Context(), directory)
	if err != nil {
		admin.operationError(writer, err)
		return
	}
	writer.Header().Set("Content-Type", "application/zip")
	writer.Header().Set("Content-Disposition", fmt.Sprintf("attachment; filename=%q", filepath.Base(result.Archive)))
	http.ServeFile(writer, request, result.Archive)
}
