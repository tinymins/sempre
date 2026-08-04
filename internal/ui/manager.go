package ui

import (
	"archive/zip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/buildinfo"
)

const (
	ManifestName      = "sempre-ui.json"
	MetadataName      = ".sempre-source.json"
	MaxArchiveSize    = int64(64 << 20)
	MaxExpandedSize   = int64(128 << 20)
	MaxExtractedFiles = 4096
)

type Manifest struct {
	Schema  int    `json:"schema"`
	Name    string `json:"name"`
	Version string `json:"version"`
	Entry   string `json:"entry"`
	API     API    `json:"api"`
}

type API struct {
	Major int `json:"major"`
}

type Metadata struct {
	Manifest    Manifest  `json:"manifest"`
	SourceType  string    `json:"source_type"`
	Source      string    `json:"source"`
	Digest      string    `json:"sha256"`
	InstalledAt time.Time `json:"installed_at"`
}

type Manager struct {
	root    string
	current string
	http    *http.Client
}

func New(root, current string) *Manager {
	return &Manager{
		root:    root,
		current: current,
		http: &http.Client{
			Timeout:       10 * time.Minute,
			CheckRedirect: httpsRedirect,
		},
	}
}

func (manager *Manager) Current() (Metadata, error) {
	data, err := os.ReadFile(filepath.Join(manager.current, MetadataName))
	if err != nil {
		return Metadata{}, err
	}
	var metadata Metadata
	if err := json.Unmarshal(data, &metadata); err != nil {
		return Metadata{}, fmt.Errorf("decode UI metadata: %w", err)
	}
	if err := validateManifest(metadata.Manifest); err != nil {
		return Metadata{}, err
	}
	return metadata, nil
}

func (manager *Manager) Installed() bool {
	_, err := manager.Current()
	return err == nil
}

func (manager *Manager) InstallFile(path, sourceType, source, expectedDigest string) (Metadata, error) {
	file, err := os.Open(path)
	if err != nil {
		return Metadata{}, fmt.Errorf("open UI archive: %w", err)
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		return Metadata{}, err
	}
	if info.Size() <= 0 || info.Size() > MaxArchiveSize {
		return Metadata{}, fmt.Errorf("UI archive size must be between 1 and %d bytes", MaxArchiveSize)
	}
	hash := sha256.New()
	if _, err := io.Copy(hash, file); err != nil {
		return Metadata{}, err
	}
	digest := hex.EncodeToString(hash.Sum(nil))
	if expectedDigest != "" && !strings.EqualFold(strings.TrimPrefix(expectedDigest, "sha256:"), digest) {
		return Metadata{}, fmt.Errorf("UI SHA-256 mismatch: expected %s, got %s", expectedDigest, digest)
	}

	staging, err := os.MkdirTemp(filepath.Dir(manager.root), ".sempre-ui-*")
	if err != nil {
		return Metadata{}, fmt.Errorf("create UI staging directory: %w", err)
	}
	defer os.RemoveAll(staging)
	if err := extract(path, staging); err != nil {
		return Metadata{}, err
	}
	manifest, err := readManifest(staging)
	if err != nil {
		return Metadata{}, err
	}
	metadata := Metadata{
		Manifest:    manifest,
		SourceType:  sourceType,
		Source:      source,
		Digest:      digest,
		InstalledAt: time.Now().UTC(),
	}
	if err := writeJSON(filepath.Join(staging, MetadataName), metadata); err != nil {
		return Metadata{}, err
	}
	if err := manager.activate(staging); err != nil {
		return Metadata{}, err
	}
	return metadata, nil
}

func (manager *Manager) InstallURL(ctx context.Context, value, expectedDigest string) (Metadata, error) {
	return manager.InstallRemote(ctx, value, expectedDigest, "url")
}

func (manager *Manager) InstallRemote(ctx context.Context, value, expectedDigest, sourceType string) (Metadata, error) {
	return manager.installRemote(ctx, value, expectedDigest, sourceType, "")
}

func (manager *Manager) InstallRemoteSource(ctx context.Context, value, expectedDigest, sourceType, source string) (Metadata, error) {
	return manager.installRemote(ctx, value, expectedDigest, sourceType, source)
}

func (manager *Manager) installRemote(ctx context.Context, value, expectedDigest, sourceType, source string) (Metadata, error) {
	parsed, err := url.Parse(value)
	if err != nil || !strings.EqualFold(parsed.Scheme, "https") || parsed.Hostname() == "" || parsed.User != nil {
		return Metadata{}, fmt.Errorf("UI URL must be an HTTPS URL without credentials")
	}
	if err := os.MkdirAll(manager.root, 0o700); err != nil {
		return Metadata{}, err
	}
	temporary, err := os.CreateTemp(manager.root, ".download-*.zip")
	if err != nil {
		return Metadata{}, err
	}
	path := temporary.Name()
	defer os.Remove(path)
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, parsed.String(), nil)
	if err != nil {
		temporary.Close()
		return Metadata{}, err
	}
	request.Header.Set("User-Agent", "Sempre/"+buildinfo.Version)
	response, err := manager.http.Do(request)
	if err != nil {
		temporary.Close()
		return Metadata{}, fmt.Errorf("download UI: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		temporary.Close()
		return Metadata{}, fmt.Errorf("download UI: HTTP %s", response.Status)
	}
	if response.ContentLength > MaxArchiveSize {
		temporary.Close()
		return Metadata{}, fmt.Errorf("UI archive exceeds %d bytes", MaxArchiveSize)
	}
	written, copyErr := io.Copy(temporary, io.LimitReader(response.Body, MaxArchiveSize+1))
	closeErr := temporary.Close()
	if copyErr != nil {
		return Metadata{}, copyErr
	}
	if closeErr != nil {
		return Metadata{}, closeErr
	}
	if written > MaxArchiveSize {
		return Metadata{}, fmt.Errorf("UI archive exceeds %d bytes", MaxArchiveSize)
	}
	if source == "" {
		source = parsed.String()
	}
	return manager.InstallFile(path, sourceType, source, expectedDigest)
}

func (manager *Manager) Remove() error {
	if err := os.RemoveAll(manager.current); err != nil {
		return fmt.Errorf("remove UI: %w", err)
	}
	return nil
}

func (manager *Manager) activate(staging string) error {
	if err := os.MkdirAll(manager.root, 0o700); err != nil {
		return err
	}
	backup := manager.current + ".previous"
	_ = os.RemoveAll(backup)
	if _, err := os.Stat(manager.current); err == nil {
		if err := os.Rename(manager.current, backup); err != nil {
			return fmt.Errorf("stage previous UI: %w", err)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if err := os.Rename(staging, manager.current); err != nil {
		if _, restoreErr := os.Stat(backup); restoreErr == nil {
			_ = os.Rename(backup, manager.current)
		}
		return fmt.Errorf("activate UI: %w", err)
	}
	if err := os.RemoveAll(backup); err != nil {
		return fmt.Errorf("UI installed but previous-version cleanup failed: %w", err)
	}
	return nil
}

func extract(path, destination string) error {
	reader, err := zip.OpenReader(path)
	if err != nil {
		return fmt.Errorf("open UI ZIP: %w", err)
	}
	defer reader.Close()
	if len(reader.File) > MaxExtractedFiles {
		return fmt.Errorf("UI archive contains more than %d entries", MaxExtractedFiles)
	}
	var expanded int64
	for _, entry := range reader.File {
		if entry.UncompressedSize64 > uint64(MaxExpandedSize) || expanded > MaxExpandedSize-int64(entry.UncompressedSize64) {
			return fmt.Errorf("UI archive expands beyond %d bytes", MaxExpandedSize)
		}
		expanded += int64(entry.UncompressedSize64)
		target, err := safeTarget(destination, entry.Name)
		if err != nil {
			return err
		}
		mode := entry.Mode()
		if mode&os.ModeSymlink != 0 || (!mode.IsRegular() && !entry.FileInfo().IsDir()) {
			return fmt.Errorf("UI archive contains unsupported entry %q", entry.Name)
		}
		if entry.FileInfo().IsDir() {
			if err := os.MkdirAll(target, 0o700); err != nil {
				return err
			}
			continue
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
			return err
		}
		source, err := entry.Open()
		if err != nil {
			return err
		}
		file, err := os.OpenFile(target, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
		if err != nil {
			source.Close()
			return err
		}
		_, copyErr := io.Copy(file, source)
		closeErr := errors.Join(file.Close(), source.Close())
		if copyErr != nil {
			return copyErr
		}
		if closeErr != nil {
			return closeErr
		}
	}
	return nil
}

func safeTarget(destination, name string) (string, error) {
	converted := filepath.FromSlash(name)
	if filepath.IsAbs(converted) || filepath.VolumeName(converted) != "" {
		return "", fmt.Errorf("UI archive entry is absolute: %q", name)
	}
	target := filepath.Join(destination, converted)
	relative, err := filepath.Rel(destination, target)
	if err != nil || relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("UI archive entry escapes its root: %q", name)
	}
	return target, nil
}

func readManifest(root string) (Manifest, error) {
	data, err := os.ReadFile(filepath.Join(root, ManifestName))
	if err != nil {
		return Manifest{}, fmt.Errorf("read %s: %w", ManifestName, err)
	}
	var manifest Manifest
	if err := json.Unmarshal(data, &manifest); err != nil {
		return Manifest{}, fmt.Errorf("decode %s: %w", ManifestName, err)
	}
	if err := validateManifest(manifest); err != nil {
		return Manifest{}, err
	}
	if info, err := os.Stat(filepath.Join(root, manifest.Entry)); err != nil || !info.Mode().IsRegular() {
		return Manifest{}, fmt.Errorf("UI entry %q is unavailable", manifest.Entry)
	}
	return manifest, nil
}

func validateManifest(manifest Manifest) error {
	if manifest.Schema != 1 || manifest.API.Major != 1 {
		return fmt.Errorf("UI manifest is incompatible with Sempre API v1")
	}
	if strings.TrimSpace(manifest.Name) == "" || strings.TrimSpace(manifest.Version) == "" {
		return fmt.Errorf("UI manifest name and version are required")
	}
	if manifest.Entry != "index.html" {
		return fmt.Errorf("UI manifest entry must be index.html")
	}
	return nil
}

func writeJSON(path string, value any) error {
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(data, '\n'), 0o600)
}

func httpsRedirect(request *http.Request, via []*http.Request) error {
	if len(via) >= 10 {
		return fmt.Errorf("too many redirects")
	}
	if !strings.EqualFold(request.URL.Scheme, "https") {
		return fmt.Errorf("refuse non-HTTPS redirect")
	}
	return nil
}
