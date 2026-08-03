package supervisor

import (
	"context"
	"sync/atomic"
	"testing"
	"time"
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
