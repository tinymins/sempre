package app

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

const MaxConfigSize = int64(32 << 20)

var errConfigurationValidation = errors.New("configuration validation failed")

func (manager *Manager) ImportConfig(ctx context.Context, source string) (Change, error) {
	return manager.importSubscriptionSource(ctx, source)
}

func (manager *Manager) CurrentConfigContent() ([]byte, string, error) {
	document, err := manager.store.Read()
	if err != nil {
		return nil, "", err
	}
	if document.Selected == nil {
		return nil, "", fmt.Errorf("no core is selected")
	}
	hash := document.Configs[document.Selected.Core]
	if hash == "" {
		return nil, "", fmt.Errorf("selected core has no configuration")
	}
	data, err := os.ReadFile(manager.paths.Config(document.Selected.Core, hash))
	if err != nil {
		return nil, "", fmt.Errorf("read active configuration: %w", err)
	}
	return data, hash, nil
}

func (manager *Manager) ValidateConfigContent(ctx context.Context, data []byte) error {
	if int64(len(data)) > MaxConfigSize {
		return fmt.Errorf("configuration exceeds %d bytes", MaxConfigSize)
	}
	document, err := manager.store.Read()
	if err != nil {
		return err
	}
	target, adapter, err := manager.configurationTarget(document)
	if err != nil {
		return err
	}
	candidate, err := os.CreateTemp(manager.paths.Runtime, "config-validate-*.json")
	if err != nil {
		return err
	}
	path := candidate.Name()
	defer os.Remove(path)
	if _, err := candidate.Write(data); err != nil {
		candidate.Close()
		return err
	}
	if err := candidate.Close(); err != nil {
		return err
	}
	return manager.validateConfiguration(
		ctx,
		adapter,
		manager.paths.CoreBinary(target.Core, target.Repository, target.Version),
		path,
		manager.output,
		manager.errors,
	)
}

func (manager *Manager) SetSubscription(ctx context.Context, value string) (Change, error) {
	return manager.setSubscription(ctx, value)
}

func (manager *Manager) setSubscription(ctx context.Context, value string) (Change, error) {
	if strings.TrimSpace(value) == "" {
		return manager.clearSubscription()
	}
	catalog, profile, _, err := manager.activeProfile()
	_ = catalog
	if err != nil {
		return Change{}, err
	}
	candidate := *profile
	candidate.Sources = []subscriptions.Source{{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true, URL: strings.TrimSpace(value), UserAgent: subscriptions.DefaultUserAgent, FetchMode: subscriptions.FetchAuto}}
	change, _, err := manager.SaveSubscriptionProfile(ctx, candidate.ID, candidate)
	return change, err
}

func (manager *Manager) ClearSubscription() (Change, error) {
	return manager.clearSubscription()
}

func (manager *Manager) clearSubscription() (Change, error) {
	var change Change
	err := manager.withOperation(func() error {
		var clearErr error
		change, clearErr = manager.clearSubscriptionUnlocked()
		return clearErr
	})
	return change, err
}

func (manager *Manager) clearSubscriptionUnlocked() (Change, error) {
	_, profile, _, err := manager.activeProfile()
	if err != nil {
		return Change{}, err
	}
	if len(profile.Sources) == 0 {
		return Change{Message: "subscription sources are already clear"}, nil
	}
	err = manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
		item, err := subscriptions.FindProfile(catalog, profile.ID)
		if err != nil {
			return err
		}
		item.Sources = []subscriptions.Source{}
		return nil
	})
	if err != nil {
		return Change{}, err
	}
	if err := manager.store.Update(func(document *state.Document) error {
		document.Subscription.URL = ""
		document.Subscription.LastCheck = time.Time{}
		document.Subscription.LastChange = time.Time{}
		document.Subscription.LastResult = ""
		return nil
	}); err != nil {
		return Change{}, err
	}
	return Change{Changed: true, Message: "subscription sources cleared; the active configuration was retained"}, nil
}

func (manager *Manager) UpdateSubscription(ctx context.Context) (Change, error) {
	return manager.updateSubscription(ctx)
}

func (manager *Manager) updateSubscription(ctx context.Context) (Change, error) {
	_, profile, _, err := manager.activeProfile()
	if err != nil {
		return Change{}, err
	}
	if !subscriptionProfileHasInputs(*profile) {
		return Change{}, fmt.Errorf("the active subscription profile has no enabled sources or custom nodes")
	}
	change, _, err := manager.RefreshSubscriptionProfile(ctx, profile.ID)
	if err == nil {
		return change, nil
	}
	now := time.Now().UTC()
	recordErr := manager.store.Update(func(document *state.Document) error {
		document.Subscription.LastCheck = now
		document.Subscription.LastResult = "update failed"
		return nil
	})
	if recordErr == nil {
		recordErr = manager.subscriptions.Update(func(catalog *subscriptions.Catalog) error {
			stored, findErr := subscriptions.FindProfile(catalog, profile.ID)
			if findErr != nil {
				return findErr
			}
			stored.LastCheck = now
			stored.LastResult = err.Error()
			return nil
		})
	}
	if recordErr != nil {
		return Change{}, errors.Join(err, fmt.Errorf("record subscription failure: %w", recordErr))
	}
	return Change{}, err
}

func (manager *Manager) SetSubscriptionSchedule(value string) (Change, error) {
	var change Change
	err := manager.withOperation(func() error {
		var err error
		change, err = manager.setSubscriptionSchedule(value)
		return err
	})
	return change, err
}

func (manager *Manager) setSubscriptionSchedule(value string) (Change, error) {
	value = strings.TrimSpace(strings.ToLower(value))
	if value != "off" {
		interval, err := time.ParseDuration(value)
		if err != nil {
			return Change{}, fmt.Errorf("invalid subscription interval: %w", err)
		}
		if interval < 5*time.Minute {
			return Change{}, fmt.Errorf("subscription interval must be at least 5m")
		}
		value = interval.String()
	}
	change := Change{}
	err := manager.store.Update(func(document *state.Document) error {
		if document.Subscription.Interval == value {
			return nil
		}
		document.Subscription.Interval = value
		change.Changed = true
		return nil
	})
	if err != nil {
		return Change{}, err
	}
	if change.Changed {
		change.Message = "subscription schedule set to " + value
	} else {
		change.Message = "subscription schedule is already " + value
	}
	return change, nil
}

func (manager *Manager) SubscriptionStatus() (string, error) {
	catalog, profile, document, err := manager.activeProfile()
	_ = catalog
	if err != nil {
		return "", err
	}
	var builder strings.Builder
	fmt.Fprintln(&builder, "Profile:", profile.Name)
	fmt.Fprintln(&builder, "Profile ID:", profile.ID)
	fmt.Fprintln(&builder, "Sources:", len(profile.Sources))
	fmt.Fprintln(&builder, "Schedule:", document.Subscription.Interval)
	fmt.Fprintln(&builder, "Automatic restart:", document.AutoRestart)
	if !document.Subscription.LastCheck.IsZero() {
		fmt.Fprintln(&builder, "Last check:", document.Subscription.LastCheck.Format(time.RFC3339))
	}
	if document.Subscription.LastResult != "" {
		fmt.Fprintln(&builder, "Last result:", document.Subscription.LastResult)
	}
	if !document.Subscription.LastChange.IsZero() {
		fmt.Fprintln(&builder, "Last change:", document.Subscription.LastChange.Format(time.RFC3339))
	}
	if next, ok := nextSubscriptionCheck(document.Subscription, subscriptionProfileHasScheduledSources(*profile)); ok {
		fmt.Fprintln(&builder, "Next check:", next.Format(time.RFC3339))
	}
	return strings.TrimRight(builder.String(), "\n"), nil
}

func (manager *Manager) activateConfig(
	ctx context.Context,
	data []byte,
	updateSubscription func(*state.Document, bool),
) (Change, error) {
	lease, err := manager.store.AcquireConfig()
	if err != nil {
		return Change{}, err
	}
	defer lease.Release()
	document, err := manager.store.Read()
	if err != nil {
		return Change{}, err
	}
	target, adapter, err := manager.configurationTarget(document)
	if err != nil {
		return Change{}, err
	}
	binary := manager.paths.CoreBinary(target.Core, target.Repository, target.Version)
	candidate, err := os.CreateTemp(manager.paths.Runtime, "config-candidate-*.json")
	if err != nil {
		return Change{}, err
	}
	candidatePath := candidate.Name()
	defer os.Remove(candidatePath)
	if _, err := candidate.Write(data); err != nil {
		candidate.Close()
		return Change{}, err
	}
	if err := candidate.Chmod(0o600); err != nil {
		candidate.Close()
		return Change{}, err
	}
	if err := candidate.Close(); err != nil {
		return Change{}, err
	}
	if err := manager.validateConfiguration(ctx, adapter, binary, candidatePath, manager.output, manager.errors); err != nil {
		return Change{}, fmt.Errorf("%w: %v", errConfigurationValidation, err)
	}
	sum := sha256.Sum256(data)
	hash := hex.EncodeToString(sum[:])
	configPath := manager.paths.Config(target.Core, hash)
	configCreated := false
	if _, err := os.Stat(configPath); errors.Is(err, os.ErrNotExist) {
		if err := state.WriteAtomic(configPath, data, 0o600); err != nil {
			return Change{}, err
		}
		configCreated = true
	} else if err != nil {
		return Change{}, err
	}

	change := Change{}
	err = manager.store.Update(func(document *state.Document) error {
		currentTarget, _, err := manager.configurationTarget(*document)
		if err != nil {
			return err
		}
		if currentTarget.Core != target.Core ||
			currentTarget.Repository != target.Repository ||
			currentTarget.Ref != target.Ref ||
			currentTarget.Version != target.Version {
			return fmt.Errorf("core selection changed while activating configuration; retry the command")
		}
		oldHash := document.Configs[target.Core]
		configChanged := oldHash != hash
		if updateSubscription != nil {
			updateSubscription(document, configChanged)
		}
		document.Configs[target.Core] = hash
		target.ConfigHash = hash
		deploymentChanged := !state.SameDeployment(document.Active, &target)
		if deploymentChanged {
			document.Stage(target)
			change.NeedsRestart = true
		}
		change.Changed = configChanged || deploymentChanged
		change.PreviousDetail = shortHash(oldHash)
		change.CurrentDetail = shortHash(hash)
		return nil
	})
	if err != nil {
		if configCreated {
			_ = os.Remove(configPath)
		}
		return Change{}, err
	}
	if change.Changed {
		change.Message = "configuration updated and validated"
	} else {
		change.Message = "configuration is already current"
	}
	return change, nil
}

func shortHash(hash string) string {
	if len(hash) > 12 {
		return hash[:12]
	}
	return hash
}

func nextSubscriptionCheck(subscription state.Subscription, hasSources bool) (time.Time, bool) {
	if !hasSources || subscription.Interval == "off" {
		return time.Time{}, false
	}
	interval, err := time.ParseDuration(subscription.Interval)
	if err != nil {
		return time.Time{}, false
	}
	if subscription.LastCheck.IsZero() {
		return time.Now().UTC(), true
	}
	return subscription.LastCheck.Add(interval), true
}
