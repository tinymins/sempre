package state

import "time"

const SchemaVersion = 2

type Document struct {
	Schema       int                   `json:"schema"`
	UpdatedAt    time.Time             `json:"updated_at"`
	Selected     *Selection            `json:"selected,omitempty"`
	Active       *Deployment           `json:"active,omitempty"`
	Previous     *Deployment           `json:"previous,omitempty"`
	Pending      bool                  `json:"pending"`
	LastError    string                `json:"last_error,omitempty"`
	Cores        map[string]*CoreState `json:"cores"`
	Configs      map[string]string     `json:"configs"`
	Subscription Subscription          `json:"subscription"`
	Runtime      Runtime               `json:"runtime"`
}

type Selection struct {
	Core string `json:"core"`
	Ref  string `json:"ref"`
}

type Deployment struct {
	Core       string `json:"core"`
	Ref        string `json:"ref"`
	Version    string `json:"version"`
	ConfigHash string `json:"config_hash"`
}

type CoreState struct {
	Channels  map[string]string        `json:"channels"`
	Installed map[string]*Installation `json:"installed"`
}

type Installation struct {
	Explicit    bool      `json:"explicit"`
	Digest      string    `json:"digest"`
	Source      string    `json:"source"`
	InstalledAt time.Time `json:"installed_at"`
}

type Subscription struct {
	URL        string    `json:"url,omitempty"`
	Interval   string    `json:"interval"`
	LastCheck  time.Time `json:"last_check,omitempty"`
	LastChange time.Time `json:"last_change,omitempty"`
	LastResult string    `json:"last_result,omitempty"`
}

type Runtime struct {
	State          string    `json:"state,omitempty"`
	PID            int       `json:"pid,omitempty"`
	Core           string    `json:"core,omitempty"`
	Version        string    `json:"version,omitempty"`
	StartedAt      time.Time `json:"started_at,omitempty"`
	RestartCount   int       `json:"restart_count,omitempty"`
	LastExit       string    `json:"last_exit,omitempty"`
	LastTransition time.Time `json:"last_transition,omitempty"`
}

func NewDocument() Document {
	return Document{
		Schema:  SchemaVersion,
		Cores:   map[string]*CoreState{},
		Configs: map[string]string{},
		Subscription: Subscription{
			Interval: "24h",
		},
	}
}

func (document *Document) Normalize() {
	if document.Schema <= 1 && document.Selected == nil && document.Active != nil {
		document.Selected = &Selection{
			Core: document.Active.Core,
			Ref:  document.Active.Ref,
		}
	}
	document.Schema = SchemaVersion
	if document.Cores == nil {
		document.Cores = map[string]*CoreState{}
	}
	if document.Configs == nil {
		document.Configs = map[string]string{}
	}
	if document.Subscription.Interval == "" {
		document.Subscription.Interval = "24h"
	}
	for _, core := range document.Cores {
		if core.Channels == nil {
			core.Channels = map[string]string{}
		}
		if core.Installed == nil {
			core.Installed = map[string]*Installation{}
		}
	}
}

func (document *Document) Core(name string) *CoreState {
	core := document.Cores[name]
	if core == nil {
		core = &CoreState{
			Channels:  map[string]string{},
			Installed: map[string]*Installation{},
		}
		document.Cores[name] = core
	}
	return core
}

func (document *Document) Stage(deployment Deployment) {
	if document.Active != nil && !document.Pending {
		previous := *document.Active
		document.Previous = &previous
	}
	active := deployment
	document.Active = &active
	document.Pending = true
	document.LastError = ""
}

func SameDeployment(left, right *Deployment) bool {
	if left == nil || right == nil {
		return left == nil && right == nil
	}
	return *left == *right
}
