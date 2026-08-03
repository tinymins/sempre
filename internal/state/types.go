package state

import (
	"fmt"
	"net/url"
	"regexp"
	"strings"
	"time"
)

const SchemaVersion = 2

var (
	coreIDPattern  = regexp.MustCompile(`^[a-z0-9][a-z0-9-]*$`)
	versionPattern = regexp.MustCompile(`^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$`)
	hashPattern    = regexp.MustCompile(`^[0-9a-fA-F]{64}$`)
)

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

func (document Document) Validate() error {
	if document.Schema != SchemaVersion {
		return fmt.Errorf("unsupported state schema %d", document.Schema)
	}
	for coreID, coreState := range document.Cores {
		if !coreIDPattern.MatchString(coreID) {
			return fmt.Errorf("invalid core ID %q", coreID)
		}
		if coreState == nil {
			return fmt.Errorf("core %q has no state", coreID)
		}
		for version, installation := range coreState.Installed {
			if !versionPattern.MatchString(version) {
				return fmt.Errorf("core %q has invalid version %q", coreID, version)
			}
			if installation == nil {
				return fmt.Errorf("core %q version %q has no installation", coreID, version)
			}
		}
		for channel, version := range coreState.Channels {
			if channel != "stable" {
				return fmt.Errorf("core %q has unsupported channel %q", coreID, channel)
			}
			if !versionPattern.MatchString(version) || coreState.Installed[version] == nil {
				return fmt.Errorf("core %q channel %q references unavailable version %q", coreID, channel, version)
			}
		}
	}
	for coreID, hash := range document.Configs {
		if !coreIDPattern.MatchString(coreID) {
			return fmt.Errorf("invalid configuration core ID %q", coreID)
		}
		if !hashPattern.MatchString(hash) {
			return fmt.Errorf("core %q has invalid configuration hash %q", coreID, hash)
		}
	}
	if document.Selected != nil {
		if err := document.validateSelection(*document.Selected); err != nil {
			return err
		}
	}
	if document.Active != nil {
		if err := document.validateDeployment("active", *document.Active); err != nil {
			return err
		}
	}
	if document.Previous != nil {
		if err := document.validateDeployment("previous", *document.Previous); err != nil {
			return err
		}
	}
	if document.Subscription.Interval != "off" {
		interval, err := time.ParseDuration(document.Subscription.Interval)
		if err != nil || interval < 5*time.Minute {
			return fmt.Errorf("invalid subscription interval %q", document.Subscription.Interval)
		}
	}
	if value := document.Subscription.URL; value != "" {
		parsed, err := url.Parse(value)
		if err != nil || !strings.EqualFold(parsed.Scheme, "https") || parsed.Hostname() == "" || parsed.User != nil {
			return fmt.Errorf("invalid subscription URL")
		}
	}
	if document.Runtime.PID < 0 || document.Runtime.RestartCount < 0 {
		return fmt.Errorf("invalid runtime counters")
	}
	if document.Runtime.Core != "" && !coreIDPattern.MatchString(document.Runtime.Core) {
		return fmt.Errorf("invalid runtime core ID %q", document.Runtime.Core)
	}
	if document.Runtime.Version != "" && !versionPattern.MatchString(document.Runtime.Version) {
		return fmt.Errorf("invalid runtime version %q", document.Runtime.Version)
	}
	switch document.Runtime.State {
	case "", "idle", "starting", "running", "restarting", "stopped", "failed":
	default:
		return fmt.Errorf("invalid runtime state %q", document.Runtime.State)
	}
	return nil
}

func (document Document) validateSelection(selection Selection) error {
	if !coreIDPattern.MatchString(selection.Core) {
		return fmt.Errorf("selected core has invalid ID %q", selection.Core)
	}
	coreState := document.Cores[selection.Core]
	if coreState == nil {
		return fmt.Errorf("selected core %q is not installed", selection.Core)
	}
	if selection.Ref == "stable" {
		if coreState.Channels[selection.Ref] == "" {
			return fmt.Errorf("selected core %q has no stable channel", selection.Core)
		}
		return nil
	}
	if !versionPattern.MatchString(selection.Ref) || coreState.Installed[selection.Ref] == nil {
		return fmt.Errorf("selected core %q references unavailable version %q", selection.Core, selection.Ref)
	}
	return nil
}

func (document Document) validateDeployment(name string, deployment Deployment) error {
	if !coreIDPattern.MatchString(deployment.Core) {
		return fmt.Errorf("%s deployment has invalid core ID %q", name, deployment.Core)
	}
	if deployment.Ref != "stable" && !versionPattern.MatchString(deployment.Ref) {
		return fmt.Errorf("%s deployment has invalid reference %q", name, deployment.Ref)
	}
	if !versionPattern.MatchString(deployment.Version) {
		return fmt.Errorf("%s deployment has invalid version %q", name, deployment.Version)
	}
	coreState := document.Cores[deployment.Core]
	if coreState == nil || coreState.Installed[deployment.Version] == nil {
		return fmt.Errorf("%s deployment references unavailable %s@%s", name, deployment.Core, deployment.Version)
	}
	if !hashPattern.MatchString(deployment.ConfigHash) {
		return fmt.Errorf("%s deployment has invalid configuration hash %q", name, deployment.ConfigHash)
	}
	return nil
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
