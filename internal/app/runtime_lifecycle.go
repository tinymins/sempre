package app

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/state"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
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

type RuntimeFailure struct {
	Stage        string             `json:"stage"`
	Error        string             `json:"error"`
	OccurredAt   time.Time          `json:"occurred_at"`
	Failed       *RuntimeDeployment `json:"failed,omitempty"`
	RolledBackTo *RuntimeDeployment `json:"rolled_back_to,omitempty"`
}

type RuntimeStatus struct {
	DesiredState   string             `json:"desired_state"`
	RuntimeState   string             `json:"runtime_state"`
	Active         *RuntimeDeployment `json:"active"`
	Target         *RuntimeDeployment `json:"target,omitempty"`
	PID            int                `json:"pid"`
	StartedAt      *time.Time         `json:"started_at"`
	UptimeSeconds  int64              `json:"uptime_seconds"`
	RestartCount   int                `json:"restart_count"`
	Pending        bool               `json:"pending"`
	LastTransition *time.Time         `json:"last_transition"`
	LastExit       string             `json:"last_exit,omitempty"`
	LastError      string             `json:"last_error,omitempty"`
	LastFailure    *RuntimeFailure    `json:"last_failure,omitempty"`
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
	return manager.ManagedRuntimeActionContext(context.Background(), action)
}

func (manager *Manager) ManagedRuntimeActionContext(ctx context.Context, action string) (RuntimeStatus, error) {
	manager.lifecycleMu.Lock()
	defer manager.lifecycleMu.Unlock()

	document, err := manager.store.Read()
	if err != nil {
		return RuntimeStatus{}, err
	}
	current := manager.runtimeStatusValue(document)
	if action == RuntimeStart && document.DesiredState == state.DesiredRunning &&
		(current.RuntimeState == "running" || current.RuntimeState == "starting" || current.RuntimeState == "restarting") &&
		!manager.runtimeConfigurationPending(document) {
		return current, nil
	}
	if action == RuntimeRestart && document.DesiredState == state.DesiredRunning && isRuntimeTransition(current.RuntimeState) &&
		!manager.runtimeConfigurationPending(document) {
		return current, nil
	}
	if action != RuntimeStart && action != RuntimeStop && action != RuntimeRestart {
		return current, runtimeActionFailure("INVALID_RUNTIME_ACTION", fmt.Errorf("runtime action must be start, stop, or restart"))
	}
	if action == RuntimeStart || action == RuntimeRestart {
		var prepareErr error
		err := manager.withOperation(func() error {
			_, prepareErr = manager.prepareActiveProfileForRuntime(ctx)
			return prepareErr
		})
		if err != nil {
			if recordErr := manager.recordRuntimePreparationFailure(err); recordErr != nil {
				return RuntimeStatus{}, fmt.Errorf("%w (record runtime preparation failure: %v)", err, recordErr)
			}
			failed, statusErr := manager.ManagedRuntimeStatus()
			if statusErr != nil {
				return RuntimeStatus{}, statusErr
			}
			return failed, runtimeActionFailure("RUNTIME_PREPARATION_FAILED", err)
		}
		document, err = manager.store.Read()
		if err != nil {
			return RuntimeStatus{}, err
		}
		current = manager.runtimeStatusValue(document)
	}
	deployment, readyErr := manager.runtimeDeployment(document)

	switch action {
	case RuntimeStart:
		if readyErr != nil {
			return current, runtimeActionFailure("RUNTIME_NOT_READY", readyErr)
		}
		err = manager.store.Update(func(document *state.Document) error {
			if err := manager.ensureRuntimeDeployment(document, deployment); err != nil {
				return err
			}
			document.DesiredState = state.DesiredRunning
			if document.Runtime.State == "stopping" {
				document.Runtime.State = "restarting"
			} else {
				document.Runtime.State = "starting"
			}
			document.Runtime.PID = 0
			document.Runtime.LastError = ""
			document.Runtime.LastFailure = nil
			document.Runtime.LastTransition = time.Now().UTC()
			return nil
		})
	case RuntimeStop:
		if document.DesiredState == state.DesiredStopped {
			return current, nil
		}
		err = manager.store.Update(func(document *state.Document) error {
			document.DesiredState = state.DesiredStopped
			if document.Active == nil {
				document.Runtime.State = "stopped"
				document.Runtime.PID = 0
				document.Runtime.LastTransition = time.Now().UTC()
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
		err = manager.store.Update(func(document *state.Document) error {
			if err := manager.ensureRuntimeDeployment(document, deployment); err != nil {
				return err
			}
			document.DesiredState = state.DesiredRunning
			if document.Runtime.PID > 0 || document.Runtime.State == "running" {
				document.Runtime.State = "stopping"
			} else {
				document.Runtime.State = "restarting"
				document.Runtime.PID = 0
			}
			document.Runtime.LastError = ""
			document.Runtime.LastFailure = nil
			document.Runtime.LastTransition = time.Now().UTC()
			return nil
		})
	}
	if err != nil {
		return RuntimeStatus{}, err
	}
	manager.RequestReload()
	return manager.ManagedRuntimeStatus()
}

func (manager *Manager) runtimeStatusValue(document state.Document) RuntimeStatus {
	deployment, readyErr := manager.runtimeDeployment(document)
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
	if lastError == "" && runtimeState != "running" {
		lastError = document.LastError
	}
	if runtimeState == "idle" && document.DesiredState == state.DesiredRunning && lastError != "" && readyErr == nil {
		runtimeState = "failed"
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
		Pending:      document.Pending || manager.runtimeConfigurationPending(document),
		LastExit:     document.Runtime.LastExit,
		LastError:    lastError,
		LastFailure:  runtimeFailureValue(document.Runtime.LastFailure),
		Actions:      manager.runtimeActions(document, runtimeState, readyErr),
	}
	if document.Active != nil {
		status.Active = runtimeDeploymentValue(*document.Active)
	} else if readyErr == nil {
		status.Target = runtimeDeploymentValue(deployment)
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

func (manager *Manager) runtimeConfigurationPending(document state.Document) bool {
	if document.Selected == nil {
		return false
	}
	catalog, err := manager.subscriptions.Read()
	if err != nil {
		return false
	}
	profile, err := subscriptions.FindProfile(&catalog, document.ActiveProfileID)
	if err != nil {
		return false
	}
	if !subscriptionProfileHasInputs(*profile) && profile.Revision == 1 && document.ConfigBuilds[document.Selected.Core].ProfileID == "" {
		return false
	}
	deployment, adapter, err := manager.configurationTarget(document)
	if err != nil {
		return false
	}
	expected, err := expectedConfigBuild(*profile, adapter, deployment.Version)
	return err == nil && document.ConfigBuilds[deployment.Core] != expected
}

func (manager *Manager) recordRuntimePreparationFailure(failure error) error {
	return manager.store.Update(func(document *state.Document) error {
		now := time.Now().UTC()
		record := &state.RuntimeFailure{Stage: "prepare runtime configuration", Error: failure.Error(), OccurredAt: now}
		if document.Active != nil {
			active := *document.Active
			record.RolledBackTo = &active
		}
		document.LastError = "prepare runtime configuration: " + failure.Error()
		document.Runtime.LastError = failure.Error()
		document.Runtime.LastFailure = record
		document.Runtime.LastTransition = now
		return nil
	})
}

func (manager *Manager) runtimeActions(document state.Document, runtimeState string, readyErr error) RuntimeActions {
	readyReason := ""
	if readyErr != nil {
		readyReason = readyErr.Error()
	}
	start := RuntimeActionAvailability{Allowed: readyErr == nil}
	restart := RuntimeActionAvailability{Allowed: readyErr == nil}
	stop := RuntimeActionAvailability{Allowed: document.DesiredState == state.DesiredRunning}
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
	}
	return RuntimeActions{Start: start, Stop: stop, Restart: restart}
}

func (manager *Manager) runtimeReadiness(document state.Document) error {
	_, err := manager.runtimeDeployment(document)
	return err
}

func (manager *Manager) runtimeDeployment(document state.Document) (state.Deployment, error) {
	var deployment state.Deployment
	if document.Active != nil {
		deployment = *document.Active
	} else {
		var err error
		deployment, _, err = manager.configurationTarget(document)
		if err != nil {
			return state.Deployment{}, err
		}
		if deployment.ConfigHash == "" {
			return state.Deployment{}, fmt.Errorf("no active configuration; import a configuration first")
		}
	}
	binary, err := manager.coreBinary(deployment.Core, deployment.Repository, deployment.Version)
	if err != nil {
		return state.Deployment{}, err
	}
	if _, err := os.Stat(binary); err != nil {
		return state.Deployment{}, fmt.Errorf("managed core binary is unavailable: %w", err)
	}
	config := manager.paths.Config(deployment.Core, deployment.ConfigHash)
	if _, err := os.Stat(config); err != nil {
		return state.Deployment{}, fmt.Errorf("managed configuration is unavailable: %w", err)
	}
	return deployment, nil
}

func (manager *Manager) ensureRuntimeDeployment(document *state.Document, expected state.Deployment) error {
	current, err := manager.runtimeDeployment(*document)
	if err != nil {
		return err
	}
	if current != expected {
		return fmt.Errorf("managed core deployment changed while applying the runtime action; retry the action")
	}
	if document.Active == nil {
		document.Stage(current)
	}
	return nil
}

func runtimeDeploymentValue(deployment state.Deployment) *RuntimeDeployment {
	return &RuntimeDeployment{
		Core:           deployment.Core,
		Repository:     deployment.Repository,
		Ref:            deployment.Ref,
		Version:        deployment.Version,
		ExactReference: exactRef(core.Ref{Core: deployment.Core, Repository: deployment.Repository}, deployment.Version).String(),
		ConfigHash:     deployment.ConfigHash,
	}
}

func runtimeFailureValue(failure *state.RuntimeFailure) *RuntimeFailure {
	if failure == nil {
		return nil
	}
	result := &RuntimeFailure{Stage: failure.Stage, Error: failure.Error, OccurredAt: failure.OccurredAt}
	if failure.Failed != nil {
		result.Failed = runtimeDeploymentValue(*failure.Failed)
	}
	if failure.RolledBackTo != nil {
		result.RolledBackTo = runtimeDeploymentValue(*failure.RolledBackTo)
	}
	return result
}

func runtimeActionFailure(code string, err error) *RuntimeActionError {
	return &RuntimeActionError{Code: code, Message: err.Error()}
}

func isRuntimeTransition(value string) bool {
	return value == "starting" || value == "stopping" || value == "restarting"
}
