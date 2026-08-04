package app

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"net/url"
	"os"
	"path/filepath"
	"strings"

	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
	uiassets "github.com/tinymins/sempre/internal/ui"
)

const DefaultInstallCore = "sing-box@stable"

type BootstrapOptions struct {
	Core         string
	Subscription string
	UI           string
	UISHA256     string
}

type BootstrapResult struct {
	CoreReference     string
	SubscriptionID    string
	UI                *uiassets.Metadata
	RuntimeTarget     *RuntimeDeployment
	PreviousService   service.State
	ApplicationChange Change
}

func (manager *Manager) BootstrapApplication(ctx context.Context, options BootstrapOptions) (BootstrapResult, error) {
	if err := manager.validateBootstrapOptions(options); err != nil {
		return BootstrapResult{}, err
	}
	previousService, err := manager.service.Status(ctx)
	if err != nil {
		return BootstrapResult{}, err
	}
	result := BootstrapResult{PreviousService: previousService}

	coreReference, coreChange, err := manager.prepareBootstrapCore(ctx, options.Core, options.Subscription == "")
	if err != nil {
		return result, err
	}
	result.CoreReference = coreReference
	result.ApplicationChange = coreChange

	if options.Subscription != "" {
		profile, changed, err := manager.prepareDefaultSubscription(options.Subscription)
		if err != nil {
			return result, err
		}
		result.SubscriptionID = profile.ID
		change, _, err := manager.RefreshSubscriptionProfile(ctx, profile.ID)
		if err != nil {
			return result, err
		}
		result.ApplicationChange.Changed = result.ApplicationChange.Changed || changed || change.Changed
		result.ApplicationChange.NeedsRestart = result.ApplicationChange.NeedsRestart || change.NeedsRestart
		result.ApplicationChange.Message = change.Message
		result.ApplicationChange.CurrentDetail = change.CurrentDetail
	}

	metadata, installed, err := manager.prepareBootstrapUI(ctx, options.UI, options.UISHA256)
	if err != nil {
		return result, err
	}
	if installed {
		result.UI = &metadata
	}
	status, err := manager.ManagedRuntimeStatus()
	if err != nil {
		return result, err
	}
	if status.Active != nil {
		result.RuntimeTarget = status.Active
	} else if status.Target != nil {
		result.RuntimeTarget = status.Target
	}
	if err := manager.InstallApplication(ctx, true); err != nil {
		return result, err
	}
	return result, nil
}

func (manager *Manager) validateBootstrapOptions(options BootstrapOptions) error {
	if options.Core != "" {
		if _, _, err := manager.resolveReference(options.Core); err != nil {
			return err
		}
	}
	if options.Subscription != "" {
		source := subscriptions.Source{
			ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true,
			URL: strings.TrimSpace(options.Subscription), FetchMode: subscriptions.FetchAuto,
		}
		if err := subscriptions.ValidateSource(source); err != nil {
			return err
		}
	}
	source := strings.TrimSpace(options.UI)
	switch {
	case source == "" || source == "official":
		if options.UISHA256 != "" {
			return fmt.Errorf("--ui-sha256 requires an HTTPS UI URL")
		}
	case strings.HasPrefix(source, "https://"):
		parsed, err := url.Parse(source)
		if err != nil || parsed.Hostname() == "" || parsed.User != nil {
			return fmt.Errorf("UI URL must be an HTTPS URL without credentials")
		}
		if options.UISHA256 != "" {
			value := strings.TrimPrefix(strings.ToLower(strings.TrimSpace(options.UISHA256)), "sha256:")
			decoded, err := hex.DecodeString(value)
			if err != nil || len(decoded) != sha256.Size {
				return fmt.Errorf("--ui-sha256 must be a 64-character SHA-256 digest")
			}
		}
	default:
		if options.UISHA256 != "" {
			return fmt.Errorf("--ui-sha256 is not accepted for GitHub UI references")
		}
		if _, err := uiassets.ParseGitHubReference(source); err != nil {
			return fmt.Errorf("invalid bootstrap UI source: use official, an HTTPS URL, or owner/repository@stable|version: %w", err)
		}
	}
	return nil
}

func (manager *Manager) prepareBootstrapCore(ctx context.Context, requested string, compileProfile bool) (string, Change, error) {
	document, err := manager.store.Read()
	if err != nil {
		return "", Change{}, err
	}
	value := bootstrapCoreReference(document, requested)
	if value == "" {
		return selectionRef(*document.Selected).String(), Change{}, nil
	}
	var combined Change
	err = manager.withOperation(func() error {
		installed, err := manager.installCore(ctx, value)
		if err != nil {
			return err
		}
		selected, err := manager.useCore(ctx, value)
		if err != nil {
			return err
		}
		combined.Changed = installed.Changed || selected.Changed
		combined.NeedsRestart = installed.NeedsRestart || selected.NeedsRestart
		combined.Message = selected.Message
		combined.CurrentDetail = selected.CurrentDetail
		combined.PreviousDetail = selected.PreviousDetail
		return nil
	})
	if err != nil {
		return "", Change{}, err
	}
	if compileProfile {
		combined, err = manager.compileSelectedProfileIfNeeded(ctx, combined)
		if err != nil {
			return "", Change{}, err
		}
	}
	document, err = manager.store.Read()
	if err != nil {
		return "", Change{}, err
	}
	return selectionRef(*document.Selected).String(), combined, nil
}

func bootstrapCoreReference(document state.Document, requested string) string {
	value := strings.TrimSpace(requested)
	if value == "" && document.Selected == nil {
		return DefaultInstallCore
	}
	return value
}

func (manager *Manager) prepareDefaultSubscription(value string) (subscriptions.Profile, bool, error) {
	value = strings.TrimSpace(value)
	var prepared subscriptions.Profile
	changed := false
	err := manager.withOperation(func() error {
		if err := manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
			index := -1
			for candidate := range catalog.Profiles {
				if strings.TrimSpace(catalog.Profiles[candidate].Name) == "" {
					index = candidate
					break
				}
			}
			if index < 0 {
				profile := subscriptions.NewProfile("")
				catalog.Profiles = append([]subscriptions.Profile{profile}, catalog.Profiles...)
				index = 0
				changed = true
			}
			profile := &catalog.Profiles[index]
			matches := false
			sources := make([]subscriptions.Source, 0, len(profile.Sources)+1)
			for _, source := range profile.Sources {
				if source.Type != subscriptions.SourceURL || strings.TrimSpace(source.URL) != value {
					sources = append(sources, source)
					continue
				}
				if matches {
					changed = true
					continue
				}
				matches = true
				if !source.Enabled || source.URL != value {
					changed = true
				}
				source.Enabled = true
				source.URL = value
				if source.UserAgent == "" {
					source.UserAgent = subscriptions.DefaultUserAgent
				}
				if source.FetchMode == "" {
					source.FetchMode = subscriptions.FetchAuto
				}
				sources = append(sources, source)
			}
			if !matches {
				sources = append(sources, subscriptions.Source{
					ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true,
					URL: value, UserAgent: subscriptions.DefaultUserAgent, FetchMode: subscriptions.FetchAuto,
				})
				changed = true
			}
			profile.Sources = sources
			prepared = *profile
			return nil
		}); err != nil {
			return err
		}
		return manager.store.Update(func(document *state.Document) error {
			if document.ActiveProfileID != prepared.ID {
				document.ActiveProfileID = prepared.ID
				changed = true
			}
			return nil
		})
	})
	return prepared, changed, err
}

func (manager *Manager) prepareBootstrapUI(ctx context.Context, source, digest string) (uiassets.Metadata, bool, error) {
	source = strings.TrimSpace(source)
	if source == "" {
		current, err := manager.ui.Current()
		if err == nil && current.SourceType != "official" {
			return uiassets.Metadata{}, false, nil
		}
		if err != nil && !errors.Is(err, os.ErrNotExist) {
			return uiassets.Metadata{}, false, err
		}
		source = "official"
	}
	if source != "official" {
		metadata, err := manager.InstallUI(ctx, source, digest)
		return metadata, err == nil, err
	}
	executable, err := layout.CurrentExecutable()
	if err != nil {
		return uiassets.Metadata{}, false, err
	}
	resources := filepath.Join(filepath.Dir(executable), "resources")
	if metadata, found, err := manager.installBundledUIFrom(resources); found || err != nil {
		return metadata, err == nil, err
	}
	metadata, err := manager.InstallOfficialUI(ctx)
	return metadata, err == nil, err
}

func (result BootstrapResult) InstalledFresh() bool {
	return result.PreviousService == service.NotInstalled
}
