package main

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestControlHandlerRequiresSecretAndReportsVersion(t *testing.T) {
	handler := controlHandler("internal-secret")

	unauthorized := httptest.NewRecorder()
	handler.ServeHTTP(unauthorized, httptest.NewRequest(http.MethodGet, "/version", nil))
	if unauthorized.Code != http.StatusUnauthorized {
		t.Fatalf("unauthorized status = %d", unauthorized.Code)
	}

	request := httptest.NewRequest(http.MethodGet, "/version", nil)
	request.Header.Set("Authorization", "Bearer internal-secret")
	response := httptest.NewRecorder()
	handler.ServeHTTP(response, request)
	if response.Code != http.StatusOK || !strings.Contains(response.Body.String(), `"version":"1.2.3"`) {
		t.Fatalf("version response = %d %q", response.Code, response.Body.String())
	}
}
