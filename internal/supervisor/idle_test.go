package supervisor

import (
	"context"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/core"
)

func TestRunnerWaitsIdleUntilReload(t *testing.T) {
	t.Parallel()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	reload := make(chan struct{}, 1)
	idle := make(chan struct{}, 1)
	var resolves atomic.Int32
	runner := Runner{Hooks: Hooks{
		Resolve: func(context.Context) (Plan, error) {
			if resolves.Add(1) == 1 {
				return Plan{}, ErrIdle
			}
			cancel()
			return Plan{}, context.Canceled
		},
		NextUpdate: func() (time.Duration, bool) { return 0, false },
		Idle: func() error {
			select {
			case idle <- struct{}{}:
			default:
			}
			return nil
		},
		Stopped: func() error { return nil },
		Log:     func(string, ...any) {},
		Reload:  reload,
	}}
	done := make(chan error, 1)
	go func() { done <- runner.Run(ctx) }()
	select {
	case <-idle:
	case <-ctx.Done():
		t.Fatal("runner did not enter idle state")
	}
	reload <- struct{}{}
	if err := <-done; err != nil {
		t.Fatal(err)
	}
	if resolves.Load() != 2 {
		t.Fatalf("resolve calls = %d", resolves.Load())
	}
}

func TestRunnerDoesNotStartAfterIntentChanges(t *testing.T) {
	t.Parallel()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	var resolves atomic.Int32
	var starts atomic.Int32
	var releases atomic.Int32
	runner := Runner{Hooks: Hooks{
		Resolve: func(context.Context) (Plan, error) {
			if resolves.Add(1) == 1 {
				return Plan{}, nil
			}
			cancel()
			return Plan{}, ErrStopped
		},
		AcquireStart: func(Plan) (func(), bool, error) {
			return func() { releases.Add(1) }, false, nil
		},
		Starting: func(Plan) error {
			starts.Add(1)
			return nil
		},
		NextUpdate: func() (time.Duration, bool) { return 0, false },
		Stopped:    func() error { return nil },
		Log:        func(string, ...any) {},
	}}
	if err := runner.Run(ctx); err != nil {
		t.Fatal(err)
	}
	if starts.Load() != 0 {
		t.Fatalf("starting callbacks = %d", starts.Load())
	}
	if releases.Load() != 1 {
		t.Fatalf("start guard releases = %d", releases.Load())
	}
}

func TestRunnerCountsExecutableStartFailure(t *testing.T) {
	t.Parallel()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	var exits atomic.Int32
	var restartCount atomic.Int32
	runner := Runner{Hooks: Hooks{
		Resolve: func(context.Context) (Plan, error) {
			return Plan{Spec: core.RunSpec{Path: filepath.Join(t.TempDir(), "missing-core")}}, nil
		},
		EarlyFailure: func(Plan, error) error { return nil },
		Exited: func(_ Plan, _ error, restarts int) error {
			exits.Add(1)
			restartCount.Store(int32(restarts))
			cancel()
			return nil
		},
		Stopped: func() error { return nil },
		Log:     func(string, ...any) {},
	}}
	if err := runner.Run(ctx); err != nil {
		t.Fatal(err)
	}
	if exits.Load() != 1 || restartCount.Load() != 1 {
		t.Fatalf("exits = %d, restart count = %d", exits.Load(), restartCount.Load())
	}
}
