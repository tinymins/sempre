package cli

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/app"
	"github.com/tinymins/sempre/internal/control"
	"github.com/tinymins/sempre/internal/controlplane"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/webconfig"
)

func (command *CLI) uninstall(ctx context.Context, arguments []string, options Options) error {
	purge := false
	for _, argument := range arguments {
		if argument != "--purge" {
			return usageError()
		}
		purge = true
	}
	if !options.Yes {
		if purge {
			fmt.Fprint(command.output, "Remove Sempre and all configuration, subscriptions, passwords, and data? [y/N]: ")
		} else {
			fmt.Fprint(command.output, "Uninstall Sempre while retaining configuration and Web settings? [y/N]: ")
		}
		line, err := command.input.ReadString('\n')
		if err != nil && !errors.Is(err, io.EOF) {
			return err
		}
		if value := strings.TrimSpace(line); !strings.EqualFold(value, "y") && !strings.EqualFold(value, "yes") {
			return fmt.Errorf("uninstall cancelled")
		}
	}
	if err := command.manager.UninstallApplication(ctx, purge); err != nil {
		return err
	}
	if purge {
		fmt.Fprintln(command.output, "Sempre and all data were removed.")
	} else {
		fmt.Fprintln(command.output, "Sempre was removed. Configuration, subscription, Web listener, and password were retained.")
	}
	return nil
}

func (command *CLI) web(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	switch arguments[0] {
	case "status":
		if len(arguments) != 1 {
			return usageError()
		}
		status, err := command.manager.WebStatus()
		if err != nil {
			return err
		}
		if options.JSON {
			return writeCLIJSON(command.output, status)
		}
		fmt.Fprintln(command.output, "Listen:", status.Listen)
		fmt.Fprintln(command.output, "Local URL:", status.LocalURL)
		fmt.Fprintln(command.output, "Password set:", status.PasswordSet)
		return nil
	case "listen":
		if len(arguments) != 2 {
			return usageError()
		}
		status, err := command.manager.SetWebListen(arguments[1])
		if err != nil {
			return err
		}
		if err := command.restartForWebChange(ctx, options); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Web listener:", status.Listen)
		return nil
	case "password":
		if len(arguments) == 2 && arguments[1] == "clear" {
			if _, err := command.manager.SetAdministratorPassword(""); err != nil {
				return err
			}
			if err := command.restartForWebChange(ctx, options); err != nil {
				return err
			}
			fmt.Fprintln(command.output, "Administrator password cleared. Empty-password warning is active.")
			return nil
		}
		if len(arguments) != 3 || arguments[1] != "set" || arguments[2] != "--stdin" {
			return usageError()
		}
		password, err := command.input.ReadString('\n')
		if err != nil && !errors.Is(err, io.EOF) {
			return err
		}
		password = strings.TrimSuffix(strings.TrimSuffix(password, "\n"), "\r")
		if password == "" {
			return fmt.Errorf("password from stdin is empty; use 'web password clear' explicitly")
		}
		if _, err := command.manager.SetAdministratorPassword(password); err != nil {
			return err
		}
		if err := command.restartForWebChange(ctx, options); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Administrator password updated.")
		return nil
	default:
		return usageError()
	}
}

func (command *CLI) restartForWebChange(ctx context.Context, options Options) error {
	if command.manager.Paths().Mode != layout.System || options.NoRestart {
		return nil
	}
	current, err := command.manager.ServiceState(ctx)
	if err != nil || current != service.Running {
		return err
	}
	return command.manager.RestartService(ctx)
}

func (command *CLI) ui(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	switch arguments[0] {
	case "status":
		if len(arguments) != 1 {
			return usageError()
		}
		metadata, err := command.manager.UIStatus()
		if err != nil {
			if errors.Is(err, os.ErrNotExist) {
				fmt.Fprintln(command.output, "UI: not installed")
				return nil
			}
			return err
		}
		return writeCLIJSON(command.output, metadata)
	case "install":
		if len(arguments) < 2 || len(arguments) > 4 {
			return usageError()
		}
		digest := ""
		if len(arguments) == 4 {
			if arguments[2] != "--sha256" {
				return usageError()
			}
			digest = arguments[3]
		}
		metadata, err := command.manager.InstallUI(ctx, arguments[1], digest)
		if err != nil {
			return err
		}
		fmt.Fprintf(command.output, "Installed UI %s %s (%s).\n", metadata.Manifest.Name, metadata.Manifest.Version, metadata.Digest)
		return nil
	case "update":
		if len(arguments) != 1 {
			return usageError()
		}
		metadata, err := command.manager.UpdateUI(ctx)
		if err != nil {
			return err
		}
		fmt.Fprintf(command.output, "Updated UI to %s %s.\n", metadata.Manifest.Name, metadata.Manifest.Version)
		return nil
	case "remove":
		if len(arguments) != 1 {
			return usageError()
		}
		if err := command.manager.RemoveUI(); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "UI removed.")
		return nil
	default:
		return usageError()
	}
}

func (command *CLI) runPortable(ctx context.Context) error {
	if command.manager.Paths().Mode != layout.Portable {
		return fmt.Errorf("portable run requires --portable mode")
	}
	go func() {
		deadline := time.NewTimer(20 * time.Second)
		defer deadline.Stop()
		ticker := time.NewTicker(200 * time.Millisecond)
		defer ticker.Stop()
		for {
			endpoint, err := webconfig.ReadEndpoint(command.manager.Paths().Endpoint)
			if err == nil && healthy(ctx, endpoint.LocalURL) {
				if !uiReady(ctx, endpoint.LocalURL) {
					fmt.Fprintln(command.output, "Portable service:", endpoint.LocalURL)
					fmt.Fprintln(command.output, "Portable Web UI: not installed")
					return
				}
				fmt.Fprintln(command.output, "Portable Web UI:", endpoint.LocalURL)
				if err := openBrowser(endpoint.LocalURL, command.output); err != nil {
					fmt.Fprintln(command.errors, "ERROR:", err)
				}
				return
			}
			select {
			case <-ctx.Done():
				return
			case <-deadline.C:
				fmt.Fprintln(command.errors, "ERROR: portable Web UI did not become ready")
				return
			case <-ticker.C:
			}
		}
	}()
	fmt.Fprintln(command.output, "Starting portable Sempre. Press Ctrl+C to stop.")
	return command.manager.RunDaemon(ctx)
}

func (command *CLI) runtime(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	if arguments[0] == "status" || arguments[0] == "start" || arguments[0] == "stop" || arguments[0] == "restart" || arguments[0] == "reload" {
		return command.runtimeLifecycle(ctx, arguments, options)
	}
	client, err := command.manager.RuntimeControl()
	if err != nil {
		return err
	}
	switch arguments[0] {
	case "capabilities":
		return writeCLIJSON(command.output, client.Capabilities(ctx))
	case "overview":
		value, err := client.Overview(ctx)
		return writeRuntimeCLI(command.output, value, err)
	case "config":
		if len(arguments) == 1 {
			value, err := client.Config(ctx)
			return writeRuntimeCLI(command.output, value, err)
		}
		if len(arguments) != 4 || arguments[1] != "set" {
			return usageError()
		}
		var value any
		if err := json.Unmarshal([]byte(arguments[3]), &value); err != nil {
			return fmt.Errorf("configuration value must be JSON: %w", err)
		}
		return client.PatchConfig(ctx, map[string]any{arguments[2]: value})
	case "proxies":
		return command.runtimeProxies(ctx, client, arguments[1:])
	case "providers":
		return command.runtimeProviders(ctx, client, arguments[1:])
	case "rules":
		value, err := client.Rules(ctx)
		return writeRuntimeCLI(command.output, value, err)
	case "rule-providers":
		if len(arguments) == 3 && arguments[1] == "update" {
			return client.UpdateRuleProvider(ctx, arguments[2])
		}
		value, err := client.RuleProviders(ctx)
		return writeRuntimeCLI(command.output, value, err)
	case "connections":
		if len(arguments) == 1 {
			value, err := client.Connections(ctx)
			return writeRuntimeCLI(command.output, value, err)
		}
		if len(arguments) == 3 && arguments[1] == "close" {
			id := arguments[2]
			if id == "--all" {
				id = ""
			}
			return client.CloseConnection(ctx, id)
		}
		return usageError()
	case "dns":
		if len(arguments) < 3 || len(arguments) > 4 || arguments[1] != "query" {
			return usageError()
		}
		recordType := "A"
		if len(arguments) == 4 {
			recordType = arguments[3]
		}
		value, err := client.DNSQuery(ctx, arguments[2], recordType)
		return writeRuntimeCLI(command.output, value, err)
	case "cache":
		if len(arguments) != 2 || arguments[1] != "flush" {
			return usageError()
		}
		return client.FlushFakeIP(ctx)
	case "events", "traffic", "memory", "logs":
		topic := arguments[0]
		if topic == "events" {
			if len(arguments) != 2 {
				return usageError()
			}
			topic = arguments[1]
		}
		return client.Stream(ctx, topic, func(data json.RawMessage) error {
			_, err := fmt.Fprintln(command.output, string(data))
			return err
		})
	default:
		return usageError()
	}
}

func (command *CLI) runtimeLifecycle(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) != 1 {
		return usageError()
	}
	client, err := controlplane.Discover(command.manager.Paths().DaemonControl)
	if err != nil {
		return err
	}
	operation := arguments[0]
	if operation == "status" {
		var status app.RuntimeStatus
		if err := client.Get(ctx, "/api/v1/runtime/status", &status); err != nil {
			return err
		}
		return command.writeManagedRuntimeStatus(status, options.JSON)
	}
	if operation == "reload" {
		var result struct {
			Status app.RuntimeStatus `json:"status"`
		}
		if err := client.Post(ctx, "/api/v1/runtime/reload", nil, &result); err != nil {
			return err
		}
		if options.JSON {
			return writeCLIJSON(command.output, result.Status)
		}
		fmt.Fprintln(command.output, "Managed core reconciliation scheduled.")
		return nil
	}

	var before app.RuntimeStatus
	if err := client.Get(ctx, "/api/v1/runtime/status", &before); err != nil {
		return err
	}
	var accepted struct {
		Status app.RuntimeStatus `json:"status"`
	}
	if err := client.Post(ctx, "/api/v1/runtime/"+operation, nil, &accepted); err != nil {
		return err
	}
	status, err := waitManagedRuntime(ctx, client, operation, before, accepted.Status)
	if err != nil {
		return err
	}
	return command.writeManagedRuntimeStatus(status, options.JSON)
}

func waitManagedRuntime(
	ctx context.Context,
	client *controlplane.Client,
	operation string,
	before app.RuntimeStatus,
	current app.RuntimeStatus,
) (app.RuntimeStatus, error) {
	deadline := time.NewTimer(60 * time.Second)
	defer deadline.Stop()
	ticker := time.NewTicker(200 * time.Millisecond)
	defer ticker.Stop()
	for {
		if managedRuntimeComplete(operation, before, current) {
			return current, nil
		}
		if current.RuntimeState == "failed" {
			message := current.LastError
			if message == "" {
				message = "managed core entered failed state"
			}
			return current, fmt.Errorf("%s", message)
		}
		select {
		case <-ctx.Done():
			return current, ctx.Err()
		case <-deadline.C:
			return current, fmt.Errorf("timed out waiting for managed core to %s (current state: %s)", operation, current.RuntimeState)
		case <-ticker.C:
			if err := client.Get(ctx, "/api/v1/runtime/status", &current); err != nil {
				return current, err
			}
		}
	}
}

func managedRuntimeComplete(operation string, before, current app.RuntimeStatus) bool {
	switch operation {
	case "stop":
		return current.DesiredState == "stopped" && (current.RuntimeState == "stopped" || current.RuntimeState == "idle")
	case "restart":
		return current.RuntimeState == "running" && (before.PID == 0 || current.PID != before.PID)
	default:
		return current.RuntimeState == "running"
	}
}

func (command *CLI) writeManagedRuntimeStatus(status app.RuntimeStatus, jsonOutput bool) error {
	if jsonOutput {
		return writeCLIJSON(command.output, status)
	}
	coreReference := "none"
	configHash := "none"
	deployment := status.Active
	if deployment == nil {
		deployment = status.Target
	}
	if deployment != nil {
		coreReference = deployment.ExactReference
		configHash = deployment.ConfigHash
		if len(configHash) > 12 {
			configHash = configHash[:12]
		}
	}
	fmt.Fprintln(command.output, "Desired:", status.DesiredState)
	fmt.Fprintln(command.output, "State:", status.RuntimeState)
	fmt.Fprintln(command.output, "Core:", coreReference)
	fmt.Fprintln(command.output, "Config:", configHash)
	fmt.Fprintln(command.output, "PID:", valueOrNone(status.PID))
	fmt.Fprintln(command.output, "Uptime:", (time.Duration(status.UptimeSeconds) * time.Second).String())
	fmt.Fprintln(command.output, "Restarts:", status.RestartCount)
	fmt.Fprintln(command.output, "Last transition:", timeOrNone(status.LastTransition))
	fmt.Fprintln(command.output, "Last exit:", stringOrNone(status.LastExit))
	fmt.Fprintln(command.output, "Last error:", stringOrNone(status.LastError))
	return nil
}

func valueOrNone(value int) string {
	if value == 0 {
		return "none"
	}
	return strconv.Itoa(value)
}

func timeOrNone(value *time.Time) string {
	if value == nil {
		return "none"
	}
	return value.Format(time.RFC3339)
}

func stringOrNone(value string) string {
	if value == "" {
		return "none"
	}
	return value
}

func (command *CLI) runtimeProxies(ctx context.Context, client *control.Client, arguments []string) error {
	if len(arguments) == 0 {
		value, err := client.Proxies(ctx)
		return writeRuntimeCLI(command.output, value, err)
	}
	switch arguments[0] {
	case "select":
		if len(arguments) != 3 {
			return usageError()
		}
		return client.SelectProxy(ctx, arguments[1], arguments[2])
	case "delay":
		if len(arguments) < 2 || len(arguments) > 4 {
			return usageError()
		}
		testURL := "https://www.gstatic.com/generate_204"
		if len(arguments) >= 3 {
			testURL = arguments[2]
		}
		timeout := 5000
		if len(arguments) == 4 {
			parsed, err := strconv.Atoi(arguments[3])
			if err != nil || parsed <= 0 {
				return fmt.Errorf("timeout must be a positive number of milliseconds")
			}
			timeout = parsed
		}
		delay, err := client.ProxyDelay(ctx, arguments[1], testURL, timeout)
		return writeRuntimeCLI(command.output, map[string]int{"delay": delay}, err)
	default:
		return usageError()
	}
}

func (command *CLI) runtimeProviders(ctx context.Context, client *control.Client, arguments []string) error {
	if len(arguments) == 0 {
		value, err := client.Providers(ctx)
		return writeRuntimeCLI(command.output, value, err)
	}
	if len(arguments) != 2 {
		return usageError()
	}
	switch arguments[0] {
	case "update":
		return client.UpdateProvider(ctx, arguments[1])
	case "healthcheck":
		return client.HealthcheckProvider(ctx, arguments[1])
	default:
		return usageError()
	}
}

func writeRuntimeCLI(output io.Writer, value any, err error) error {
	if err != nil {
		return err
	}
	return writeCLIJSON(output, value)
}

func writeCLIJSON(output io.Writer, value any) error {
	encoder := json.NewEncoder(output)
	encoder.SetIndent("", "  ")
	return encoder.Encode(value)
}

func parseMilliseconds(value string, fallback int) int {
	parsed, err := strconv.Atoi(strings.TrimSpace(value))
	if err != nil || parsed <= 0 {
		return fallback
	}
	return parsed
}
