package app

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/buildinfo"
	"github.com/tinymins/sempre/internal/state"
)

const MaxConfigSize = int64(32 << 20)

var errConfigurationValidation = errors.New("configuration validation failed")

func (manager *Manager) ImportConfig(ctx context.Context, source string) (Change, error) {
	var change Change
	err := manager.withOperation(func() error {
		var err error
		change, err = manager.importConfig(ctx, source)
		return err
	})
	return change, err
}

func (manager *Manager) SaveConfigContent(ctx context.Context, data []byte) (Change, error) {
	if int64(len(data)) > MaxConfigSize {
		return Change{}, fmt.Errorf("configuration exceeds %d bytes", MaxConfigSize)
	}
	var change Change
	err := manager.withOperation(func() error {
		var err error
		change, err = manager.activateConfig(ctx, data, nil)
		return err
	})
	return change, err
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
		manager.paths.CoreBinary(target.Core, target.Version),
		path,
		manager.output,
		manager.errors,
	)
}

func (manager *Manager) importConfig(ctx context.Context, source string) (Change, error) {
	file, err := os.Open(source)
	if err != nil {
		return Change{}, fmt.Errorf("open configuration: %w", err)
	}
	defer file.Close()
	data, err := readLimited(file, MaxConfigSize)
	if err != nil {
		return Change{}, err
	}
	return manager.activateConfig(ctx, data, nil)
}

func (manager *Manager) SetSubscription(ctx context.Context, value string) (Change, error) {
	var change Change
	err := manager.withOperation(func() error {
		var err error
		change, err = manager.setSubscription(ctx, value)
		return err
	})
	return change, err
}

func (manager *Manager) setSubscription(ctx context.Context, value string) (Change, error) {
	if strings.TrimSpace(value) == "" {
		return manager.clearSubscription()
	}
	parsed, err := validateSubscriptionURL(value)
	if err != nil {
		return Change{}, err
	}
	normalized := parsed.String()
	return manager.downloadSubscription(ctx, normalized, true, false)
}

func (manager *Manager) ClearSubscription() (Change, error) {
	var change Change
	err := manager.withOperation(func() error {
		var err error
		change, err = manager.clearSubscription()
		return err
	})
	return change, err
}

func (manager *Manager) clearSubscription() (Change, error) {
	change := Change{}
	err := manager.store.Update(func(document *state.Document) error {
		if document.Subscription.URL == "" &&
			document.Subscription.LastCheck.IsZero() &&
			document.Subscription.LastChange.IsZero() &&
			document.Subscription.LastResult == "" {
			return nil
		}
		document.Subscription.URL = ""
		document.Subscription.LastCheck = time.Time{}
		document.Subscription.LastChange = time.Time{}
		document.Subscription.LastResult = ""
		change.Changed = true
		change.NeedsRestart = true
		return nil
	})
	if err != nil {
		return Change{}, err
	}
	if change.Changed {
		change.Message = "subscription cleared; the active configuration was retained"
	} else {
		change.Message = "subscription is already clear"
	}
	return change, nil
}

func (manager *Manager) UpdateSubscription(ctx context.Context) (Change, error) {
	var change Change
	err := manager.withOperation(func() error {
		var err error
		change, err = manager.updateSubscription(ctx)
		return err
	})
	return change, err
}

func (manager *Manager) updateSubscription(ctx context.Context) (Change, error) {
	document, err := manager.store.Read()
	if err != nil {
		return Change{}, err
	}
	if document.Subscription.URL == "" {
		return Change{}, fmt.Errorf("no subscription URL is configured")
	}
	return manager.downloadSubscription(ctx, document.Subscription.URL, false, true)
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
		change.NeedsRestart = true
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
	document, err := manager.store.Read()
	if err != nil {
		return "", err
	}
	var builder strings.Builder
	if document.Subscription.URL == "" {
		fmt.Fprintln(&builder, "URL: not configured")
	} else {
		fmt.Fprintln(&builder, "URL:", redactedURL(document.Subscription.URL))
	}
	fmt.Fprintln(&builder, "Schedule:", document.Subscription.Interval)
	if !document.Subscription.LastCheck.IsZero() {
		fmt.Fprintln(&builder, "Last check:", document.Subscription.LastCheck.Format(time.RFC3339))
	}
	if document.Subscription.LastResult != "" {
		fmt.Fprintln(&builder, "Last result:", document.Subscription.LastResult)
	}
	if !document.Subscription.LastChange.IsZero() {
		fmt.Fprintln(&builder, "Last change:", document.Subscription.LastChange.Format(time.RFC3339))
	}
	if next, ok := nextSubscriptionCheck(document.Subscription); ok {
		fmt.Fprintln(&builder, "Next check:", next.Format(time.RFC3339))
	}
	return strings.TrimRight(builder.String(), "\n"), nil
}

func (manager *Manager) downloadSubscription(
	ctx context.Context,
	value string,
	saveURL bool,
	recordFailures bool,
) (Change, error) {
	parsed, err := validateSubscriptionURL(value)
	if err != nil {
		return Change{}, err
	}
	client := &http.Client{
		Timeout: 5 * time.Minute,
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
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, parsed.String(), nil)
	if err != nil {
		return Change{}, err
	}
	request.Header.Set("User-Agent", "Sempre/"+buildinfo.Version)
	response, err := client.Do(request)
	if err != nil {
		failure := fmt.Errorf("download subscription from %s failed: %s", redactedURL(value), safeNetworkError(err))
		return manager.subscriptionFailure(failure, "download failed", recordFailures)
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		failure := fmt.Errorf("download subscription from %s: HTTP %s", redactedURL(value), response.Status)
		return manager.subscriptionFailure(failure, "HTTP "+response.Status, recordFailures)
	}
	data, err := readLimited(response.Body, MaxConfigSize)
	if err != nil {
		return manager.subscriptionFailure(err, "download rejected", recordFailures)
	}
	change, err := manager.activateConfig(ctx, data, func(document *state.Document, changed bool) {
		if saveURL {
			document.Subscription.URL = parsed.String()
		}
		document.Subscription.LastCheck = time.Now().UTC()
		if changed {
			document.Subscription.LastChange = document.Subscription.LastCheck
			document.Subscription.LastResult = "configuration updated"
		} else {
			document.Subscription.LastResult = "no change"
		}
	})
	if err != nil {
		result := "configuration activation failed"
		if errors.Is(err, errConfigurationValidation) {
			result = "configuration validation failed"
		}
		return manager.subscriptionFailure(err, result, recordFailures)
	}
	return change, nil
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
	binary := manager.paths.CoreBinary(target.Core, target.Version)
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

func (manager *Manager) subscriptionFailure(failure error, result string, record bool) (Change, error) {
	if !record {
		return Change{}, failure
	}
	if err := manager.recordSubscriptionResult(result); err != nil {
		return Change{}, errors.Join(failure, fmt.Errorf("record subscription result: %w", err))
	}
	return Change{}, failure
}

func (manager *Manager) recordSubscriptionResult(result string) error {
	return manager.store.Update(func(document *state.Document) error {
		document.Subscription.LastCheck = time.Now().UTC()
		document.Subscription.LastResult = result
		return nil
	})
}

func validateSubscriptionURL(value string) (*url.URL, error) {
	value = strings.TrimSpace(value)
	if value == "" || strings.ContainsAny(value, "\r\n") {
		return nil, fmt.Errorf("subscription URL must be one absolute HTTPS URL")
	}
	parsed, err := url.Parse(value)
	if err != nil ||
		!strings.EqualFold(parsed.Scheme, "https") ||
		parsed.Hostname() == "" ||
		parsed.User != nil {
		return nil, fmt.Errorf("subscription URL must be one absolute HTTPS URL without user information")
	}
	return parsed, nil
}

func redactedURL(value string) string {
	parsed, err := url.Parse(value)
	if err != nil || parsed.Hostname() == "" {
		return "<redacted>"
	}
	return parsed.Scheme + "://" + parsed.Host
}

func safeNetworkError(err error) string {
	var urlError *url.Error
	if errors.As(err, &urlError) && urlError.Err != nil {
		return urlError.Err.Error()
	}
	return "network request failed"
}

func readLimited(reader io.Reader, limit int64) ([]byte, error) {
	data, err := io.ReadAll(io.LimitReader(reader, limit+1))
	if err != nil {
		return nil, err
	}
	if int64(len(data)) > limit {
		return nil, fmt.Errorf("configuration exceeds %d bytes", limit)
	}
	if len(data) == 0 {
		return nil, fmt.Errorf("configuration is empty")
	}
	return data, nil
}

func shortHash(hash string) string {
	if len(hash) > 12 {
		return hash[:12]
	}
	return hash
}

func nextSubscriptionCheck(subscription state.Subscription) (time.Time, bool) {
	if subscription.URL == "" || subscription.Interval == "off" {
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
