package cli

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
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
		status, address := launcherStatus(ctx)
		fmt.Fprintln(output, "Sempre")
		fmt.Fprintf(output, "version: %s\n", buildinfo.Version)
		if address != "" {
			fmt.Fprintf(output, "status: %s, listening on %s\n", status, address)
		} else {
			fmt.Fprintf(output, "status: %s\n", status)
		}
		fmt.Fprintln(output, "\n1. Open Web UI")
		fmt.Fprintln(output, "2. Install / Repair")
		fmt.Fprintln(output, "3. Uninstall")
		fmt.Fprintln(output, "4. Run Portable")
		fmt.Fprintln(output, "0. Exit")
		fmt.Fprint(output, "\nSelect [0-4]: ")
		line, err := reader.ReadString('\n')
		if err != nil && !errors.Is(err, io.EOF) {
			fmt.Fprintln(errorOutput, "ERROR:", err)
			return 1
		}
		switch strings.TrimSpace(line) {
		case "", "0":
			return 0
		case "1":
			return Run(ctx, []string{"open"}, reader, output, errorOutput)
		case "2":
			return Run(ctx, []string{"install"}, reader, output, errorOutput)
		case "3":
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
		case "4":
			return Run(ctx, []string{"--portable", "portable", "run"}, reader, output, errorOutput)
		default:
			fmt.Fprintln(errorOutput, "Invalid selection.")
		}
	}
}

func launcherStatus(ctx context.Context) (string, string) {
	paths, err := layout.ForMode(layout.System)
	if err != nil {
		return "unavailable", ""
	}
	endpoint, endpointErr := webconfig.ReadEndpoint(paths.Endpoint)
	serviceState, serviceErr := service.New().Status(ctx)
	if endpointErr == nil && healthy(ctx, endpoint.LocalURL) {
		return "running", endpoint.LocalURL
	}
	if serviceErr != nil {
		return "unavailable", ""
	}
	switch serviceState {
	case service.NotInstalled:
		return "not installed", ""
	case service.Running, service.StartPending:
		if endpointErr == nil {
			return "starting", endpoint.LocalURL
		}
		return "starting", ""
	default:
		return string(serviceState), valueOrEmpty(endpoint.LocalURL, endpointErr)
	}
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
