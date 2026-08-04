package cli

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"slices"
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

func TestLauncherMenuAndSelection(t *testing.T) {
	t.Parallel()
	for _, action := range []string{"Install", "Repair", "Upgrade"} {
		for _, test := range []struct {
			name         string
			showOpen     bool
			showPortable bool
			want         string
			wantActions  []launcherAction
		}{
			{
				name:        "installed and running",
				showOpen:    true,
				want:        "\n1. Open Web UI\n2. " + action + "\n3. Uninstall\n0. Exit\n",
				wantActions: []launcherAction{launcherOpen, launcherInstall, launcherUninstall},
			},
			{
				name:         "not installed",
				showPortable: true,
				want:         "\n1. " + action + "\n2. Uninstall\n3. Run Portable\n0. Exit\n",
				wantActions:  []launcherAction{launcherInstall, launcherUninstall, launcherPortable},
			},
		} {
			t.Run(action+"/"+test.name, func(t *testing.T) {
				t.Parallel()
				var output bytes.Buffer
				actions := writeLauncherMenu(&output, action, test.showOpen, test.showPortable)
				if output.String() != test.want {
					t.Fatalf("menu = %q, want %q", output.String(), test.want)
				}
				if !slices.Equal(actions, test.wantActions) {
					t.Fatalf("actions = %v, want %v", actions, test.wantActions)
				}
				for index, want := range test.wantActions {
					got, ok := launcherSelection(fmt.Sprint(index+1), actions)
					if !ok || got != want {
						t.Fatalf("selection %d = %q, %t; want %q, true", index+1, got, ok, want)
					}
				}
			})
		}
	}
	for _, choice := range []string{"", "0", "4", "invalid"} {
		if action, ok := launcherSelection(choice, []launcherAction{launcherInstall}); ok {
			t.Fatalf("selection %q = %q, true; want invalid", choice, action)
		}
	}
}

func TestLauncherPortableAvailable(t *testing.T) {
	t.Parallel()
	failure := errors.New("unavailable")
	for _, test := range []struct {
		name            string
		state           service.State
		serviceErr      error
		endpointHealthy bool
		want            bool
	}{
		{"not installed", service.NotInstalled, nil, false, true},
		{"stopped", service.Stopped, nil, false, true},
		{"running", service.Running, nil, false, false},
		{"start pending", service.StartPending, nil, false, false},
		{"stop pending", service.StopPending, nil, false, false},
		{"unknown", service.Unknown, nil, false, true},
		{"status unavailable", service.Unknown, failure, false, true},
		{"healthy endpoint", service.Unknown, failure, true, false},
	} {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := launcherPortableAvailable(test.state, test.serviceErr, test.endpointHealthy); got != test.want {
				t.Fatalf("available = %t, want %t", got, test.want)
			}
		})
	}
}

func TestLauncherUninstallSelectionOpensConfirmationMenu(t *testing.T) {
	status := launcherStatus(context.Background())
	var menuOutput bytes.Buffer
	actions := writeLauncherMenu(&menuOutput, status.installAction, status.showOpen, status.showPortable)
	uninstallChoice := 0
	for index, action := range actions {
		if action == launcherUninstall {
			uninstallChoice = index + 1
			break
		}
	}
	if uninstallChoice == 0 {
		t.Fatal("uninstall action missing from launcher menu")
	}
	var output bytes.Buffer
	var errorOutput bytes.Buffer
	input := fmt.Sprintf("%d\n0\n0\n", uninstallChoice)
	code := runLauncher(context.Background(), strings.NewReader(input), &output, &errorOutput)
	if code != 0 {
		t.Fatalf("exit code = %d, errors = %s", code, errorOutput.String())
	}
	if !strings.Contains(output.String(), "\n1. Uninstall and keep configuration\n2. Full uninstall and remove all data\n0. Cancel\n") {
		t.Fatalf("uninstall menu missing from output: %s", output.String())
	}
}
