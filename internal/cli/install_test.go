package cli

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tinymins/sempre/internal/app"
)

func TestParseInstallOptionsSupportsEqualsAndSeparateValues(t *testing.T) {
	t.Parallel()
	options, err := parseInstallOptions([]string{
		"--core=sing-box:tinymins/sing-box@13.11.2",
		"--subscription", "https://example.com/subscription?token=secret",
		"--ui=tinymins/sempre-ui@stable",
	})
	if err != nil {
		t.Fatal(err)
	}
	want := app.BootstrapOptions{
		Core:         "sing-box:tinymins/sing-box@13.11.2",
		Subscription: "https://example.com/subscription?token=secret",
		UI:           "tinymins/sempre-ui@stable",
	}
	if options != want {
		t.Fatalf("options = %#v, want %#v", options, want)
	}
}

func TestParseInstallOptionsReadsSubscriptionFile(t *testing.T) {
	t.Parallel()
	path := filepath.Join(t.TempDir(), "subscription")
	if err := os.WriteFile(path, []byte("\ufeffhttps://example.com/subscription?token=secret\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	options, err := parseInstallOptions([]string{"--subscription-file", path})
	if err != nil {
		t.Fatal(err)
	}
	if options.Subscription != "https://example.com/subscription?token=secret" {
		t.Fatalf("subscription = %q", options.Subscription)
	}
}

func TestParseInstallOptionsRejectsAmbiguousOrInvalidArguments(t *testing.T) {
	t.Parallel()
	for _, arguments := range [][]string{
		{"--unknown=value"},
		{"--core"},
		{"--core", "--subscription=https://example.com/subscription"},
		{"--core="},
		{"--core=a", "--core=b"},
		{"--subscription=a", "--subscription-file=b"},
	} {
		if _, err := parseInstallOptions(arguments); err == nil {
			t.Errorf("parseInstallOptions(%q) unexpectedly succeeded", arguments)
		}
	}
}

func TestSameRuntimeDeploymentRequiresExactTarget(t *testing.T) {
	t.Parallel()
	target := &app.RuntimeDeployment{Core: "sing-box", Repository: "tinymins/sing-box", Ref: "stable", Version: "13.11.2", ConfigHash: strings.Repeat("a", 64)}
	copy := *target
	if !sameRuntimeDeployment(target, &copy) {
		t.Fatal("identical runtime deployments did not match")
	}
	copy.Version = "13.11.1"
	if sameRuntimeDeployment(target, &copy) {
		t.Fatal("different runtime deployments matched")
	}
}

func TestBootstrapRuntimeResultWaitsForTransientDeadPID(t *testing.T) {
	t.Parallel()
	target := &app.RuntimeDeployment{Core: "sing-box", Ref: "stable", Version: "1.13.15", ConfigHash: strings.Repeat("a", 64)}
	status := app.RuntimeStatus{
		RuntimeState: "failed",
		PID:          41,
		LastError:    "recorded PID 41 is not running",
	}
	done, err := bootstrapRuntimeResult(status, target)
	if done || err != nil {
		t.Fatalf("transient dead PID result = done %t, error %v", done, err)
	}

	status.PID = 0
	status.LastError = "exit status 1"
	done, err = bootstrapRuntimeResult(status, target)
	if !done || err == nil || err.Error() != status.LastError {
		t.Fatalf("stable failure result = done %t, error %v", done, err)
	}
}

func TestBootstrapRuntimeResultRequiresExpectedRunningDeployment(t *testing.T) {
	t.Parallel()
	target := &app.RuntimeDeployment{Core: "sing-box", Repository: "tinymins/sing-box", Ref: "stable", Version: "1.13.15", ConfigHash: strings.Repeat("a", 64)}
	status := app.RuntimeStatus{RuntimeState: "running", Active: target}
	done, err := bootstrapRuntimeResult(status, target)
	if !done || err != nil {
		t.Fatalf("expected running result = done %t, error %v", done, err)
	}

	other := *target
	other.Version = "1.13.14"
	status.Active = &other
	done, err = bootstrapRuntimeResult(status, target)
	if !done || err == nil {
		t.Fatalf("wrong running deployment result = done %t, error %v", done, err)
	}

	status = app.RuntimeStatus{RuntimeState: "restarting"}
	done, err = bootstrapRuntimeResult(status, target)
	if done || err != nil {
		t.Fatalf("restarting result = done %t, error %v", done, err)
	}
}
