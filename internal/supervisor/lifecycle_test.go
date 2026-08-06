package supervisor

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/core"
)

func TestRunnerRollsBackStartedHookFailure(t *testing.T) {
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	t.Setenv("SEMPRE_SUPERVISOR_HELPER", "1")
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	marker := filepath.Join(t.TempDir(), "stop")
	failure := errors.New("activate data plane")
	var earlyFailures atomic.Int32
	var exits atomic.Int32
	runner := Runner{
		Stdout: NewRollingWriter(filepath.Join(t.TempDir(), "out.log"), 1024, 1),
		Stderr: NewRollingWriter(filepath.Join(t.TempDir(), "err.log"), 1024, 1),
		Hooks: Hooks{
			Resolve: func(context.Context) (Plan, error) {
				return Plan{Spec: core.RunSpec{
					Path: executable,
					Args: []string{"-test.run=^TestSupervisorHelperProcess$", "--", marker},
				}}, nil
			},
			Starting: func(Plan) error { return nil },
			Started:  func(Plan, int) error { return failure },
			EarlyFailure: func(_ Plan, got error) error {
				if !errors.Is(got, failure) {
					t.Errorf("startup failure = %v", got)
				}
				earlyFailures.Add(1)
				return nil
			},
			Exited: func(_ Plan, got error, restarts int) error {
				if !errors.Is(got, failure) || restarts != 1 {
					t.Errorf("exit failure = %v, restarts = %d", got, restarts)
				}
				exits.Add(1)
				cancel()
				return nil
			},
			Stopped: func() error { return nil },
			Log:     func(string, ...any) {},
		},
	}
	if err := runner.Run(ctx); err != nil {
		t.Fatal(err)
	}
	if earlyFailures.Load() != 1 || exits.Load() != 1 {
		t.Fatalf("early failures = %d, exits = %d", earlyFailures.Load(), exits.Load())
	}
}

func TestRunnerRollsBackHealthyHookFailure(t *testing.T) {
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	t.Setenv("SEMPRE_SUPERVISOR_HELPER", "1")
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	marker := filepath.Join(t.TempDir(), "stop")
	failure := errors.New("verify data plane")
	var earlyFailures atomic.Int32
	var exits atomic.Int32
	runner := Runner{
		Stdout:       NewRollingWriter(filepath.Join(t.TempDir(), "out.log"), 1024, 1),
		Stderr:       NewRollingWriter(filepath.Join(t.TempDir(), "err.log"), 1024, 1),
		StartupGrace: 10 * time.Millisecond,
		Hooks: Hooks{
			Resolve: func(context.Context) (Plan, error) {
				return Plan{Spec: core.RunSpec{
					Path: executable,
					Args: []string{"-test.run=^TestSupervisorHelperProcess$", "--", marker},
				}}, nil
			},
			Starting: func(Plan) error { return nil },
			Started:  func(Plan, int) error { return nil },
			Healthy:  func(Plan) error { return failure },
			NextUpdate: func() (time.Duration, bool) {
				return 0, false
			},
			EarlyFailure: func(_ Plan, got error) error {
				if !errors.Is(got, failure) {
					t.Errorf("health failure = %v", got)
				}
				earlyFailures.Add(1)
				return nil
			},
			Exited: func(_ Plan, got error, restarts int) error {
				if !errors.Is(got, failure) || restarts != 1 {
					t.Errorf("exit failure = %v, restarts = %d", got, restarts)
				}
				exits.Add(1)
				cancel()
				return nil
			},
			Stopped: func() error { return nil },
			Log:     func(string, ...any) {},
		},
	}
	if err := runner.Run(ctx); err != nil {
		t.Fatal(err)
	}
	if earlyFailures.Load() != 1 || exits.Load() != 1 {
		t.Fatalf("early failures = %d, exits = %d", earlyFailures.Load(), exits.Load())
	}
}

func TestRunnerStartRestartStopLifecycle(t *testing.T) {
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	t.Setenv("SEMPRE_SUPERVISOR_HELPER", "1")
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	reload := make(chan struct{}, 1)
	var desiredStopped atomic.Bool
	var starts atomic.Int32
	var stopping atomic.Int32
	var restarting atomic.Int32
	var stopped atomic.Int32
	var pidMu sync.Mutex
	var pids []int
	marker := filepath.Join(t.TempDir(), "stop")
	plan := Plan{Spec: core.RunSpec{
		Path:       executable,
		Args:       []string{"-test.run=^TestSupervisorHelperProcess$", "--", marker},
		WorkingDir: t.TempDir(),
	}}
	runner := Runner{
		Stdout: NewRollingWriter(filepath.Join(t.TempDir(), "out.log"), 1024, 1),
		Stderr: NewRollingWriter(filepath.Join(t.TempDir(), "err.log"), 1024, 1),
		Hooks: Hooks{
			Resolve: func(context.Context) (Plan, error) {
				if desiredStopped.Load() {
					return Plan{}, ErrStopped
				}
				return plan, nil
			},
			NextUpdate: func() (time.Duration, bool) { return 0, false },
			Starting: func(Plan) error {
				if err := os.Remove(marker); err != nil && !os.IsNotExist(err) {
					return err
				}
				starts.Add(1)
				return nil
			},
			Started: func(_ Plan, pid int) error {
				pidMu.Lock()
				pids = append(pids, pid)
				count := len(pids)
				pidMu.Unlock()
				if count == 2 {
					desiredStopped.Store(true)
				}
				reload <- struct{}{}
				return nil
			},
			Healthy: func(Plan) error { return nil },
			Stopping: func(Plan) error {
				stopping.Add(1)
				return os.WriteFile(marker, []byte("stop"), 0o600)
			},
			Restarting: func(Plan) error {
				restarting.Add(1)
				return nil
			},
			EarlyFailure: func(_ Plan, failure error) error {
				t.Errorf("unexpected early failure: %v", failure)
				return nil
			},
			Exited: func(_ Plan, failure error, _ int) error {
				t.Errorf("unexpected exit: %v", failure)
				return nil
			},
			Stopped: func() error {
				stopped.Add(1)
				if desiredStopped.Load() {
					cancel()
				}
				return nil
			},
			Log:    func(string, ...any) {},
			Reload: reload,
		},
	}
	if err := runner.Run(ctx); err != nil {
		t.Fatal(err)
	}
	pidMu.Lock()
	defer pidMu.Unlock()
	if starts.Load() != 2 || len(pids) != 2 || pids[0] == pids[1] {
		t.Fatalf("starts = %d, pids = %v", starts.Load(), pids)
	}
	if stopping.Load() != 2 || restarting.Load() != 1 || stopped.Load() == 0 {
		t.Fatalf("stopping = %d, restarting = %d, stopped = %d", stopping.Load(), restarting.Load(), stopped.Load())
	}
}

func TestRunForegroundPassesRunEnvironment(t *testing.T) {
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	spec := core.RunSpec{
		Path: executable,
		Args: []string{"-test.run=^TestSupervisorEnvironmentHelper$"},
		Env:  []string{"SEMPRE_SUPERVISOR_ENV_HELPER=expected"},
	}
	if err := RunForeground(context.Background(), spec, os.Stdout, os.Stderr); err != nil {
		t.Fatal(err)
	}
}

func TestSupervisorEnvironmentHelper(t *testing.T) {
	value, exists := os.LookupEnv("SEMPRE_SUPERVISOR_ENV_HELPER")
	if !exists {
		return
	}
	if value != "expected" {
		t.Fatalf("environment value = %q", value)
	}
}

func TestSupervisorHelperProcess(t *testing.T) {
	if os.Getenv("SEMPRE_SUPERVISOR_HELPER") != "1" {
		return
	}
	marker := os.Args[len(os.Args)-1]
	for {
		if _, err := os.Stat(marker); err == nil {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
}
