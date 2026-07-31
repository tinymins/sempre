package download

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestVerifiedDownload(t *testing.T) {
	t.Parallel()
	content := []byte("verified artifact")
	hash := sha256.Sum256(content)
	server := httptest.NewTLSServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		response.Header().Set("Content-Length", "17")
		_, _ = response.Write(content)
	}))
	defer server.Close()
	destination := filepath.Join(t.TempDir(), "artifact")
	err := verified(context.Background(), server.Client(), Artifact{
		Name:   "artifact",
		URL:    server.URL,
		Digest: "sha256:" + hex.EncodeToString(hash[:]),
		Size:   int64(len(content)),
	}, destination)
	if err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(destination)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != string(content) {
		t.Fatalf("content = %q", data)
	}
}

func TestVerifiedDownloadRejectsDigestMismatch(t *testing.T) {
	t.Parallel()
	content := []byte("artifact")
	server := httptest.NewTLSServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		_, _ = response.Write(content)
	}))
	defer server.Close()
	err := verified(context.Background(), server.Client(), Artifact{
		Name:   "artifact",
		URL:    server.URL,
		Digest: "sha256:" + strings.Repeat("0", 64),
		Size:   int64(len(content)),
	}, filepath.Join(t.TempDir(), "artifact"))
	if err == nil || !strings.Contains(err.Error(), "mismatch") {
		t.Fatalf("error = %v", err)
	}
}

func TestParseSHA256RequiresTypedDigest(t *testing.T) {
	t.Parallel()
	if _, err := parseSHA256(strings.Repeat("0", 64)); err == nil {
		t.Fatal("untyped digest was accepted")
	}
}
