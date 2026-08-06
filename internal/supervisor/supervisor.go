package supervisor

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
)

const (
	startupGrace = 10 * time.Second
	stopGrace    = 10 * time.Second
	maxBackoff   = 60 * time.Second
)

var (
	ErrIdle    = errors.New("no active core deployment")
	ErrStopped = errors.New("managed core is stopped")
)

type Plan struct {
	Deployment state.Deployment
	Spec       core.RunSpec
	Control    core.ControlSpec
}

type Hooks struct {
	Resolve         func(context.Context) (Plan, error)
	ResolveFailure  func(error) (bool, error)
	ScheduledUpdate func(context.Context) (bool, error)
	NextUpdate      func() (time.Duration, bool)
	AcquireStart    func(Plan) (release func(), allowed bool, err error)
	Starting        func(Plan) error
	Started         func(Plan, int) error
	Healthy         func(Plan) error
	Stopping        func(Plan) error
	Restarting      func(Plan) error
	EarlyFailure    func(Plan, error) error
	Exited          func(Plan, error, int) error
	Stopped         func() error
	Idle            func() error
	Log             func(string, ...any)
	Reload          <-chan struct{}
}

type Runner struct {
	Stdout       *RollingWriter
	Stderr       *RollingWriter
	Hooks        Hooks
	StartupGrace time.Duration
}

func RunForeground(ctx context.Context, spec core.RunSpec, stdout, stderr interface{ Write([]byte) (int, error) }) error {
	command := exec.Command(spec.Path, spec.Args...)
	command.Dir = spec.WorkingDir
	if len(spec.Env) > 0 {
		command.Env = append(os.Environ(), spec.Env...)
	}
	command.Stdout = stdout
	command.Stderr = stderr
	configureCommand(command)
	if err := command.Start(); err != nil {
		return err
	}
	process, err := attachProcess(command)
	if err != nil {
		_ = command.Process.Kill()
		_ = command.Wait()
		return err
	}
	waited := make(chan error, 1)
	go func() { waited <- command.Wait() }()
	select {
	case <-ctx.Done():
		stopProcess(process, waited)
		closeProcess(process)
		return nil
	case err := <-waited:
		closeProcess(process)
		return err
	}
}

func (runner *Runner) Run(ctx context.Context) error {
	backoff := time.Second
	restarts := 0
	restarting := false
	for {
		if err := ctx.Err(); err != nil {
			if runner.Hooks.Stopped != nil {
				_ = runner.Hooks.Stopped()
			}
			return nil
		}
		plan, err := runner.Hooks.Resolve(ctx)
		if err != nil {
			if errors.Is(err, ErrStopped) {
				if runner.Hooks.Stopped != nil {
					_ = runner.Hooks.Stopped()
				}
				if runner.waitInactive(ctx) {
					restarting = false
					continue
				}
				return nil
			}
			if errors.Is(err, ErrIdle) {
				if runner.Hooks.Idle != nil {
					_ = runner.Hooks.Idle()
				}
				if runner.waitInactive(ctx) {
					restarting = false
					continue
				}
				_ = runner.Hooks.Stopped()
				return nil
			}
			if runner.Hooks.ResolveFailure != nil {
				retry, rollbackErr := runner.Hooks.ResolveFailure(err)
				if rollbackErr != nil {
					runner.Hooks.Log("resolve failure handling failed: %v", errors.Join(err, rollbackErr))
				}
				if retry {
					backoff = time.Second
					restarting = true
					continue
				}
			}
			if err := runner.waitRetry(ctx, backoff); err != nil {
				_ = runner.Hooks.Stopped()
				return nil
			}
			backoff = nextBackoff(backoff)
			restarting = true
			continue
		}
		if restarting && runner.Hooks.Restarting != nil {
			if err := runner.Hooks.Restarting(plan); err != nil {
				return err
			}
		}
		releaseStart := func() {}
		if runner.Hooks.AcquireStart != nil {
			var allowed bool
			releaseStart, allowed, err = runner.Hooks.AcquireStart(plan)
			if err != nil {
				if releaseStart != nil {
					releaseStart()
				}
				return err
			}
			if releaseStart == nil {
				releaseStart = func() {}
			}
			if !allowed {
				releaseStart()
				restarting = false
				continue
			}
		}
		if runner.Hooks.Starting != nil {
			if err := runner.Hooks.Starting(plan); err != nil {
				releaseStart()
				return err
			}
		}
		command := exec.Command(plan.Spec.Path, plan.Spec.Args...)
		command.Dir = plan.Spec.WorkingDir
		if len(plan.Spec.Env) > 0 {
			command.Env = append(os.Environ(), plan.Spec.Env...)
		}
		command.Stdout = runner.Stdout
		command.Stderr = runner.Stderr
		configureCommand(command)
		if err := command.Start(); err != nil {
			_ = runner.Hooks.EarlyFailure(plan, err)
			restarts++
			_ = runner.Hooks.Exited(plan, err, restarts)
			releaseStart()
			if err := waitBackoff(ctx, runner.Hooks.Reload, backoff); err != nil {
				_ = runner.Hooks.Stopped()
				return nil
			}
			backoff = nextBackoff(backoff)
			restarting = true
			continue
		}
		process, err := attachProcess(command)
		if err != nil {
			releaseStart()
			_ = command.Process.Kill()
			_ = command.Wait()
			return err
		}
		if err := runner.Hooks.Started(plan, command.Process.Pid); err != nil {
			releaseStart()
			_ = forceStop(process)
			_ = command.Wait()
			closeProcess(process)
			if runner.Hooks.EarlyFailure != nil {
				_ = runner.Hooks.EarlyFailure(plan, err)
			}
			restarts++
			if runner.Hooks.Exited != nil {
				_ = runner.Hooks.Exited(plan, err, restarts)
			}
			if err := waitBackoff(ctx, runner.Hooks.Reload, backoff); err != nil {
				if runner.Hooks.Stopped != nil {
					_ = runner.Hooks.Stopped()
				}
				return nil
			}
			backoff = nextBackoff(backoff)
			restarting = true
			continue
		}
		releaseStart()
		waited := make(chan error, 1)
		go func() { waited <- command.Wait() }()
		graceDuration := runner.StartupGrace
		if graceDuration <= 0 {
			graceDuration = startupGrace
		}
		grace := time.NewTimer(graceDuration)
		updateTimer, updateChannel := runner.updateTimer()
		healthy := false
		intentionalRestart := false
		restarting = false

	processLoop:
		for {
			select {
			case <-ctx.Done():
				if updateTimer != nil {
					updateTimer.Stop()
				}
				grace.Stop()
				if runner.Hooks.Stopping != nil {
					_ = runner.Hooks.Stopping(plan)
				}
				stopProcess(process, waited)
				closeProcess(process)
				_ = runner.Hooks.Stopped()
				return nil
			case <-runner.Hooks.Reload:
				if updateTimer != nil {
					updateTimer.Stop()
				}
				intentionalRestart = true
				grace.Stop()
				if runner.Hooks.Stopping != nil {
					_ = runner.Hooks.Stopping(plan)
				}
				stopProcess(process, waited)
				closeProcess(process)
				break processLoop
			case <-grace.C:
				if err := runner.Hooks.Healthy(plan); err != nil {
					runner.Hooks.Log("commit healthy deployment: %v", err)
					if updateTimer != nil {
						updateTimer.Stop()
					}
					if runner.Hooks.EarlyFailure != nil {
						_ = runner.Hooks.EarlyFailure(plan, err)
					}
					stopProcess(process, waited)
					closeProcess(process)
					restarts++
					if runner.Hooks.Exited != nil {
						_ = runner.Hooks.Exited(plan, err, restarts)
					}
					if err := waitBackoff(ctx, runner.Hooks.Reload, backoff); err != nil {
						if runner.Hooks.Stopped != nil {
							_ = runner.Hooks.Stopped()
						}
						return nil
					}
					backoff = nextBackoff(backoff)
					restarting = true
					break processLoop
				}
				healthy = true
				backoff = time.Second
			case <-updateChannel:
				changed, updateErr := runner.Hooks.ScheduledUpdate(ctx)
				if updateErr != nil {
					runner.Hooks.Log("scheduled subscription update failed: %v", updateErr)
					updateTimer, updateChannel = runner.updateTimer()
					continue
				}
				if !changed {
					updateTimer, updateChannel = runner.updateTimer()
					continue
				}
				intentionalRestart = true
				grace.Stop()
				if runner.Hooks.Stopping != nil {
					_ = runner.Hooks.Stopping(plan)
				}
				stopProcess(process, waited)
				closeProcess(process)
				break processLoop
			case waitErr := <-waited:
				if updateTimer != nil {
					updateTimer.Stop()
				}
				grace.Stop()
				closeProcess(process)
				if !healthy {
					if err := runner.Hooks.EarlyFailure(plan, waitErr); err != nil {
						runner.Hooks.Log("roll back failed deployment: %v", err)
					}
				}
				restarts++
				_ = runner.Hooks.Exited(plan, waitErr, restarts)
				if err := waitBackoff(ctx, runner.Hooks.Reload, backoff); err != nil {
					_ = runner.Hooks.Stopped()
					return nil
				}
				backoff = nextBackoff(backoff)
				restarting = true
				break processLoop
			}
		}
		if intentionalRestart {
			backoff = time.Second
			restarting = true
		}
	}
}

func (runner *Runner) waitInactive(ctx context.Context) bool {
	timer, updates := runner.updateTimer()
	if timer != nil {
		defer timer.Stop()
	}
	select {
	case <-ctx.Done():
		return false
	case <-runner.Hooks.Reload:
		return true
	case <-updates:
		if runner.Hooks.ScheduledUpdate != nil {
			if _, err := runner.Hooks.ScheduledUpdate(ctx); err != nil {
				runner.Hooks.Log("scheduled subscription update failed while idle: %v", err)
			}
		}
		return true
	}
}

func (runner *Runner) waitRetry(ctx context.Context, delay time.Duration) error {
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-runner.Hooks.Reload:
		return nil
	case <-timer.C:
		return nil
	}
}

func (runner *Runner) updateTimer() (*time.Timer, <-chan time.Time) {
	delay, enabled := runner.Hooks.NextUpdate()
	if !enabled {
		return nil, nil
	}
	if delay < time.Second {
		delay = time.Second
	}
	timer := time.NewTimer(delay)
	return timer, timer.C
}

func stopProcess(process *processHandle, waited <-chan error) {
	if err := gracefulStop(process); err != nil {
		_ = forceStop(process)
		<-waited
		return
	}
	timer := time.NewTimer(stopGrace)
	defer timer.Stop()
	select {
	case <-waited:
		return
	case <-timer.C:
		_ = forceStop(process)
		<-waited
	}
}

func waitBackoff(ctx context.Context, reload <-chan struct{}, delay time.Duration) error {
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-reload:
		return nil
	case <-timer.C:
		return nil
	}
}

func nextBackoff(current time.Duration) time.Duration {
	next := current * 2
	if next > maxBackoff {
		return maxBackoff
	}
	return next
}

func exitText(err error) string {
	if err == nil {
		return "exited successfully"
	}
	return fmt.Sprintf("%v", err)
}
