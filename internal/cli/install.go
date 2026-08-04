package cli

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/app"
	"github.com/tinymins/sempre/internal/controlplane"
)

const maxSubscriptionArgumentSize = int64(64 << 10)

func (command *CLI) install(ctx context.Context, arguments []string, _ Options) error {
	options, err := parseInstallOptions(arguments)
	if err != nil {
		return err
	}
	result, err := command.manager.BootstrapApplication(ctx, options)
	if err != nil {
		return err
	}
	endpoint, installedUI, err := waitForSystemReady(ctx)
	if err != nil {
		return command.cleanupFreshInstall(ctx, result, err)
	}
	if result.RuntimeTarget != nil {
		if err := command.startBootstrapRuntime(ctx, result.RuntimeTarget); err != nil {
			return command.cleanupFreshInstall(ctx, result, err)
		}
	}
	fmt.Fprintln(command.output, "Sempre installed, enabled, and started.")
	if result.CoreReference != "" {
		fmt.Fprintln(command.output, "Core:", result.CoreReference)
	}
	if result.SubscriptionID != "" {
		fmt.Fprintln(command.output, "Default subscription set:", result.SubscriptionID)
	}
	if installedUI {
		fmt.Fprintln(command.output, "Web UI:", endpoint.LocalURL)
		return openBrowser(endpoint.LocalURL)
	}
	fmt.Fprintln(command.output, "Service:", endpoint.LocalURL)
	fmt.Fprintln(command.output, "Web UI: not installed")
	return nil
}

func (command *CLI) cleanupFreshInstall(ctx context.Context, result app.BootstrapResult, cause error) error {
	if !result.InstalledFresh() {
		return cause
	}
	if err := command.manager.UninstallService(ctx); err != nil {
		return errors.Join(cause, fmt.Errorf("remove incomplete system service: %w", err))
	}
	return fmt.Errorf("%w; incomplete system service registration was removed", cause)
}

func (command *CLI) startBootstrapRuntime(ctx context.Context, expected *app.RuntimeDeployment) error {
	client, err := controlplane.Discover(command.manager.Paths().DaemonControl)
	if err != nil {
		return err
	}
	var current app.RuntimeStatus
	if err := client.Get(ctx, "/api/v1/runtime/status", &current); err != nil {
		return err
	}
	var accepted struct {
		Status app.RuntimeStatus `json:"status"`
	}
	if err := client.Post(ctx, "/api/v1/runtime/start", nil, &accepted); err != nil {
		return err
	}
	current = accepted.Status
	deadline := time.NewTimer(60 * time.Second)
	defer deadline.Stop()
	ticker := time.NewTicker(200 * time.Millisecond)
	defer ticker.Stop()
	for {
		if current.RuntimeState == "running" {
			if sameRuntimeDeployment(current.Active, expected) {
				return nil
			}
			return fmt.Errorf("requested core did not become active; Sempre rolled back to %s", runtimeReference(current.Active))
		}
		if current.RuntimeState == "failed" {
			message := current.LastError
			if message == "" {
				message = "managed core entered failed state"
			}
			return fmt.Errorf("%s", message)
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-deadline.C:
			return fmt.Errorf("timed out waiting for requested core to start (current state: %s)", current.RuntimeState)
		case <-ticker.C:
			if err := client.Get(ctx, "/api/v1/runtime/status", &current); err != nil {
				return err
			}
		}
	}
}

func sameRuntimeDeployment(left, right *app.RuntimeDeployment) bool {
	return left != nil && right != nil &&
		left.Core == right.Core && left.Repository == right.Repository && left.Ref == right.Ref &&
		left.Version == right.Version && left.ConfigHash == right.ConfigHash
}

func runtimeReference(deployment *app.RuntimeDeployment) string {
	if deployment == nil {
		return "no active core"
	}
	return deployment.ExactReference
}

func parseInstallOptions(arguments []string) (app.BootstrapOptions, error) {
	var options app.BootstrapOptions
	seen := map[string]bool{}
	subscriptionFile := ""
	for index := 0; index < len(arguments); index++ {
		argument := arguments[index]
		name, value, hasValue := strings.Cut(argument, "=")
		switch name {
		case "--core", "--subscription", "--subscription-file", "--ui", "--ui-sha256":
		default:
			return app.BootstrapOptions{}, fmt.Errorf("unknown install option %q", argument)
		}
		if seen[name] {
			return app.BootstrapOptions{}, fmt.Errorf("install option %s was provided more than once", name)
		}
		seen[name] = true
		if !hasValue {
			index++
			if index >= len(arguments) {
				return app.BootstrapOptions{}, fmt.Errorf("install option %s requires a value", name)
			}
			value = arguments[index]
			if strings.HasPrefix(value, "--") {
				return app.BootstrapOptions{}, fmt.Errorf("install option %s requires a value", name)
			}
		}
		if strings.TrimSpace(value) == "" {
			return app.BootstrapOptions{}, fmt.Errorf("install option %s cannot be empty", name)
		}
		switch name {
		case "--core":
			options.Core = value
		case "--subscription":
			options.Subscription = value
		case "--subscription-file":
			subscriptionFile = value
		case "--ui":
			options.UI = value
		case "--ui-sha256":
			options.UISHA256 = value
		}
	}
	if options.Subscription != "" && subscriptionFile != "" {
		return app.BootstrapOptions{}, fmt.Errorf("--subscription and --subscription-file cannot be used together")
	}
	if subscriptionFile != "" {
		value, err := readSubscriptionArgument(subscriptionFile)
		if err != nil {
			return app.BootstrapOptions{}, err
		}
		options.Subscription = value
	}
	return options, nil
}

func readSubscriptionArgument(path string) (string, error) {
	file, err := os.Open(path)
	if err != nil {
		return "", fmt.Errorf("open subscription argument file: %w", err)
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		return "", err
	}
	if !info.Mode().IsRegular() || info.Size() <= 0 || info.Size() > maxSubscriptionArgumentSize {
		return "", fmt.Errorf("subscription argument file must be a non-empty regular file no larger than %d bytes", maxSubscriptionArgumentSize)
	}
	data, err := io.ReadAll(io.LimitReader(file, maxSubscriptionArgumentSize+1))
	if err != nil {
		return "", err
	}
	if int64(len(data)) > maxSubscriptionArgumentSize {
		return "", fmt.Errorf("subscription argument file exceeds %d bytes", maxSubscriptionArgumentSize)
	}
	value := strings.TrimSpace(strings.TrimPrefix(string(data), "\ufeff"))
	if value == "" {
		return "", fmt.Errorf("subscription argument file is empty")
	}
	return value, nil
}
