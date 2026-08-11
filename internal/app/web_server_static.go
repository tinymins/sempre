package app

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"mime"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/tinymins/sempre/internal/control"
	uiassets "github.com/tinymins/sempre/internal/ui"
)

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
