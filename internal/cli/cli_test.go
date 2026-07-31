package cli

import (
	"path/filepath"
	"testing"

	"github.com/sempre-lab/sempre/internal/layout"
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
		{[]string{"service", "status"}, layout.System, false},
		{[]string{"service", "install"}, layout.Portable, true},
		{[]string{"run"}, layout.Portable, true},
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

type testWriter struct {
	t *testing.T
}

func (writer testWriter) Write(data []byte) (int, error) {
	writer.t.Log(string(data))
	return len(data), nil
}
