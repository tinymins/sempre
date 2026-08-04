package subscription

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/state"
)

type Fetcher struct {
	store *Store
	now   func() time.Time
}

type cacheEntry struct {
	URL          string    `json:"url"`
	UserAgent    string    `json:"user_agent"`
	FetchMode    string    `json:"fetch_mode"`
	SnapshotHash string    `json:"snapshot_hash"`
	FetchedAt    time.Time `json:"fetched_at"`
}

func NewFetcher(store *Store) *Fetcher {
	return &Fetcher{store: store, now: func() time.Time { return time.Now().UTC() }}
}

func (fetcher *Fetcher) Load(ctx context.Context, source Source, force bool) ([]byte, Source, bool, error) {
	return fetcher.load(ctx, source, force, nil)
}

func (fetcher *Fetcher) LoadValidated(
	ctx context.Context,
	source Source,
	force bool,
	validate func([]byte) error,
) ([]byte, Source, bool, error) {
	return fetcher.load(ctx, source, force, validate)
}

func (fetcher *Fetcher) load(
	ctx context.Context,
	source Source,
	force bool,
	validate func([]byte) error,
) ([]byte, Source, bool, error) {
	if source.Type == SourceRaw {
		data := []byte(source.Content)
		if validate != nil {
			if err := validate(data); err != nil {
				return nil, source, false, err
			}
		}
		hash, err := fetcher.store.SaveBlob(data)
		if err != nil {
			return nil, source, false, err
		}
		source.SnapshotHash = hash
		source.FetchedAt = fetcher.now()
		source.LastStatus = "raw content"
		source.LastError = ""
		return data, source, false, nil
	}
	if err := ValidateSource(source); err != nil {
		return nil, source, false, err
	}
	if source.UserAgent == "" {
		source.UserAgent = DefaultUserAgent
	}
	if source.FetchMode == "" {
		source.FetchMode = FetchAuto
	}
	key := strings.Join([]string{source.URL, source.UserAgent, source.FetchMode}, "\x00")
	cache, cacheErr := fetcher.readCache(key)
	ttl := DefaultCacheTTL
	if source.CacheTTLMinutes > 0 {
		ttl = time.Duration(source.CacheTTLMinutes) * time.Minute
	}
	if !force && cacheErr == nil && fetcher.now().Before(cache.FetchedAt.Add(ttl)) {
		data, err := fetcher.store.ReadBlob(cache.SnapshotHash)
		if err == nil && (validate == nil || validate(data) == nil) {
			source.SnapshotHash, source.FetchedAt, source.LastStatus, source.LastError = cache.SnapshotHash, cache.FetchedAt, "fresh cache", ""
			return data, source, true, nil
		}
	}

	data, err := fetcher.download(ctx, source)
	if err == nil && validate != nil {
		if validationErr := validate(data); validationErr != nil {
			err = fmt.Errorf("downloaded content is unusable: %w", validationErr)
		}
	}
	if err != nil {
		if cacheErr == nil {
			cached, readErr := fetcher.store.ReadBlob(cache.SnapshotHash)
			if readErr == nil && (validate == nil || validate(cached) == nil) {
				source.SnapshotHash, source.FetchedAt, source.LastStatus, source.LastError = cache.SnapshotHash, cache.FetchedAt, "last-known-good cache", err.Error()
				return cached, source, true, nil
			}
		}
		source.LastStatus, source.LastError = "fetch failed", err.Error()
		return nil, source, false, err
	}
	hash, err := fetcher.store.SaveBlob(data)
	if err != nil {
		return nil, source, false, err
	}
	entry := cacheEntry{URL: source.URL, UserAgent: source.UserAgent, FetchMode: source.FetchMode, SnapshotHash: hash, FetchedAt: fetcher.now()}
	if err := fetcher.writeCache(key, entry); err != nil {
		return nil, source, false, err
	}
	source.SnapshotHash, source.FetchedAt, source.LastStatus, source.LastError = hash, entry.FetchedAt, "downloaded", ""
	return data, source, false, nil
}

func (fetcher *Fetcher) download(ctx context.Context, source Source) ([]byte, error) {
	transport := http.DefaultTransport.(*http.Transport).Clone()
	defer transport.CloseIdleConnections()
	if source.FetchMode == FetchDomesticDirect {
		proxyValue := strings.TrimSpace(os.Getenv("DIRECT_PROXY_URL"))
		if proxyValue == "" {
			return nil, fmt.Errorf("domestic-direct fetch mode requires DIRECT_PROXY_URL")
		}
		proxyURL, err := url.Parse(proxyValue)
		if err != nil || proxyURL.Host == "" || (proxyURL.Scheme != "http" && proxyURL.Scheme != "https") {
			return nil, fmt.Errorf("DIRECT_PROXY_URL must be an absolute HTTP or HTTPS URL")
		}
		username, hasUsername := os.LookupEnv("DIRECT_PROXY_USERNAME")
		password, hasPassword := os.LookupEnv("DIRECT_PROXY_PASSWORD")
		if hasUsername != hasPassword || (hasUsername && (username == "" || password == "")) {
			return nil, fmt.Errorf("DIRECT_PROXY_USERNAME and DIRECT_PROXY_PASSWORD must be configured together")
		}
		if hasUsername {
			proxyURL.User = url.UserPassword(username, password)
		}
		transport.Proxy = http.ProxyURL(proxyURL)
	}
	client := &http.Client{
		Transport: transport,
		Timeout:   15 * time.Second,
		CheckRedirect: func(request *http.Request, via []*http.Request) error {
			if len(via) >= 10 {
				return fmt.Errorf("too many redirects")
			}
			if request.URL.Hostname() == "" || request.URL.User != nil || (request.URL.Scheme != "http" && request.URL.Scheme != "https") {
				return fmt.Errorf("refuse invalid redirect target")
			}
			return nil
		},
	}
	var failures []error
	for attempt := 1; attempt <= 3; attempt++ {
		request, err := http.NewRequestWithContext(ctx, http.MethodGet, source.URL, nil)
		if err != nil {
			return nil, err
		}
		request.Header.Set("User-Agent", source.UserAgent)
		response, err := client.Do(request)
		if err == nil {
			data, readErr := readResponse(response)
			response.Body.Close()
			if readErr == nil {
				return data, nil
			}
			err = readErr
		}
		failures = append(failures, fmt.Errorf("attempt %d: %w", attempt, err))
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
	}
	return nil, fmt.Errorf("download subscription failed after 3 attempts: %w", errors.Join(failures...))
}

func readResponse(response *http.Response) ([]byte, error) {
	if response.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("HTTP %s", response.Status)
	}
	reader := io.LimitReader(response.Body, MaxSourceSize+1)
	data, err := io.ReadAll(reader)
	if err != nil {
		return nil, err
	}
	if int64(len(data)) > MaxSourceSize {
		return nil, fmt.Errorf("subscription response exceeds %d bytes", MaxSourceSize)
	}
	if strings.TrimSpace(string(data)) == "" {
		return nil, fmt.Errorf("subscription response is empty")
	}
	return data, nil
}

func (fetcher *Fetcher) readCache(key string) (cacheEntry, error) {
	data, err := os.ReadFile(fetcher.store.CachePath(key))
	if err != nil {
		return cacheEntry{}, err
	}
	var entry cacheEntry
	if err := json.Unmarshal(data, &entry); err != nil {
		return cacheEntry{}, err
	}
	if entry.SnapshotHash == "" || entry.FetchedAt.IsZero() {
		return cacheEntry{}, fmt.Errorf("invalid cache entry")
	}
	return entry, nil
}

func (fetcher *Fetcher) writeCache(key string, entry cacheEntry) error {
	data, err := json.MarshalIndent(entry, "", "  ")
	if err != nil {
		return err
	}
	return state.WriteAtomic(fetcher.store.CachePath(key), append(data, '\n'), 0o600)
}
