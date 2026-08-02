//go:build windows

package service

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"golang.org/x/sys/windows"
	"golang.org/x/sys/windows/svc"
	"golang.org/x/sys/windows/svc/mgr"
)

type windowsController struct{}

func New() Controller {
	return windowsController{}
}

func (windowsController) Install(ctx context.Context, executable, _ string) error {
	executable, err := filepath.Abs(executable)
	if err != nil {
		return err
	}
	manager, err := mgr.Connect()
	if err != nil {
		return fmt.Errorf("connect to Windows SCM: %w", err)
	}
	defer manager.Disconnect()
	config := mgr.Config{
		StartType:        mgr.StartAutomatic,
		ErrorControl:     mgr.ErrorNormal,
		DisplayName:      DisplayName,
		Description:      Description,
		DelayedAutoStart: true,
	}
	service, err := manager.OpenService(Name)
	if errors.Is(err, windows.ERROR_SERVICE_DOES_NOT_EXIST) {
		service, err = manager.CreateService(Name, executable, config, "--system", "daemon")
	} else if err == nil {
		var existing mgr.Config
		existing, err = service.Config()
		if err == nil {
			existing.StartType = config.StartType
			existing.ErrorControl = config.ErrorControl
			existing.BinaryPathName = windowsServiceCommand(executable)
			existing.DisplayName = config.DisplayName
			existing.Description = config.Description
			existing.DelayedAutoStart = config.DelayedAutoStart
			err = service.UpdateConfig(existing)
		}
	}
	if err != nil {
		return fmt.Errorf("install Windows service: %w", err)
	}
	defer service.Close()
	if err := service.SetRecoveryActions([]mgr.RecoveryAction{
		{Type: mgr.ServiceRestart, Delay: 5 * time.Second},
		{Type: mgr.ServiceRestart, Delay: 15 * time.Second},
		{Type: mgr.ServiceRestart, Delay: 60 * time.Second},
	}, 300); err != nil {
		return fmt.Errorf("configure service recovery: %w", err)
	}
	return service.SetRecoveryActionsOnNonCrashFailures(true)
}

func windowsServiceCommand(executable string) string {
	arguments := []string{executable, "--system", "daemon"}
	for index := range arguments {
		arguments[index] = syscall.EscapeArg(arguments[index])
	}
	return strings.Join(arguments, " ")
}

func (windowsController) Uninstall(ctx context.Context) error {
	manager, err := mgr.Connect()
	if err != nil {
		return err
	}
	defer manager.Disconnect()
	service, err := manager.OpenService(Name)
	if errors.Is(err, windows.ERROR_SERVICE_DOES_NOT_EXIST) {
		return nil
	}
	if err != nil {
		return err
	}
	defer service.Close()
	status, _ := service.Query()
	if status.State != svc.Stopped {
		_, _ = service.Control(svc.Stop)
		_ = waitWindowsService(ctx, service, svc.Stopped, 20*time.Second)
	}
	return service.Delete()
}

func (windowsController) Start(ctx context.Context) error {
	service, disconnect, err := openWindowsService()
	if err != nil {
		return err
	}
	defer disconnect()
	status, err := service.Query()
	if err != nil {
		return err
	}
	if status.State == svc.Running {
		return nil
	}
	if err := service.Start(); err != nil {
		return err
	}
	return waitWindowsService(ctx, service, svc.Running, 20*time.Second)
}

func (windowsController) Stop(ctx context.Context) error {
	service, disconnect, err := openWindowsService()
	if errors.Is(err, windows.ERROR_SERVICE_DOES_NOT_EXIST) {
		return nil
	}
	if err != nil {
		return err
	}
	defer disconnect()
	status, err := service.Query()
	if err != nil {
		return err
	}
	if status.State == svc.Stopped {
		return nil
	}
	if _, err := service.Control(svc.Stop); err != nil {
		return err
	}
	return waitWindowsService(ctx, service, svc.Stopped, 20*time.Second)
}

func (controller windowsController) Restart(ctx context.Context) error {
	if err := controller.Stop(ctx); err != nil {
		return err
	}
	return controller.Start(ctx)
}

func (windowsController) Status(ctx context.Context) (State, error) {
	manager, err := windows.OpenSCManager(nil, nil, windows.SC_MANAGER_CONNECT)
	if err != nil {
		return Unknown, err
	}
	defer windows.CloseServiceHandle(manager)
	name, err := windows.UTF16PtrFromString(Name)
	if err != nil {
		return Unknown, err
	}
	handle, err := windows.OpenService(manager, name, windows.SERVICE_QUERY_STATUS)
	if errors.Is(err, windows.ERROR_SERVICE_DOES_NOT_EXIST) {
		return NotInstalled, nil
	}
	if err != nil {
		return Unknown, err
	}
	defer windows.CloseServiceHandle(handle)
	var status windows.SERVICE_STATUS
	if err := windows.QueryServiceStatus(handle, &status); err != nil {
		return Unknown, err
	}
	switch svc.State(status.CurrentState) {
	case svc.Stopped:
		return Stopped, nil
	case svc.StartPending:
		return StartPending, nil
	case svc.Running:
		return Running, nil
	case svc.StopPending:
		return StopPending, nil
	default:
		return Unknown, nil
	}
}

func (windowsController) Run(ctx context.Context, daemon func(context.Context) error) error {
	isService, err := svc.IsWindowsService()
	if err != nil {
		return err
	}
	if !isService {
		runCtx, cancel := signal.NotifyContext(ctx, os.Interrupt)
		defer cancel()
		return daemon(runCtx)
	}
	return svc.Run(Name, &windowsHandler{parent: ctx, daemon: daemon})
}

type windowsHandler struct {
	parent context.Context
	daemon func(context.Context) error
}

func (handler *windowsHandler) Execute(
	_ []string,
	requests <-chan svc.ChangeRequest,
	status chan<- svc.Status,
) (bool, uint32) {
	status <- svc.Status{State: svc.StartPending}
	ctx, cancel := context.WithCancel(handler.parent)
	defer cancel()
	done := make(chan error, 1)
	go func() { done <- handler.daemon(ctx) }()
	status <- svc.Status{State: svc.Running, Accepts: svc.AcceptStop | svc.AcceptShutdown}
	for {
		select {
		case request := <-requests:
			switch request.Cmd {
			case svc.Interrogate:
				status <- request.CurrentStatus
			case svc.Stop, svc.Shutdown:
				status <- svc.Status{State: svc.StopPending}
				cancel()
				err := <-done
				if err != nil {
					return true, 1
				}
				status <- svc.Status{State: svc.Stopped}
				return false, 0
			}
		case err := <-done:
			if err != nil {
				return true, 1
			}
			return false, 0
		}
	}
}

func openWindowsService() (*mgr.Service, func(), error) {
	manager, err := mgr.Connect()
	if err != nil {
		return nil, func() {}, err
	}
	service, err := manager.OpenService(Name)
	if err != nil {
		manager.Disconnect()
		return nil, func() {}, err
	}
	return service, func() {
		service.Close()
		manager.Disconnect()
	}, nil
}

func waitWindowsService(ctx context.Context, service *mgr.Service, expected svc.State, timeout time.Duration) error {
	timer := time.NewTimer(timeout)
	defer timer.Stop()
	ticker := time.NewTicker(250 * time.Millisecond)
	defer ticker.Stop()
	for {
		status, err := service.Query()
		if err != nil {
			return err
		}
		if status.State == expected {
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
