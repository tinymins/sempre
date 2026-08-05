package state

import (
	"fmt"
	"net/url"
	"regexp"
	"strings"
	"time"
)

const SchemaVersion = 6

const (
	DesiredRunning = "running"
	DesiredStopped = "stopped"
)

var (
	coreIDPattern     = regexp.MustCompile(`^[a-z0-9][a-z0-9-]*$`)
	repositoryPattern = regexp.MustCompile(`^[a-z0-9](?:[a-z0-9-]{0,38})/[a-z0-9_.-]{1,100}$`)
	versionPattern    = regexp.MustCompile(`^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$`)
	hashPattern       = regexp.MustCompile(`^[0-9a-fA-F]{64}$`)
)

type Document struct {
	Schema          int                    `json:"schema"`
	UpdatedAt       time.Time              `json:"updated_at"`
	Selected        *Selection             `json:"selected,omitempty"`
	Active          *Deployment            `json:"active,omitempty"`
	Previous        *Deployment            `json:"previous,omitempty"`
	Pending         bool                   `json:"pending"`
	LastError       string                 `json:"last_error,omitempty"`
	Cores           map[string]*CoreState  `json:"cores"`
	Configs         map[string]string      `json:"configs"`
	ConfigBuilds    map[string]ConfigBuild `json:"config_builds"`
	Subscription    Subscription           `json:"subscription"`
	ActiveProfileID string                 `json:"active_profile_id,omitempty"`
	AutoRestart     bool                   `json:"subscription_auto_restart"`
	DesiredState    string                 `json:"desired_state"`
	Runtime         Runtime                `json:"runtime"`
}

type Selection struct {
	Core       string `json:"core"`
	Repository string `json:"repository,omitempty"`
	Ref        string `json:"ref"`
}

type ConfigBuild struct {
	ProfileID       string `json:"profile_id"`
	ProfileRevision uint64 `json:"profile_revision"`
	TargetKey       string `json:"target_key"`
}

type Deployment struct {
	Core       string `json:"core"`
	Repository string `json:"repository,omitempty"`
	Ref        string `json:"ref"`
	Version    string `json:"version"`
	ConfigHash string `json:"config_hash"`
}

type CoreState struct {
	Default SourceState             `json:"default"`
	Custom  map[string]*SourceState `json:"custom,omitempty"`
	// Schema v2 fields are retained only for decoding and migration.
	Channels  map[string]string        `json:"channels,omitempty"`
	Installed map[string]*Installation `json:"installed,omitempty"`
}

type SourceState struct {
	Channels  map[string]string        `json:"channels"`
	Installed map[string]*Installation `json:"installed"`
}

type SourceEntry struct {
	Repository string
	State      *SourceState
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
	Repository     string    `json:"repository,omitempty"`
	Ref            string    `json:"ref,omitempty"`
	Version        string    `json:"version,omitempty"`
	ConfigHash     string    `json:"config_hash,omitempty"`
	RuntimeConfig  string    `json:"runtime_config,omitempty"`
	RuntimeHash    string    `json:"runtime_config_hash,omitempty"`
	StartedAt      time.Time `json:"started_at,omitempty"`
	RestartCount   int       `json:"restart_count,omitempty"`
	LastExit       string    `json:"last_exit,omitempty"`
	LastError      string    `json:"last_error,omitempty"`
	LastTransition time.Time `json:"last_transition,omitempty"`
}

func NewDocument() Document {
	return Document{
		Schema:       SchemaVersion,
		Cores:        map[string]*CoreState{},
		Configs:      map[string]string{},
		ConfigBuilds: map[string]ConfigBuild{},
		DesiredState: DesiredRunning,
		AutoRestart:  true,
		Subscription: Subscription{
			Interval: "24h",
		},
	}
}

func (document *Document) Normalize() {
	previousSchema := document.Schema
	if previousSchema <= 3 && document.DesiredState == "" {
		document.DesiredState = DesiredRunning
	}
	if document.Schema <= 1 && document.Selected == nil && document.Active != nil {
		document.Selected = &Selection{
			Core:       document.Active.Core,
			Repository: document.Active.Repository,
			Ref:        document.Active.Ref,
		}
	}
	document.Schema = SchemaVersion
	if document.Cores == nil {
		document.Cores = map[string]*CoreState{}
	}
	if document.Configs == nil {
		document.Configs = map[string]string{}
	}
	if document.ConfigBuilds == nil {
		document.ConfigBuilds = map[string]ConfigBuild{}
	}
	if document.Subscription.Interval == "" {
		document.Subscription.Interval = "24h"
	}
	if previousSchema <= 4 {
		document.AutoRestart = true
	}
	for _, core := range document.Cores {
		if core == nil {
			continue
		}
		core.Default.normalize()
		if previousSchema <= 2 {
			for channel, version := range core.Channels {
				core.Default.Channels[channel] = version
			}
			for version, installation := range core.Installed {
				core.Default.Installed[version] = installation
			}
			core.Channels = nil
			core.Installed = nil
		}
		if core.Custom == nil {
			core.Custom = map[string]*SourceState{}
		}
		for _, source := range core.Custom {
			if source != nil {
				source.normalize()
			}
		}
	}
}

func (document Document) Validate() error {
	if document.Schema != SchemaVersion {
		return fmt.Errorf("unsupported state schema %d", document.Schema)
	}
	switch document.DesiredState {
	case DesiredRunning, DesiredStopped:
	default:
		return fmt.Errorf("invalid desired runtime state %q", document.DesiredState)
	}
	for coreID, coreState := range document.Cores {
		if !coreIDPattern.MatchString(coreID) {
			return fmt.Errorf("invalid core ID %q", coreID)
		}
		if coreState == nil {
			return fmt.Errorf("core %q has no state", coreID)
		}
		if err := validateSource(coreID, "default", &coreState.Default); err != nil {
			return err
		}
		for repository, source := range coreState.Custom {
			if !validRepository(repository) {
				return fmt.Errorf("core %q has invalid repository %q", coreID, repository)
			}
			if source == nil {
				return fmt.Errorf("core %q repository %q has no state", coreID, repository)
			}
			if err := validateSource(coreID, repository, source); err != nil {
				return err
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
	for coreID, build := range document.ConfigBuilds {
		if !coreIDPattern.MatchString(coreID) {
			return fmt.Errorf("invalid configuration build core ID %q", coreID)
		}
		if document.Configs[coreID] == "" {
			return fmt.Errorf("core %q has configuration build metadata without a configuration", coreID)
		}
		if strings.TrimSpace(build.ProfileID) == "" || build.ProfileRevision == 0 || strings.TrimSpace(build.TargetKey) == "" {
			return fmt.Errorf("core %q has invalid configuration build metadata", coreID)
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
	if document.Runtime.Repository != "" && !validRepository(document.Runtime.Repository) {
		return fmt.Errorf("invalid runtime repository %q", document.Runtime.Repository)
	}
	if document.Runtime.Version != "" && !versionPattern.MatchString(document.Runtime.Version) {
		return fmt.Errorf("invalid runtime version %q", document.Runtime.Version)
	}
	if document.Runtime.Ref != "" && document.Runtime.Ref != "stable" && !versionPattern.MatchString(document.Runtime.Ref) {
		return fmt.Errorf("invalid runtime reference %q", document.Runtime.Ref)
	}
	if document.Runtime.ConfigHash != "" && !hashPattern.MatchString(document.Runtime.ConfigHash) {
		return fmt.Errorf("invalid runtime configuration hash %q", document.Runtime.ConfigHash)
	}
	if document.Runtime.RuntimeHash != "" && !hashPattern.MatchString(document.Runtime.RuntimeHash) {
		return fmt.Errorf("invalid prepared runtime configuration hash %q", document.Runtime.RuntimeHash)
	}
	switch document.Runtime.State {
	case "", "idle", "starting", "running", "stopping", "restarting", "stopped", "failed":
	default:
		return fmt.Errorf("invalid runtime state %q", document.Runtime.State)
	}
	return nil
}

func validateSource(coreID, repository string, source *SourceState) error {
	for version, installation := range source.Installed {
		if !versionPattern.MatchString(version) {
			return fmt.Errorf("core %q repository %q has invalid version %q", coreID, repository, version)
		}
		if installation == nil {
			return fmt.Errorf("core %q repository %q version %q has no installation", coreID, repository, version)
		}
	}
	for channel, version := range source.Channels {
		if channel != "stable" {
			return fmt.Errorf("core %q repository %q has unsupported channel %q", coreID, repository, channel)
		}
		if !versionPattern.MatchString(version) || source.Installed[version] == nil {
			return fmt.Errorf("core %q repository %q channel %q references unavailable version %q", coreID, repository, channel, version)
		}
	}
	return nil
}

func validRepository(repository string) bool {
	if !repositoryPattern.MatchString(repository) {
		return false
	}
	_, name, _ := strings.Cut(repository, "/")
	return name != "." && name != ".."
}

func (document Document) validateSelection(selection Selection) error {
	if !coreIDPattern.MatchString(selection.Core) {
		return fmt.Errorf("selected core has invalid ID %q", selection.Core)
	}
	coreState := document.Cores[selection.Core]
	if coreState == nil {
		return fmt.Errorf("selected core %q is not installed", selection.Core)
	}
	source := coreState.LookupSource(selection.Repository)
	if source == nil {
		return fmt.Errorf("selected core %q repository %q is not installed", selection.Core, selection.Repository)
	}
	if selection.Ref == "stable" {
		if source.Channels[selection.Ref] == "" {
			return fmt.Errorf("selected core %q has no stable channel", selection.Core)
		}
		return nil
	}
	if !versionPattern.MatchString(selection.Ref) || source.Installed[selection.Ref] == nil {
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
	if coreState == nil || coreState.LookupSource(deployment.Repository) == nil || coreState.LookupSource(deployment.Repository).Installed[deployment.Version] == nil {
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
		core = &CoreState{}
		core.normalize()
		document.Cores[name] = core
	}
	return core
}

func (core *CoreState) Source(repository string) *SourceState {
	if repository == "" {
		core.Default.normalize()
		return &core.Default
	}
	if core.Custom == nil {
		core.Custom = map[string]*SourceState{}
	}
	source := core.Custom[repository]
	if source == nil {
		source = &SourceState{}
		source.normalize()
		core.Custom[repository] = source
	}
	return source
}

func (core *CoreState) LookupSource(repository string) *SourceState {
	if repository == "" {
		return &core.Default
	}
	return core.Custom[repository]
}

func (core *CoreState) SourceEntries() []SourceEntry {
	entries := []SourceEntry{{State: &core.Default}}
	for repository, source := range core.Custom {
		entries = append(entries, SourceEntry{Repository: repository, State: source})
	}
	return entries
}

func (core *CoreState) Empty() bool {
	if len(core.Default.Channels) != 0 || len(core.Default.Installed) != 0 {
		return false
	}
	for _, source := range core.Custom {
		if source != nil && (len(source.Channels) != 0 || len(source.Installed) != 0) {
			return false
		}
	}
	return true
}

func (core *CoreState) normalize() {
	core.Default.normalize()
	if core.Custom == nil {
		core.Custom = map[string]*SourceState{}
	}
}

func (source *SourceState) normalize() {
	if source.Channels == nil {
		source.Channels = map[string]string{}
	}
	if source.Installed == nil {
		source.Installed = map[string]*Installation{}
	}
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
