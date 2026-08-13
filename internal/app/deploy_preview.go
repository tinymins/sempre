package app

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"reflect"
	"strings"

	"github.com/tinymins/sempre/internal/gateway"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/state"
	"github.com/tinymins/sempre/internal/tunnel"
	uiassets "github.com/tinymins/sempre/internal/ui"
	"github.com/tinymins/sempre/internal/webconfig"
)

type replacementPreview struct {
	Meaningful bool
	Same       bool
	Summary    string
}

func (manager *Manager) previewReplacement(
	target layout.Layout,
	sourceDocument, targetDocument state.Document,
	targetSubscriptions bool,
) (replacementPreview, error) {
	sourceSubscriptions, err := manager.meaningfulSubscriptionData(manager.paths, sourceDocument)
	if err != nil {
		return replacementPreview{}, err
	}
	sameSubscriptions, err := manager.sameSubscriptionCatalog(target)
	if err != nil {
		return replacementPreview{}, err
	}
	sourceTunnel, err := manager.tunnels.Read()
	if err != nil {
		return replacementPreview{}, err
	}
	targetTunnel, err := readTunnelConfig(target.TunnelConfig)
	if err != nil {
		return replacementPreview{}, err
	}
	sourceGateway, err := manager.gateway.Read()
	if err != nil {
		return replacementPreview{}, err
	}
	targetGateway, err := readGatewayConfig(target.GatewayConfig)
	if err != nil {
		return replacementPreview{}, err
	}
	sourceWeb, err := manager.web.Read()
	if err != nil {
		return replacementPreview{}, err
	}
	targetWeb, err := readWebConfig(target.WebConfig)
	if err != nil {
		return replacementPreview{}, err
	}
	sourceUI, err := readCurrentUI(manager.paths)
	if err != nil {
		return replacementPreview{}, err
	}
	targetUI, err := readCurrentUI(target)
	if err != nil {
		return replacementPreview{}, err
	}

	sameState := sameDeploymentData(sourceDocument, targetDocument)
	sameTunnel := reflect.DeepEqual(sourceTunnel, targetTunnel)
	sameGateway := reflect.DeepEqual(sourceGateway, targetGateway)
	sameWeb := reflect.DeepEqual(sourceWeb, targetWeb)
	sameUI := equalUI(sourceUI, targetUI)
	meaningful := meaningfulState(targetDocument) || targetSubscriptions || len(targetTunnel.Instances) != 0 ||
		!reflect.DeepEqual(targetGateway, gateway.DefaultConfig()) || meaningfulWeb(targetWeb) || targetUI != nil

	lines := []string{"Existing system data will be replaced:"}
	appendChange := func(label, current, replacement string, changed bool) {
		if changed {
			if current == replacement {
				lines = append(lines, fmt.Sprintf("  %s: changed (%s)", label, current))
				return
			}
			lines = append(lines, fmt.Sprintf("  %s: %s -> %s", label, current, replacement))
		}
	}
	selectedChanged := !reflect.DeepEqual(targetDocument.Selected, sourceDocument.Selected)
	activeChanged := !reflect.DeepEqual(targetDocument.Active, sourceDocument.Active)
	coresChanged := !sameCoreInventory(targetDocument, sourceDocument)
	appendChange("Selected", selectedSummary(targetDocument), selectedSummary(sourceDocument), selectedChanged)
	appendChange("Active", activeSummary(targetDocument), activeSummary(sourceDocument), activeChanged)
	appendChange("Core versions", fmt.Sprint(coreVersionCount(targetDocument)), fmt.Sprint(coreVersionCount(sourceDocument)), coresChanged)
	appendChange("Subscription", yesNo(targetSubscriptions), yesNo(sourceSubscriptions), !sameSubscriptions)
	appendChange("Tunnels", tunnelSummary(targetTunnel), tunnelSummary(sourceTunnel), !sameTunnel)
	appendChange("Gateway", configuredDefault(targetGateway, gateway.DefaultConfig()), configuredDefault(sourceGateway, gateway.DefaultConfig()), !sameGateway)
	appendChange("Web listener", targetWeb.Listen, sourceWeb.Listen, targetWeb.Listen != sourceWeb.Listen)
	appendChange("Web password", setEmpty(targetWeb.Password), setEmpty(sourceWeb.Password), targetWeb.Password != sourceWeb.Password)
	appendChange("UI", uiSummary(targetUI), uiSummary(sourceUI), !sameUI)
	if !sameState && !selectedChanged && !activeChanged && !coresChanged {
		lines = append(lines, "  Deployment state: changed")
	}
	return replacementPreview{
		Meaningful: meaningful,
		Same:       sameState && sameSubscriptions && sameTunnel && sameGateway && sameWeb && sameUI,
		Summary:    strings.Join(lines, "\n"),
	}, nil
}

func readTunnelConfig(path string) (tunnel.Config, error) {
	config := tunnel.DefaultConfig()
	data, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return config, nil
	}
	if err != nil {
		return tunnel.Config{}, err
	}
	if err := json.Unmarshal(data, &config); err != nil {
		return tunnel.Config{}, fmt.Errorf("decode tunnel configuration: %w", err)
	}
	config.Normalize()
	return config, config.Validate()
}

func readGatewayConfig(path string) (gateway.Config, error) {
	config := gateway.DefaultConfig()
	data, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return config, nil
	}
	if err != nil {
		return gateway.Config{}, err
	}
	if err := json.Unmarshal(data, &config); err != nil {
		return gateway.Config{}, fmt.Errorf("decode gateway configuration: %w", err)
	}
	config.Normalize()
	return config, config.Validate()
}

func readWebConfig(path string) (webconfig.Config, error) {
	config := webconfig.Config{Schema: webconfig.SchemaVersion, Listen: webconfig.DefaultListen}
	data, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return config, nil
	}
	if err != nil {
		return webconfig.Config{}, err
	}
	if err := json.Unmarshal(data, &config); err != nil {
		return webconfig.Config{}, fmt.Errorf("decode web configuration: %w", err)
	}
	return config, config.Validate()
}

func readCurrentUI(paths layout.Layout) (*uiassets.Metadata, error) {
	metadata, err := uiassets.New(paths.UI, paths.UICurrent).Current()
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	return &metadata, nil
}

func equalUI(left, right *uiassets.Metadata) bool {
	if left == nil || right == nil {
		return left == nil && right == nil
	}
	return left.Manifest == right.Manifest && left.SourceType == right.SourceType &&
		left.Source == right.Source && left.Digest == right.Digest
}

func meaningfulWeb(config webconfig.Config) bool {
	return config.Listen != webconfig.DefaultListen || config.Password != ""
}

func selectedSummary(document state.Document) string {
	if document.Selected == nil {
		return "none"
	}
	return selectionRef(*document.Selected).String()
}

func activeSummary(document state.Document) string {
	if document.Active == nil {
		return "none"
	}
	return deploymentLabel(*document.Active)
}

func coreVersionCount(document state.Document) int {
	count := 0
	for _, coreState := range document.Cores {
		for _, source := range coreState.SourceEntries() {
			count += len(source.State.Installed)
		}
	}
	return count
}

func sameCoreInventory(left, right state.Document) bool {
	return reflect.DeepEqual(left.Cores, right.Cores)
}

func tunnelSummary(config tunnel.Config) string {
	forwards := 0
	for _, instance := range config.Instances {
		forwards += len(instance.Forwards)
	}
	return fmt.Sprintf("%d instances, %d forwards", len(config.Instances), forwards)
}

func configuredDefault[T any](value, defaults T) string {
	if reflect.DeepEqual(value, defaults) {
		return "default"
	}
	return "configured"
}

func setEmpty(value string) string {
	if value == "" {
		return "empty"
	}
	return "set"
}

func yesNo(value bool) string {
	if value {
		return "yes"
	}
	return "no"
}

func uiSummary(metadata *uiassets.Metadata) string {
	if metadata == nil {
		return "not installed"
	}
	return fmt.Sprintf("%s %s (%s)", metadata.Manifest.Name, metadata.Manifest.Version, metadata.SourceType)
}
