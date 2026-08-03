package supervisor

import (
	"context"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/tinymins/sempre/internal/core"
)

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
