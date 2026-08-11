package cli

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"
	"time"

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
