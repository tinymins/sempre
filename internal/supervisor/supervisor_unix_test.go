//go:build !windows

package supervisor

import (
	"context"
	"errors"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
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

func TestRunnerRetriesAfterResolveRollback(t *testing.T) {
	t.Parallel()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	var resolves atomic.Int32
	var rollbacks atomic.Int32
	runner := Runner{
		Stdout: NewRollingWriter(filepath.Join(t.TempDir(), "out.log"), 1024, 1),
		Stderr: NewRollingWriter(filepath.Join(t.TempDir(), "err.log"), 1024, 1),
		Hooks: Hooks{
			Resolve: func(context.Context) (Plan, error) {
				if resolves.Add(1) == 1 {
					return Plan{}, errors.New("candidate is unavailable")
				}
				cancel()
				return Plan{}, context.Canceled
			},
			ResolveFailure: func(err error) (bool, error) {
				if strings.Contains(err.Error(), "candidate is unavailable") {
					rollbacks.Add(1)
					return true, nil
				}
				return false, nil
			},
			Stopped: func() error { return nil },
		},
	}
	if err := runner.Run(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("runner error = %v", err)
	}
	if rollbacks.Load() != 1 || resolves.Load() != 2 {
		t.Fatalf("resolves = %d, rollbacks = %d", resolves.Load(), rollbacks.Load())
	}
}
