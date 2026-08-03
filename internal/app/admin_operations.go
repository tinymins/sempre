package app

import (
	"context"
	"fmt"

	uiassets "github.com/tinymins/sempre/internal/ui"
	"github.com/tinymins/sempre/internal/webconfig"
)

type WebStatus struct {
	Listen       string `json:"listen"`
	LocalURL     string `json:"local_url"`
	PasswordSet  bool   `json:"password_set"`
	PasswordWarn bool   `json:"password_warning"`
}

func (manager *Manager) WebStatus() (WebStatus, error) {
	config, err := manager.web.Read()
	if err != nil {
		return WebStatus{}, err
	}
	localURL, err := webconfig.LocalURL(config.Listen)
	if err != nil {
		return WebStatus{}, err
	}
	return WebStatus{
		Listen: config.Listen, LocalURL: localURL,
		PasswordSet: config.Password != "", PasswordWarn: config.Password == "",
	}, nil
}

func (manager *Manager) SetWebListen(value string) (WebStatus, error) {
	if _, err := manager.web.Update(func(config *webconfig.Config) error {
		config.Listen = value
		return nil
	}); err != nil {
		return WebStatus{}, err
	}
	return manager.WebStatus()
}

func (manager *Manager) SetAdministratorPassword(value string) (WebStatus, error) {
	if _, err := manager.web.SetPassword(value); err != nil {
		return WebStatus{}, err
	}
	return manager.WebStatus()
}

func (manager *Manager) UIStatus() (uiassets.Metadata, error) {
	return manager.ui.Current()
}

func (manager *Manager) InstallUI(ctx context.Context, source, digest string) (uiassets.Metadata, error) {
	switch source {
	case "", "official":
		return manager.InstallOfficialUI(ctx)
	default:
		if len(source) >= 8 && source[:8] == "https://" {
			return manager.ui.InstallURL(ctx, source, digest)
		}
		return manager.ui.InstallFile(source, "local", source, digest)
	}
}

func (manager *Manager) UpdateUI(ctx context.Context) (uiassets.Metadata, error) {
	current, err := manager.ui.Current()
	if err != nil {
		return uiassets.Metadata{}, err
	}
	switch current.SourceType {
	case "official":
		return manager.InstallOfficialUI(ctx)
	case "url":
		return manager.ui.InstallURL(ctx, current.Source, "")
	default:
		return uiassets.Metadata{}, fmt.Errorf("locally installed UI has no update source")
	}
}

func (manager *Manager) RemoveUI() error {
	return manager.ui.Remove()
}
