package app

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"time"

	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func verifyConfigurationHash(path, expected string) error {
	actual, err := configurationFileHash(path)
	if err != nil {
		return err
	}
	return equalValue(actual, expected, "configuration content hash does not match recorded state")
}

func equalValue(actual, expected, message string) error {
	if actual != expected {
		return fmt.Errorf("%s: got %q, expected %q", message, actual, expected)
	}
	return nil
}

func probeExternalManagementAPI(ctx context.Context, config subscriptions.ManagementAPIConfig) error {
	host, port, err := net.SplitHostPort(config.ExternalController)
	if err != nil {
		return err
	}
	address := net.ParseIP(host)
	if host == "" || address != nil && address.IsUnspecified() {
		host = "127.0.0.1"
	}
	probeCtx, cancel := context.WithTimeout(ctx, 3*time.Second)
	defer cancel()
	request, err := http.NewRequestWithContext(probeCtx, http.MethodGet, "http://"+net.JoinHostPort(host, port)+"/version", nil)
	if err != nil {
		return err
	}
	request.Header.Set("Authorization", "Bearer "+config.Secret)
	response, err := (&http.Client{Transport: &http.Transport{Proxy: nil}}).Do(request)
	if err != nil {
		return err
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return fmt.Errorf("external controller returned HTTP %d", response.StatusCode)
	}
	return nil
}

type networkProbeResult struct {
	name string
	err  error
}

func transparentNetworkProbes(ctx context.Context) []networkProbeResult {
	probes := []struct {
		name string
		url  string
	}{
		{name: "domestic reachability through direct rules", url: "https://www.baidu.com/"},
		{name: "foreign reachability through proxy rules", url: "https://www.google.com/generate_204"},
	}
	results := make(chan networkProbeResult, len(probes)+1)
	for _, probe := range probes {
		go func() {
			probeCtx, cancel := context.WithTimeout(ctx, 8*time.Second)
			defer cancel()
			request, err := http.NewRequestWithContext(probeCtx, http.MethodGet, probe.url, nil)
			if err == nil {
				response, requestErr := (&http.Client{Transport: &http.Transport{Proxy: nil}}).Do(request)
				err = requestErr
				if response != nil {
					_ = response.Body.Close()
					if err == nil && (response.StatusCode < 200 || response.StatusCode >= 400) {
						err = fmt.Errorf("HTTP %d", response.StatusCode)
					}
				}
			}
			results <- networkProbeResult{name: probe.name, err: err}
		}()
	}
	go func() {
		probeCtx, cancel := context.WithTimeout(ctx, 8*time.Second)
		defer cancel()
		addresses, err := net.DefaultResolver.LookupIPAddr(probeCtx, "www.google.com")
		if err == nil {
			for _, value := range addresses {
				if value.IP.IsPrivate() || value.IP.IsLoopback() || value.IP.IsUnspecified() || value.IP.IsMulticast() {
					err = fmt.Errorf("resolver returned non-public address %s", value.IP)
					break
				}
			}
		}
		results <- networkProbeResult{name: "foreign DNS response sanity", err: err}
	}()
	output := make([]networkProbeResult, 0, cap(results))
	for range cap(results) {
		output = append(output, <-results)
	}
	return output
}

func (manager *Manager) runtimeStatus(document state.Document) (string, error) {
	locked, err := manager.store.InstanceRunning()
	if err != nil {
		return "", err
	}
	runtimeState := document.Runtime
	if runtimeState.PID > 0 {
		if !processAlive(runtimeState.PID) {
			return fmt.Sprintf(
				"stale record: PID %d is not running (recorded state %s)",
				runtimeState.PID,
				runtimeState.State,
			), nil
		}
		if !locked {
			return fmt.Sprintf(
				"stale record: PID %d exists but the Sempre instance lock is free",
				runtimeState.PID,
			), nil
		}
		return fmt.Sprintf(
			"%s, PID %d, restarts %d",
			runtimeState.State,
			runtimeState.PID,
			runtimeState.RestartCount,
		), nil
	}
	if locked {
		switch runtimeState.State {
		case "idle", "stopped", "failed":
			return fmt.Sprintf("%s, no running process", runtimeState.State), nil
		default:
			return "starting or stopping; instance lock held before PID was recorded", nil
		}
	}
	switch runtimeState.State {
	case "running", "starting", "restarting":
		return fmt.Sprintf("stale record: state is %s but no managed process or instance lock exists", runtimeState.State), nil
	case "":
		return "", nil
	default:
		return fmt.Sprintf("%s, no running process", runtimeState.State), nil
	}
}

func writableDirectory(path string) error {
	file, err := os.CreateTemp(path, ".write-check-*")
	if err != nil {
		return err
	}
	name := file.Name()
	if err := file.Close(); err != nil {
		return err
	}
	return os.Remove(name)
}

func FollowLogs(ctx context.Context, output io.Writer, paths []string, follow bool) error {
	cursors := map[string]logCursor{}
	for {
		for _, path := range paths {
			cursor, err := printLogDelta(output, filepath.Base(path), path, cursors[path], !follow)
			if err != nil {
				return err
			}
			cursors[path] = cursor
		}
		if !follow {
			return nil
		}
		select {
		case <-ctx.Done():
			return nil
		case <-time.After(250 * time.Millisecond):
		}
	}
}

type logCursor struct {
	offset  int64
	info    os.FileInfo
	partial []byte
}

func printLogDelta(output io.Writer, label, path string, cursor logCursor, flushPartial bool) (logCursor, error) {
	file, err := os.Open(path)
	if errors.Is(err, os.ErrNotExist) {
		return cursor, nil
	}
	if err != nil {
		return cursor, fmt.Errorf("open log %s: %w", path, err)
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		return cursor, fmt.Errorf("inspect log %s: %w", path, err)
	}
	if cursor.info != nil && (!os.SameFile(cursor.info, info) || info.Size() < cursor.offset) {
		cursor = logCursor{}
	}
	trimInitialLine := false
	if cursor.info == nil && cursor.offset == 0 && info.Size() > 64*1024 {
		cursor.offset = info.Size() - 64*1024
		trimInitialLine = true
	}
	if _, err := file.Seek(cursor.offset, io.SeekStart); err != nil {
		return cursor, fmt.Errorf("seek log %s: %w", path, err)
	}
	data, err := io.ReadAll(file)
	if err != nil {
		return cursor, fmt.Errorf("read log %s: %w", path, err)
	}
	cursor.offset += int64(len(data))
	cursor.info = info
	data = append(cursor.partial, data...)
	cursor.partial = nil
	if trimInitialLine {
		if newline := bytes.IndexByte(data, '\n'); newline >= 0 {
			data = data[newline+1:]
		} else if !flushPartial {
			cursor.partial = data
			return cursor, nil
		}
	}
	for {
		newline := bytes.IndexByte(data, '\n')
		if newline < 0 {
			break
		}
		line := bytes.TrimSuffix(data[:newline], []byte{'\r'})
		if _, err := fmt.Fprintf(output, "[%s] %s\n", label, line); err != nil {
			return cursor, err
		}
		data = data[newline+1:]
	}
	if len(data) > 0 {
		if flushPartial {
			if _, err := fmt.Fprintf(output, "[%s] %s\n", label, bytes.TrimSuffix(data, []byte{'\r'})); err != nil {
				return cursor, err
			}
		} else {
			cursor.partial = append(cursor.partial, data...)
		}
	}
	return cursor, nil
}
