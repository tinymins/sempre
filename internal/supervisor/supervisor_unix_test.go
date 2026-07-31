//go:build !windows

package supervisor

import (
	"context"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/sempre-lab/sempre/internal/core"
	"github.com/sempre-lab/sempre/internal/state"
)

func TestRunnerReportsEarlyFailureAndStops(t *testing.T) {
	t.Parallel()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	var earlyFailures atomic.Int32
	var stopped atomic.Int32
	plan := Plan{
		Deployment: state.Deployment{Core: "test", Ref: "stable", Version: "1.0.0", ConfigHash: "hash"},
		Spec: core.RunSpec{
			Path:       "/bin/sh",
			Args:       []string{"-c", "exit 7"},
			WorkingDir: t.TempDir(),
		},
	}
	runner := Runner{
		Stdout: NewRollingWriter(filepath.Join(t.TempDir(), "out.log"), 1024, 1),
		Stderr: NewRollingWriter(filepath.Join(t.TempDir(), "err.log"), 1024, 1),
		Hooks: Hooks{
			Resolve: func(context.Context) (Plan, error) { return plan, nil },
			ScheduledUpdate: func(context.Context) (bool, error) {
				return false, nil
			},
			NextUpdate: func() (time.Duration, bool) { return 0, false },
			Started:    func(Plan, int) error { return nil },
			Healthy:    func(Plan) error { return nil },
			EarlyFailure: func(_ Plan, err error) error {
				if err == nil || !strings.Contains(err.Error(), "exit status 7") {
					t.Errorf("early failure = %v", err)
				}
				earlyFailures.Add(1)
				cancel()
				return nil
			},
			Exited: func(Plan, error, int) error { return nil },
			Stopped: func() error {
				stopped.Add(1)
				return nil
			},
			Log: func(string, ...any) {},
		},
	}
	if err := runner.Run(ctx); err != nil {
		t.Fatal(err)
	}
	if earlyFailures.Load() != 1 {
		t.Fatalf("early failures = %d", earlyFailures.Load())
	}
	if stopped.Load() != 1 {
		t.Fatalf("stopped callbacks = %d", stopped.Load())
	}
}
