package cli

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"strings"
	"time"

	"github.com/tinymins/sempre/internal/buildinfo"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
	"github.com/tinymins/sempre/internal/webconfig"
)

func runLauncher(ctx context.Context, input io.Reader, output, errorOutput io.Writer) int {
	reader := bufio.NewReader(input)
	for {
		status := launcherStatus(ctx)
		fmt.Fprintln(output, "Sempre")
		fmt.Fprintf(output, "version: %s\n", buildinfo.Version)
		if status.address != "" {
			fmt.Fprintf(output, "status: %s, listening on %s\n", status.service, status.address)
		} else {
			fmt.Fprintf(output, "status: %s\n", status.service)
		}
		writeLauncherMenu(output, status.installAction)
		fmt.Fprint(output, "\nSelect [0-4]: ")
		line, err := reader.ReadString('\n')
		if err != nil && !errors.Is(err, io.EOF) {
			fmt.Fprintln(errorOutput, "ERROR:", err)
			return 1
		}
		choice := strings.TrimSpace(line)
		switch choice {
		case "", "0":
			return 0
		case "1", "3", "4":
			return Run(ctx, launcherArguments(choice), reader, output, errorOutput)
		case "2":
			fmt.Fprintln(output, "\n1. Uninstall and keep configuration")
			fmt.Fprintln(output, "2. Full uninstall and remove all data")
			fmt.Fprintln(output, "0. Cancel")
			fmt.Fprint(output, "\nSelect [0-2]: ")
			confirmation, _ := reader.ReadString('\n')
			switch strings.TrimSpace(confirmation) {
			case "1":
				return Run(ctx, []string{"uninstall", "--yes"}, reader, output, errorOutput)
			case "2":
				return Run(ctx, []string{"uninstall", "--purge", "--yes"}, reader, output, errorOutput)
			default:
				continue
			}
		default:
			fmt.Fprintln(errorOutput, "Invalid selection.")
		}
	}
}

type launcherSnapshot struct {
	service       string
	address       string
	installAction string
}

func launcherStatus(ctx context.Context) launcherSnapshot {
	paths, err := layout.ForMode(layout.System)
	if err != nil {
		return launcherSnapshot{service: "unavailable", installAction: "Install"}
	}
	endpoint, endpointErr := webconfig.ReadEndpoint(paths.Endpoint)
	serviceState, serviceErr := service.New().Status(ctx)
	result := launcherSnapshot{
		installAction: launcherInstallAction(ctx, paths.ServiceExecutable, serviceState, serviceErr),
	}
	if endpointErr == nil && healthy(ctx, endpoint.LocalURL) {
		result.service = "running"
		result.address = endpoint.LocalURL
		return result
	}
	if serviceErr != nil {
		result.service = "unavailable"
		return result
	}
	switch serviceState {
	case service.NotInstalled:
		result.service = "not installed"
	case service.Running, service.StartPending:
		result.service = "starting"
		if endpointErr == nil {
			result.address = endpoint.LocalURL
		}
	default:
		result.service = string(serviceState)
		result.address = valueOrEmpty(endpoint.LocalURL, endpointErr)
	}
	return result
}

func writeLauncherMenu(output io.Writer, installAction string) {
	fmt.Fprintf(output, "\n1. %s\n", installAction)
	fmt.Fprintln(output, "2. Uninstall")
	fmt.Fprintln(output, "3. Open Web UI")
	fmt.Fprintln(output, "4. Run Portable")
	fmt.Fprintln(output, "0. Exit")
}

func launcherArguments(choice string) []string {
	switch choice {
	case "1":
		return []string{"install"}
	case "3":
		return []string{"open"}
	case "4":
		return []string{"--portable", "portable", "run"}
	default:
		return nil
	}
}

func launcherInstallAction(ctx context.Context, executable string, state service.State, serviceErr error) string {
	if serviceErr == nil && state == service.NotInstalled {
		return "Install"
	}
	installedVersion, versionErr := installedSempreVersion(ctx, executable)
	return classifyLauncherInstallAction(buildinfo.Version, state, serviceErr, installedVersion, versionErr)
}

func classifyLauncherInstallAction(currentVersion string, state service.State, serviceErr error, installedVersion string, versionErr error) string {
	if serviceErr == nil && state == service.NotInstalled {
		return "Install"
	}
	if versionErr != nil {
		if serviceErr != nil && errors.Is(versionErr, os.ErrNotExist) {
			return "Install"
		}
		return "Repair"
	}
	if installedVersion == currentVersion {
		return "Repair"
	}
	return "Upgrade"
}

func installedSempreVersion(ctx context.Context, executable string) (string, error) {
	versionCtx, cancel := context.WithTimeout(ctx, 2*time.Second)
	defer cancel()
	output, err := exec.CommandContext(versionCtx, executable, "version").CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("read installed Sempre version: %w", err)
	}
	return parseSempreVersion(output)
}

func parseSempreVersion(output []byte) (string, error) {
	fields := strings.Fields(string(output))
	if len(fields) < 2 || fields[0] != "Sempre" || fields[1] == "" {
		return "", fmt.Errorf("invalid Sempre version output %q", strings.TrimSpace(string(output)))
	}
	return fields[1], nil
}

func openSystemUI(ctx context.Context) error {
	paths, err := layout.ForMode(layout.System)
	if err != nil {
		return err
	}
	endpoint, err := webconfig.ReadEndpoint(paths.Endpoint)
	if err != nil {
		return fmt.Errorf("Sempre Web is unavailable; run 'sempre install' or start the service")
	}
	if !healthy(ctx, endpoint.LocalURL) {
		return fmt.Errorf("Sempre Web is not responding at %s", endpoint.LocalURL)
	}
	return openBrowser(endpoint.LocalURL)
}

func waitAndOpenSystem(ctx context.Context, output io.Writer) error {
	paths, err := layout.ForMode(layout.System)
	if err != nil {
		return err
	}
	deadline := time.NewTimer(20 * time.Second)
	defer deadline.Stop()
	ticker := time.NewTicker(200 * time.Millisecond)
	defer ticker.Stop()
	for {
		endpoint, endpointErr := webconfig.ReadEndpoint(paths.Endpoint)
		if endpointErr == nil && healthy(ctx, endpoint.LocalURL) {
			if !uiReady(ctx, endpoint.LocalURL) {
				fmt.Fprintln(output, "Service:", endpoint.LocalURL)
				fmt.Fprintln(output, "Web UI: not installed")
				return nil
			}
			fmt.Fprintln(output, "Web UI:", endpoint.LocalURL)
			return openBrowser(endpoint.LocalURL)
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-deadline.C:
			return fmt.Errorf("Sempre service started but Web UI did not become ready")
		case <-ticker.C:
		}
	}
}

func uiReady(ctx context.Context, baseURL string) bool {
	requestCtx, cancel := context.WithTimeout(ctx, time.Second)
	defer cancel()
	request, err := http.NewRequestWithContext(requestCtx, http.MethodGet, strings.TrimRight(baseURL, "/")+"/", nil)
	if err != nil {
		return false
	}
	response, err := http.DefaultClient.Do(request)
	if err != nil {
		return false
	}
	defer response.Body.Close()
	return response.StatusCode == http.StatusOK
}

func healthy(ctx context.Context, baseURL string) bool {
	requestCtx, cancel := context.WithTimeout(ctx, time.Second)
	defer cancel()
	request, err := http.NewRequestWithContext(requestCtx, http.MethodGet, strings.TrimRight(baseURL, "/")+"/api/v1/health", nil)
	if err != nil {
		return false
	}
	response, err := http.DefaultClient.Do(request)
	if err != nil {
		return false
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return false
	}
	var value struct {
		Status string `json:"status"`
	}
	return json.NewDecoder(response.Body).Decode(&value) == nil && value.Status == "ok"
}

func valueOrEmpty(value string, err error) string {
	if err != nil {
		return ""
	}
	return value
}
