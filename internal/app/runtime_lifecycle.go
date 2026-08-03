package app

import (
	"fmt"
	"os"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
)

const (
	RuntimeStart   = "start"
	RuntimeStop    = "stop"
	RuntimeRestart = "restart"
)

type RuntimeActionAvailability struct {
	Allowed bool   `json:"allowed"`
	Reason  string `json:"reason,omitempty"`
}

type RuntimeActions struct {
	Start   RuntimeActionAvailability `json:"start"`
	Stop    RuntimeActionAvailability `json:"stop"`
	Restart RuntimeActionAvailability `json:"restart"`
}

type RuntimeDeployment struct {
	Core           string `json:"core"`
	Repository     string `json:"repository,omitempty"`
	Ref            string `json:"ref"`
	Version        string `json:"version"`
	ExactReference string `json:"exact_reference"`
	ConfigHash     string `json:"config_hash"`
}

type RuntimeStatus struct {
	DesiredState   string             `json:"desired_state"`
	RuntimeState   string             `json:"runtime_state"`
	Active         *RuntimeDeployment `json:"active"`
	PID            int                `json:"pid"`
	StartedAt      *time.Time         `json:"started_at"`
	UptimeSeconds  int64              `json:"uptime_seconds"`
	RestartCount   int                `json:"restart_count"`
	Pending        bool               `json:"pending"`
	LastTransition *time.Time         `json:"last_transition"`
	LastExit       string             `json:"last_exit,omitempty"`
	LastError      string             `json:"last_error,omitempty"`
	Actions        RuntimeActions     `json:"actions"`
}

type RuntimeActionError struct {
	Code    string
	Message string
}

func (failure *RuntimeActionError) Error() string {
	return failure.Message
}

func (manager *Manager) ManagedRuntimeStatus() (RuntimeStatus, error) {
	document, err := manager.store.Read()
	if err != nil {
		return RuntimeStatus{}, err
	}
	return manager.runtimeStatusValue(document), nil
}

func (manager *Manager) ManagedRuntimeAction(action string) (RuntimeStatus, error) {
	manager.lifecycleMu.Lock()
	defer manager.lifecycleMu.Unlock()

	document, err := manager.store.Read()
	if err != nil {
		return RuntimeStatus{}, err
	}
	current := manager.runtimeStatusValue(document)
	readyErr := manager.runtimeReadiness(document)

	switch action {
	case RuntimeStart:
		if readyErr != nil {
			return current, runtimeActionFailure("RUNTIME_NOT_READY", readyErr)
		}
		if document.DesiredState == state.DesiredRunning &&
			(current.RuntimeState == "running" || current.RuntimeState == "starting" || current.RuntimeState == "restarting") {
			return current, nil
		}
		err = manager.store.Update(func(document *state.Document) error {
			document.DesiredState = state.DesiredRunning
			if document.Runtime.State == "stopping" {
				document.Runtime.State = "restarting"
			} else {
				document.Runtime.State = "starting"
			}
			document.Runtime.PID = 0
			document.Runtime.LastTransition = time.Now().UTC()
			return nil
		})
	case RuntimeStop:
		if document.DesiredState == state.DesiredStopped &&
			(document.Runtime.State == "stopped" || document.Runtime.State == "idle") {
			return current, nil
		}
		err = manager.store.Update(func(document *state.Document) error {
			document.DesiredState = state.DesiredStopped
			if document.Active == nil {
				document.Runtime = state.Runtime{State: "idle", LastTransition: time.Now().UTC()}
			} else if document.Runtime.PID > 0 || isRuntimeTransition(document.Runtime.State) || document.Runtime.State == "running" {
				document.Runtime.State = "stopping"
				document.Runtime.LastTransition = time.Now().UTC()
			} else {
				document.Runtime.State = "stopped"
				document.Runtime.PID = 0
				document.Runtime.LastTransition = time.Now().UTC()
			}
			return nil
		})
	case RuntimeRestart:
		if readyErr != nil {
			return current, runtimeActionFailure("RUNTIME_NOT_READY", readyErr)
		}
		if document.DesiredState == state.DesiredRunning && isRuntimeTransition(current.RuntimeState) {
			return current, nil
		}
		err = manager.store.Update(func(document *state.Document) error {
			document.DesiredState = state.DesiredRunning
			if document.Runtime.PID > 0 || document.Runtime.State == "running" {
				document.Runtime.State = "stopping"
			} else {
				document.Runtime.State = "restarting"
				document.Runtime.PID = 0
			}
			document.Runtime.LastTransition = time.Now().UTC()
			return nil
		})
	default:
		return current, runtimeActionFailure("INVALID_RUNTIME_ACTION", fmt.Errorf("runtime action must be start, stop, or restart"))
	}
	if err != nil {
		return RuntimeStatus{}, err
	}
	manager.RequestReload()
	return manager.ManagedRuntimeStatus()
}

func (manager *Manager) runtimeStatusValue(document state.Document) RuntimeStatus {
	runtimeState := document.Runtime.State
	if runtimeState == "" {
		if document.Active == nil {
			runtimeState = "idle"
		} else if document.DesiredState == state.DesiredStopped {
			runtimeState = "stopped"
		} else {
			runtimeState = "starting"
		}
	}
	lastError := document.Runtime.LastError
	if lastError == "" {
		lastError = document.LastError
	}
	if document.Runtime.PID > 0 && !processAlive(document.Runtime.PID) {
		runtimeState = "failed"
		lastError = fmt.Sprintf("recorded PID %d is not running", document.Runtime.PID)
	}
	status := RuntimeStatus{
		DesiredState: document.DesiredState,
		RuntimeState: runtimeState,
		PID:          document.Runtime.PID,
		RestartCount: document.Runtime.RestartCount,
		Pending:      document.Pending,
		LastExit:     document.Runtime.LastExit,
		LastError:    lastError,
		Actions:      manager.runtimeActions(document, runtimeState),
	}
	if document.Active != nil {
		status.Active = &RuntimeDeployment{
			Core:           document.Active.Core,
			Repository:     document.Active.Repository,
			Ref:            document.Active.Ref,
			Version:        document.Active.Version,
			ExactReference: exactRef(core.Ref{Core: document.Active.Core, Repository: document.Active.Repository}, document.Active.Version).String(),
			ConfigHash:     document.Active.ConfigHash,
		}
	}
	if !document.Runtime.StartedAt.IsZero() {
		started := document.Runtime.StartedAt
		status.StartedAt = &started
		if document.Runtime.PID > 0 {
			status.UptimeSeconds = max(int64(0), int64(time.Since(started).Seconds()))
		}
	}
	if !document.Runtime.LastTransition.IsZero() {
		transition := document.Runtime.LastTransition
		status.LastTransition = &transition
	}
	return status
}

func (manager *Manager) runtimeActions(document state.Document, runtimeState string) RuntimeActions {
	readyErr := manager.runtimeReadiness(document)
	readyReason := ""
	if readyErr != nil {
		readyReason = readyErr.Error()
	}
	start := RuntimeActionAvailability{Allowed: readyErr == nil}
	restart := RuntimeActionAvailability{Allowed: readyErr == nil}
	stop := RuntimeActionAvailability{Allowed: document.DesiredState == state.DesiredRunning && runtimeState != "idle"}
	if readyErr != nil {
		start.Reason = readyReason
		restart.Reason = readyReason
	}
	if runtimeState == "running" {
		start.Allowed = false
		start.Reason = "managed core is already running"
	}
	if runtimeState == "starting" || runtimeState == "restarting" {
		start.Allowed = false
		start.Reason = "managed core is " + runtimeState
		restart.Allowed = false
		restart.Reason = start.Reason
	}
	if runtimeState == "stopping" {
		start.Allowed = false
		start.Reason = "managed core is stopping"
		restart.Allowed = false
		restart.Reason = start.Reason
	}
	if document.DesiredState == state.DesiredStopped {
		stop.Allowed = false
		stop.Reason = "managed core is already stopped"
	} else if runtimeState == "idle" {
		stop.Reason = "no active core deployment is available"
	}
	return RuntimeActions{Start: start, Stop: stop, Restart: restart}
}

func (manager *Manager) runtimeReadiness(document state.Document) error {
	if document.Active == nil {
		return fmt.Errorf("no active core deployment; select a core and import a configuration first")
	}
	binary := manager.paths.CoreBinary(document.Active.Core, document.Active.Repository, document.Active.Version)
	if _, err := os.Stat(binary); err != nil {
		return fmt.Errorf("active core binary is unavailable: %w", err)
	}
	config := manager.paths.Config(document.Active.Core, document.Active.ConfigHash)
	if _, err := os.Stat(config); err != nil {
		return fmt.Errorf("active configuration is unavailable: %w", err)
	}
	return nil
}

func runtimeActionFailure(code string, err error) *RuntimeActionError {
	return &RuntimeActionError{Code: code, Message: err.Error()}
}

func isRuntimeTransition(value string) bool {
	return value == "starting" || value == "stopping" || value == "restarting"
}
