package cli

import (
	"bytes"
	"context"
	"errors"
	"os"
	"reflect"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/service"
)

func TestClassifyLauncherInstallAction(t *testing.T) {
	t.Parallel()
	notFound := &os.PathError{Op: "open", Path: "sempre", Err: os.ErrNotExist}
	failure := errors.New("unavailable")
	for _, test := range []struct {
		name             string
		state            service.State
		serviceErr       error
		installedVersion string
		versionErr       error
		want             string
	}{
		{"not installed", service.NotInstalled, nil, "", nil, "Install"},
		{"same version", service.Stopped, nil, "1.2.3", nil, "Repair"},
		{"different version", service.Running, nil, "1.2.2", nil, "Upgrade"},
		{"missing registered executable", service.Stopped, nil, "", notFound, "Repair"},
		{"unknown service without executable", service.Unknown, failure, "", notFound, "Install"},
		{"unknown service with same version", service.Unknown, failure, "1.2.3", nil, "Repair"},
		{"unknown service with different version", service.Unknown, failure, "1.2.2", nil, "Upgrade"},
		{"unknown service with unreadable executable", service.Unknown, failure, "", failure, "Repair"},
	} {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := classifyLauncherInstallAction("1.2.3", test.state, test.serviceErr, test.installedVersion, test.versionErr); got != test.want {
				t.Fatalf("action = %q, want %q", got, test.want)
			}
		})
	}
}

func TestParseSempreVersion(t *testing.T) {
	t.Parallel()
	version, err := parseSempreVersion([]byte("Sempre 1.2.3 (abcdef, 2026-08-04)\n"))
	if err != nil || version != "1.2.3" {
		t.Fatalf("version = %q, err = %v", version, err)
	}
	for _, output := range []string{"", "sing-box version 1.2.3"} {
		if _, err := parseSempreVersion([]byte(output)); err == nil {
			t.Fatalf("accepted invalid output %q", output)
		}
	}
}

func TestLauncherMenuAndArguments(t *testing.T) {
	t.Parallel()
	for _, action := range []string{"Install", "Repair", "Upgrade"} {
		var output bytes.Buffer
		writeLauncherMenu(&output, action)
		want := "\n1. " + action + "\n2. Uninstall\n3. Open Web UI\n4. Run Portable\n0. Exit\n"
		if output.String() != want {
			t.Fatalf("menu = %q, want %q", output.String(), want)
		}
	}
	for choice, want := range map[string][]string{
		"1": {"install"},
		"3": {"open"},
		"4": {"--portable", "portable", "run"},
	} {
		if got := launcherArguments(choice); !reflect.DeepEqual(got, want) {
			t.Fatalf("choice %s arguments = %#v, want %#v", choice, got, want)
		}
	}
}

func TestLauncherChoiceTwoOpensUninstallMenu(t *testing.T) {
	var output bytes.Buffer
	var errorOutput bytes.Buffer
	code := runLauncher(context.Background(), strings.NewReader("2\n0\n0\n"), &output, &errorOutput)
	if code != 0 {
		t.Fatalf("exit code = %d, errors = %s", code, errorOutput.String())
	}
	if !strings.Contains(output.String(), "\n1. Uninstall and keep configuration\n2. Full uninstall and remove all data\n0. Cancel\n") {
		t.Fatalf("uninstall menu missing from output: %s", output.String())
	}
}
