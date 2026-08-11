package app

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"

	"github.com/tinymins/sempre/internal/buildinfo"
	"github.com/tinymins/sempre/internal/release"
	uiassets "github.com/tinymins/sempre/internal/ui"
	"github.com/tinymins/sempre/internal/webconfig"
)

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
	case "github":
		metadata, err = admin.manager.ui.InstallGitHub(request.Context(), admin.manager.uiReleases, current.Source)
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
	return manager.installBundledUIFrom(manager.paths.Resources)
}

func (manager *Manager) installBundledUIFrom(resources string) (uiassets.Metadata, bool, error) {
	archive := filepath.Join(resources, "sempre-ui.zip")
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
	digest, err := checksumFromFile(filepath.Join(resources, "SHA256SUMS"), "sempre-ui.zip")
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
