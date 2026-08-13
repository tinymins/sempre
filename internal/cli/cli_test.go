package cli

import (
	"bufio"
	"bytes"
	"context"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/app"
	"github.com/tinymins/sempre/internal/layout"
)

func TestParseGlobalOptions(t *testing.T) {
	t.Parallel()
	arguments, options, err := parseGlobalOptions([]string{"subscription", "update", "--yes", "--elevated"})
	if err != nil {
		t.Fatal(err)
	}
	if !options.Yes || options.NoRestart || !options.Elevated {
		t.Fatalf("options = %#v", options)
	}
	if len(arguments) != 2 || arguments[0] != "subscription" || arguments[1] != "update" {
		t.Fatalf("arguments = %#v", arguments)
	}
}

func TestParseGlobalOptionsRejectsConflictingRestartFlags(t *testing.T) {
	t.Parallel()
	if _, _, err := parseGlobalOptions([]string{"update", "--yes", "--no-restart"}); err == nil {
		t.Fatal("conflicting restart flags were accepted")
	}
}

func TestInstallCommandsRejectUnusedGlobalOptions(t *testing.T) {
	t.Parallel()
	for _, arguments := range [][]string{{"install"}, {"bundle", "restore"}, {"service", "install"}} {
		if err := validateCommandOptions(arguments, Options{NoRestart: true}); err == nil {
			t.Errorf("%v accepted --no-restart", arguments)
		}
		if err := validateCommandOptions(arguments, Options{JSON: true}); err == nil {
			t.Errorf("%v accepted --json", arguments)
		}
	}
	if err := validateCommandOptions([]string{"core", "install", "sing-box@stable"}, Options{NoRestart: true}); err != nil {
		t.Fatal(err)
	}
}

func TestParseGlobalOptionsSelectsMode(t *testing.T) {
	t.Parallel()
	arguments, options, err := parseGlobalOptions([]string{"core", "--portable", "list"})
	if err != nil {
		t.Fatal(err)
	}
	if options.Mode != layout.Portable || len(arguments) != 2 {
		t.Fatalf("arguments = %#v, options = %#v", arguments, options)
	}
	if _, _, err := parseGlobalOptions([]string{"--system", "--portable", "status"}); err == nil {
		t.Fatal("conflicting modes were accepted")
	}
}

func TestResolveModeUsesMarkerUnlessExplicit(t *testing.T) {
	t.Parallel()
	executable := filepath.Join(t.TempDir(), "sempre")
	mode, err := resolveMode("", executable)
	if err != nil || mode != layout.System {
		t.Fatalf("default mode = %q, %v", mode, err)
	}
	if err := layout.SetPortableMarker(executable, true); err != nil {
		t.Fatal(err)
	}
	mode, err = resolveMode("", executable)
	if err != nil || mode != layout.Portable {
		t.Fatalf("marker mode = %q, %v", mode, err)
	}
	mode, err = resolveMode(layout.System, executable)
	if err != nil || mode != layout.System {
		t.Fatalf("explicit mode = %q, %v", mode, err)
	}
}

func TestAdministratorRequirement(t *testing.T) {
	t.Parallel()
	for _, test := range []struct {
		arguments []string
		mode      layout.Mode
		want      bool
	}{
		{[]string{"status"}, layout.System, true},
		{[]string{"status"}, layout.Portable, false},
		{[]string{"install"}, layout.System, true},
		{[]string{"install"}, layout.Portable, true},
		{[]string{"service", "status"}, layout.System, false},
		{[]string{"service", "install"}, layout.Portable, true},
		{[]string{"bundle", "install"}, layout.Portable, true},
		{[]string{"bundle", "export", "/tmp/out"}, layout.System, true},
		{[]string{"bundle", "export", "/tmp/out"}, layout.Portable, false},
		{[]string{"run"}, layout.Portable, true},
		{[]string{"portable", "run"}, layout.Portable, true},
		{[]string{"portable", "enable"}, layout.System, false},
		{[]string{"version"}, layout.System, false},
	} {
		if got := requiresAdministrator(test.arguments, test.mode); got != test.want {
			t.Errorf("requiresAdministrator(%v, %s) = %v", test.arguments, test.mode, got)
		}
	}
}

func TestInvocationArgumentsPreserveModeAndRestartFlags(t *testing.T) {
	t.Parallel()
	got := invocationArguments(
		[]string{"subscription", "update"},
		Options{Mode: layout.Portable, Yes: true, Elevated: true},
	)
	want := []string{"--portable", "subscription", "update", "--yes", "--elevated"}
	if len(got) != len(want) {
		t.Fatalf("arguments = %#v", got)
	}
	for index := range want {
		if got[index] != want[index] {
			t.Fatalf("arguments = %#v", got)
		}
	}
}

func TestRunStatelessManagesPortableMarker(t *testing.T) {
	t.Parallel()
	executable := filepath.Join(t.TempDir(), "sempre")
	handled, code := runStateless(
		t.Context(),
		[]string{"portable", "enable"},
		executable,
		testWriter{t},
		testWriter{t},
	)
	if !handled || code != 0 {
		t.Fatalf("handled = %v, code = %d", handled, code)
	}
	enabled, err := layout.PortableMarkerEnabled(executable)
	if err != nil || !enabled {
		t.Fatalf("marker = %v, %v", enabled, err)
	}
}

func TestRunStatelessDefersPortableRun(t *testing.T) {
	t.Parallel()
	handled, code := runStateless(
		t.Context(),
		[]string{"portable", "run"},
		filepath.Join(t.TempDir(), "sempre"),
		testWriter{t},
		testWriter{t},
	)
	if handled || code != 0 {
		t.Fatalf("handled = %v, code = %d", handled, code)
	}
}

func TestUIReadyRequiresSuccessfulRootResponse(t *testing.T) {
	t.Parallel()
	for _, test := range []struct {
		name   string
		status int
		want   bool
	}{
		{name: "installed", status: http.StatusOK, want: true},
		{name: "missing", status: http.StatusServiceUnavailable, want: false},
	} {
		test := test
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
				if request.URL.Path != "/" {
					t.Fatalf("path = %q", request.URL.Path)
				}
				writer.WriteHeader(test.status)
			}))
			defer server.Close()
			if got := uiReady(context.Background(), server.URL); got != test.want {
				t.Fatalf("uiReady() = %v, want %v", got, test.want)
			}
		})
	}
}

func TestConfirmBundledUIReplacement(t *testing.T) {
	for _, test := range []struct {
		input string
		want  bool
	}{
		{input: "y\n", want: true},
		{input: "YES\n", want: true},
		{input: "n\n"},
		{input: "\n"},
	} {
		var output bytes.Buffer
		command := &CLI{input: bufio.NewReader(strings.NewReader(test.input)), output: &output}
		got := command.confirmBundledUIReplacement(app.BundledUIReplacement{Name: "Sempre Console", Version: "1.2.3", SourceType: "local"})
		if got != test.want {
			t.Errorf("confirmation %q = %t, want %t", test.input, got, test.want)
		}
		for _, expected := range []string{"Sempre Console", "1.2.3", "local", "Replace the installed UI? [y/N]:"} {
			if !strings.Contains(output.String(), expected) {
				t.Errorf("confirmation output %q does not contain %q", output.String(), expected)
			}
		}
	}
}

func TestManagedRuntimeCompletionSemantics(t *testing.T) {
	t.Parallel()
	before := app.RuntimeStatus{PID: 41, RuntimeState: "running", DesiredState: "running"}
	for _, test := range []struct {
		operation string
		status    app.RuntimeStatus
		want      bool
	}{
		{operation: "start", status: app.RuntimeStatus{RuntimeState: "starting", DesiredState: "running"}},
		{operation: "start", status: app.RuntimeStatus{RuntimeState: "running", DesiredState: "running"}, want: true},
		{operation: "stop", status: app.RuntimeStatus{RuntimeState: "stopping", DesiredState: "stopped"}},
		{operation: "stop", status: app.RuntimeStatus{RuntimeState: "stopped", DesiredState: "stopped"}, want: true},
		{operation: "stop", status: app.RuntimeStatus{RuntimeState: "idle", DesiredState: "stopped"}, want: true},
		{operation: "restart", status: app.RuntimeStatus{RuntimeState: "running", DesiredState: "running", PID: 41}},
		{operation: "restart", status: app.RuntimeStatus{RuntimeState: "running", DesiredState: "running", PID: 42}, want: true},
	} {
		if got := managedRuntimeComplete(test.operation, before, test.status); got != test.want {
			t.Errorf("managedRuntimeComplete(%q, %#v) = %v", test.operation, test.status, got)
		}
	}
}

func TestManagedRuntimeStatusOutput(t *testing.T) {
	t.Parallel()
	var output bytes.Buffer
	command := &CLI{output: &output}
	status := app.RuntimeStatus{
		DesiredState:  "running",
		RuntimeState:  "running",
		PID:           1234,
		UptimeSeconds: 90,
		RestartCount:  2,
		Active: &app.RuntimeDeployment{
			ExactReference: "sing-box@1.2.3",
			ConfigHash:     strings.Repeat("a", 64),
		},
	}
	if err := command.writeManagedRuntimeStatus(status, false); err != nil {
		t.Fatal(err)
	}
	for _, expected := range []string{
		"Desired: running", "State: running", "Core: sing-box@1.2.3",
		"Config: aaaaaaaaaaaa", "PID: 1234", "Uptime: 1m30s", "Restarts: 2",
	} {
		if !strings.Contains(output.String(), expected) {
			t.Fatalf("status output does not contain %q:\n%s", expected, output.String())
		}
	}
}

func TestManagedRuntimeStatusOutputUsesRetryTarget(t *testing.T) {
	t.Parallel()
	var output bytes.Buffer
	command := &CLI{output: &output}
	status := app.RuntimeStatus{
		DesiredState: "running",
		RuntimeState: "failed",
		Target: &app.RuntimeDeployment{
			ExactReference: "sing-box:tinymins/sing-box@1.13.15-ddns.1",
			ConfigHash:     strings.Repeat("b", 64),
		},
	}
	if err := command.writeManagedRuntimeStatus(status, false); err != nil {
		t.Fatal(err)
	}
	for _, expected := range []string{
		"State: failed",
		"Core: sing-box:tinymins/sing-box@1.13.15-ddns.1",
		"Config: bbbbbbbbbbbb",
	} {
		if !strings.Contains(output.String(), expected) {
			t.Fatalf("status output does not contain %q:\n%s", expected, output.String())
		}
	}
}

type testWriter struct {
	t *testing.T
}

func (writer testWriter) Write(data []byte) (int, error) {
	writer.t.Log(string(data))
	return len(data), nil
}
