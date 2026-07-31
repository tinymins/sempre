package download

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"

	"github.com/sempre-lab/sempre/internal/buildinfo"
)

const MaxArtifactSize = int64(512 << 20)

type Artifact struct {
	Name   string
	URL    string
	Digest string
	Size   int64
}

func Verified(ctx context.Context, artifact Artifact, destination string) error {
	client := &http.Client{
		Timeout: 15 * time.Minute,
		CheckRedirect: func(request *http.Request, via []*http.Request) error {
			if len(via) >= 10 {
				return fmt.Errorf("too many redirects")
			}
			if !strings.EqualFold(request.URL.Scheme, "https") {
				return fmt.Errorf("refuse non-HTTPS redirect")
			}
			return nil
		},
	}
	return verified(ctx, client, artifact, destination)
}

func verified(ctx context.Context, client *http.Client, artifact Artifact, destination string) error {
	expected, err := parseSHA256(artifact.Digest)
	if err != nil {
		return fmt.Errorf("%s: %w", artifact.Name, err)
	}
	parsed, err := url.Parse(artifact.URL)
	if err != nil || !strings.EqualFold(parsed.Scheme, "https") || parsed.Host == "" {
		return fmt.Errorf("%s has an invalid HTTPS URL", artifact.Name)
	}
	if artifact.Size <= 0 || artifact.Size > MaxArtifactSize {
		return fmt.Errorf("%s has invalid size %d", artifact.Name, artifact.Size)
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, artifact.URL, nil)
	if err != nil {
		return err
	}
	request.Header.Set("User-Agent", "Sempre/"+buildinfo.Version)
	response, err := client.Do(request)
	if err != nil {
		return fmt.Errorf("download %s: %w", artifact.Name, err)
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return fmt.Errorf("download %s: HTTP %s", artifact.Name, response.Status)
	}
	if response.ContentLength > 0 && response.ContentLength != artifact.Size {
		return fmt.Errorf("%s size changed: expected %d, server reports %d", artifact.Name, artifact.Size, response.ContentLength)
	}
	file, err := os.OpenFile(destination, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
	if err != nil {
		return fmt.Errorf("create download: %w", err)
	}
	hash := sha256.New()
	written, copyErr := io.Copy(io.MultiWriter(file, hash), io.LimitReader(response.Body, MaxArtifactSize+1))
	closeErr := file.Close()
	if copyErr != nil {
		return fmt.Errorf("download %s: %w", artifact.Name, copyErr)
	}
	if closeErr != nil {
		return closeErr
	}
	if written != artifact.Size {
		return fmt.Errorf("%s size mismatch: expected %d, got %d", artifact.Name, artifact.Size, written)
	}
	actual := hex.EncodeToString(hash.Sum(nil))
	if !strings.EqualFold(actual, expected) {
		return fmt.Errorf("%s SHA-256 mismatch: expected %s, got %s", artifact.Name, expected, actual)
	}
	return nil
}

func parseSHA256(value string) (string, error) {
	value = strings.TrimSpace(value)
	if !strings.HasPrefix(strings.ToLower(value), "sha256:") {
		return "", fmt.Errorf("release asset does not provide a SHA-256 digest")
	}
	value = value[len("sha256:"):]
	decoded, err := hex.DecodeString(value)
	if err != nil || len(decoded) != sha256.Size {
		return "", fmt.Errorf("invalid SHA-256 digest")
	}
	return strings.ToLower(value), nil
}
