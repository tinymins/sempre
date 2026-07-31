package service

import (
	"context"
	"time"
)

const (
	Name        = "sempre"
	DisplayName = "Sempre"
	Description = "Cross-platform lifecycle manager for proxy cores"
)

type State string

const (
	NotInstalled State = "not installed"
	Stopped      State = "stopped"
	StartPending State = "start pending"
	Running      State = "running"
	StopPending  State = "stop pending"
	Unknown      State = "unknown"
)

type Controller interface {
	Install(context.Context, string) error
	Uninstall(context.Context) error
	Start(context.Context) error
	Stop(context.Context) error
	Restart(context.Context) error
	Status(context.Context) (State, error)
	Run(context.Context, func(context.Context) error) error
}

func waitFor(ctx context.Context, status func(context.Context) (State, error), expected State, timeout time.Duration) error {
	timer := time.NewTimer(timeout)
	defer timer.Stop()
	ticker := time.NewTicker(250 * time.Millisecond)
	defer ticker.Stop()
	for {
		current, err := status(ctx)
		if err != nil {
			return err
		}
		if current == expected {
			return nil
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-timer.C:
			return context.DeadlineExceeded
		case <-ticker.C:
		}
	}
}
