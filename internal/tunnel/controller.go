package tunnel

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/tinymins/sempre/internal/core"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/supervisor"
)

type BinaryStatus struct {
	Version   string `json:"version"`
	Installed bool   `json:"installed"`
}

type InstanceStatus struct {
	ID           string     `json:"id"`
	State        string     `json:"state"`
	RestartCount int        `json:"restart_count"`
	StartedAt    *time.Time `json:"started_at,omitempty"`
	LastError    string     `json:"last_error,omitempty"`
	LogPath      string     `json:"log_path"`
}

type Status struct {
	Config    Config            `json:"config"`
	Binary    BinaryStatus      `json:"binary"`
	Instances []InstanceStatus  `json:"instances"`
	Forwards  []ForwardEndpoint `json:"forwards"`
}

type worker struct {
	signature string
	cancel    context.CancelFunc
	done      chan struct{}
}

type Controller struct {
	paths    layout.Layout
	store    *Store
	opMu     sync.Mutex
	mu       sync.Mutex
	ctx      context.Context
	cancel   context.CancelFunc
	workers  map[string]*worker
	statuses map[string]InstanceStatus
}

func New(paths layout.Layout) (*Controller, error) {
	store := NewStore(paths)
	if err := store.Initialize(); err != nil {
		return nil, err
	}
	return &Controller{paths: paths, store: store, workers: map[string]*worker{}, statuses: map[string]InstanceStatus{}}, nil
}

func (controller *Controller) Read() (Config, error) {
	return controller.store.Read()
}

func (controller *Controller) Update(config Config) (Config, error) {
	controller.opMu.Lock()
	defer controller.opMu.Unlock()
	saved, err := controller.store.Update(config)
	if err != nil {
		return Config{}, err
	}
	if err := controller.reconcile(saved); err != nil {
		return Config{}, err
	}
	return saved, nil
}

func (controller *Controller) Start(ctx context.Context) error {
	controller.opMu.Lock()
	defer controller.opMu.Unlock()
	config, err := controller.store.Read()
	if err != nil {
		return err
	}
	controller.mu.Lock()
	if controller.ctx != nil {
		controller.mu.Unlock()
		return nil
	}
	controller.ctx, controller.cancel = context.WithCancel(ctx)
	controller.mu.Unlock()
	return controller.reconcile(config)
}

func (controller *Controller) Stop() {
	controller.opMu.Lock()
	defer controller.opMu.Unlock()
	controller.mu.Lock()
	if controller.cancel != nil {
		controller.cancel()
	}
	workers := controller.takeWorkersLocked()
	controller.ctx = nil
	controller.cancel = nil
	controller.mu.Unlock()
	waitWorkers(workers)
}

func (controller *Controller) Action(id, action string) (Status, error) {
	controller.opMu.Lock()
	defer controller.opMu.Unlock()
	config, err := controller.store.Read()
	if err != nil {
		return Status{}, err
	}
	index := instanceIndex(config, id)
	if index < 0 {
		return Status{}, fmt.Errorf("tunnel instance %q was not found", id)
	}
	switch action {
	case "start":
		config.Instances[index].DesiredState = DesiredRunning
	case "stop":
		config.Instances[index].DesiredState = DesiredStopped
	case "restart":
		if config.Instances[index].DesiredState != DesiredRunning {
			return Status{}, fmt.Errorf("stopped tunnel instance %q cannot be restarted", id)
		}
		controller.stopWorker(id)
	default:
		return Status{}, fmt.Errorf("unsupported tunnel action %q", action)
	}
	if action != "restart" {
		config, err = controller.store.Update(config)
		if err != nil {
			return Status{}, err
		}
	}
	if err := controller.reconcile(config); err != nil {
		return Status{}, err
	}
	return controller.status(config), nil
}

func (controller *Controller) Status() (Status, error) {
	config, err := controller.store.Read()
	if err != nil {
		return Status{}, err
	}
	return controller.status(config), nil
}

func (controller *Controller) Forward(id string) (ForwardEndpoint, bool) {
	config, err := controller.store.Read()
	if err != nil {
		return ForwardEndpoint{}, false
	}
	return config.Forward(id)
}

func (controller *Controller) Install(ctx context.Context) (BinaryStatus, error) {
	_, _, err := EnsureBinary(ctx, controller.paths)
	return BinaryStatus{Version: Version, Installed: Installed(controller.paths)}, err
}

func (controller *Controller) Log(id string) (string, error) {
	if !idPattern.MatchString(id) {
		return "", fmt.Errorf("invalid tunnel instance ID")
	}
	data, err := os.ReadFile(controller.logPath(id))
	if errors.Is(err, os.ErrNotExist) {
		return "", nil
	}
	if err != nil {
		return "", err
	}
	const limit = 256 << 10
	if len(data) > limit {
		data = data[len(data)-limit:]
	}
	return string(data), nil
}

func (controller *Controller) reconcile(config Config) error {
	controller.mu.Lock()
	if controller.ctx == nil {
		controller.mu.Unlock()
		return nil
	}
	desired := map[string]Instance{}
	for _, instance := range config.Instances {
		if instance.DesiredState == DesiredRunning {
			desired[instance.ID] = instance
		}
	}
	stopped := []*worker{}
	for id, running := range controller.workers {
		instance, wanted := desired[id]
		if wanted && running.signature == instanceSignature(instance) {
			delete(desired, id)
			continue
		}
		running.cancel()
		stopped = append(stopped, running)
		delete(controller.workers, id)
		controller.statuses[id] = InstanceStatus{ID: id, State: "stopping", LogPath: controller.logPath(id)}
	}
	ctx := controller.ctx
	controller.mu.Unlock()
	waitWorkers(stopped)
	controller.mu.Lock()
	defer controller.mu.Unlock()
	if controller.ctx == nil || controller.ctx != ctx {
		return nil
	}
	for id, instance := range desired {
		workerCtx, cancel := context.WithCancel(ctx)
		item := &worker{signature: instanceSignature(instance), cancel: cancel, done: make(chan struct{})}
		controller.workers[id] = item
		controller.statuses[id] = InstanceStatus{ID: id, State: "starting", LogPath: controller.logPath(id)}
		go controller.runWorker(workerCtx, instance, item.done)
	}
	for _, instance := range config.Instances {
		if instance.DesiredState == DesiredStopped {
			controller.statuses[instance.ID] = InstanceStatus{ID: instance.ID, State: "stopped", LogPath: controller.logPath(instance.ID)}
		}
	}
	return nil
}

func (controller *Controller) runWorker(ctx context.Context, instance Instance, done chan struct{}) {
	defer close(done)
	backoff := time.Second
	restarts := 0
	for {
		controller.setStatus(instance.ID, InstanceStatus{ID: instance.ID, State: "installing", RestartCount: restarts, LogPath: controller.logPath(instance.ID)})
		binary, _, err := EnsureBinary(ctx, controller.paths)
		if err != nil {
			if !controller.retry(ctx, instance.ID, err, restarts, backoff) {
				return
			}
			restarts++
			backoff = nextBackoff(backoff)
			continue
		}
		log := supervisor.NewRollingWriter(controller.logPath(instance.ID), 5<<20, 2)
		spec := core.RunSpec{Path: binary, Args: BuildArgs(instance), WorkingDir: filepath.Dir(binary)}
		finished := make(chan error, 1)
		controller.setStatus(instance.ID, InstanceStatus{ID: instance.ID, State: "starting", RestartCount: restarts, LogPath: controller.logPath(instance.ID)})
		go func() { finished <- supervisor.RunForeground(ctx, spec, log, log) }()
		started := time.NewTimer(500 * time.Millisecond)
		select {
		case err = <-finished:
			started.Stop()
		case <-started.C:
			now := time.Now().UTC()
			controller.setStatus(instance.ID, InstanceStatus{ID: instance.ID, State: "running", RestartCount: restarts, StartedAt: &now, LogPath: controller.logPath(instance.ID)})
			err = <-finished
		}
		if ctx.Err() != nil {
			controller.setStatus(instance.ID, InstanceStatus{ID: instance.ID, State: "stopped", RestartCount: restarts, LogPath: controller.logPath(instance.ID)})
			return
		}
		if err == nil {
			err = fmt.Errorf("wstunnel exited unexpectedly")
		}
		if !controller.retry(ctx, instance.ID, err, restarts, backoff) {
			return
		}
		restarts++
		backoff = nextBackoff(backoff)
	}
}

func (controller *Controller) retry(ctx context.Context, id string, failure error, restarts int, backoff time.Duration) bool {
	controller.setStatus(id, InstanceStatus{ID: id, State: "restarting", RestartCount: restarts + 1, LastError: failure.Error(), LogPath: controller.logPath(id)})
	timer := time.NewTimer(backoff)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return false
	case <-timer.C:
		return true
	}
}

func (controller *Controller) status(config Config) Status {
	controller.mu.Lock()
	defer controller.mu.Unlock()
	statuses := make([]InstanceStatus, 0, len(config.Instances))
	for _, instance := range config.Instances {
		status, ok := controller.statuses[instance.ID]
		if !ok {
			status = InstanceStatus{ID: instance.ID, State: "stopped", LogPath: controller.logPath(instance.ID)}
		}
		statuses = append(statuses, status)
	}
	return Status{Config: config, Binary: BinaryStatus{Version: Version, Installed: Installed(controller.paths)}, Instances: statuses, Forwards: config.ForwardEndpoints()}
}

func (controller *Controller) setStatus(id string, status InstanceStatus) {
	controller.mu.Lock()
	controller.statuses[id] = status
	controller.mu.Unlock()
}

func (controller *Controller) stopWorker(id string) {
	controller.mu.Lock()
	item := controller.workers[id]
	if item != nil {
		item.cancel()
		delete(controller.workers, id)
	}
	controller.mu.Unlock()
	if item != nil {
		waitWorkers([]*worker{item})
	}
}

func (controller *Controller) takeWorkersLocked() []*worker {
	workers := make([]*worker, 0, len(controller.workers))
	for id, item := range controller.workers {
		item.cancel()
		workers = append(workers, item)
		delete(controller.workers, id)
	}
	return workers
}

func (controller *Controller) logPath(id string) string {
	return filepath.Join(controller.paths.TunnelLogs, id+".log")
}

func instanceIndex(config Config, id string) int {
	for index := range config.Instances {
		if config.Instances[index].ID == id {
			return index
		}
	}
	return -1
}

func instanceSignature(instance Instance) string {
	data, _ := json.Marshal(instance)
	return string(data)
}

func waitWorkers(workers []*worker) {
	for _, item := range workers {
		select {
		case <-item.done:
		case <-time.After(10 * time.Second):
		}
	}
}

func nextBackoff(value time.Duration) time.Duration {
	value *= 2
	if value > time.Minute {
		return time.Minute
	}
	return value
}
