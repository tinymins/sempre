package layout

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
)

type Mode string

const (
	System   Mode = "system"
	Portable Mode = "portable"

	PortableMarker = ".sempre-portable"
)

type Layout struct {
	Mode              Mode
	Root              string
	Home              string
	State             string
	WebConfig         string
	UI                string
	UICurrent         string
	Resources         string
	Endpoint          string
	DaemonControl     string
	CoreControl       string
	Lock              string
	OperationLock     string
	ConfigLock        string
	Cores             string
	Configs           string
	Runtime           string
	Logs              string
	StdoutLog         string
	StderrLog         string
	ManagerLog        string
	InstanceLock      string
	ServiceExecutable string
	instanceLockMode  Mode
	test              bool
}

func ForMode(mode Mode) (Layout, error) {
	switch mode {
	case System:
		return systemLayout()
	case Portable:
		executable, err := CurrentExecutable()
		if err != nil {
			return Layout{}, err
		}
		portable := portableLayout(executable)
		system, err := systemLayout()
		if err != nil {
			return Layout{}, err
		}
		portable.InstanceLock = system.InstanceLock
		portable.instanceLockMode = System
		return portable, nil
	default:
		return Layout{}, fmt.Errorf("unsupported Sempre mode %q", mode)
	}
}

func CurrentExecutable() (string, error) {
	executable, err := os.Executable()
	if err != nil {
		return "", fmt.Errorf("locate executable: %w", err)
	}
	executable, err = filepath.EvalSymlinks(executable)
	if err != nil {
		return "", fmt.Errorf("resolve executable: %w", err)
	}
	return filepath.Abs(executable)
}

func PortableMarkerPath(executable string) string {
	return filepath.Join(filepath.Dir(executable), PortableMarker)
}

func PortableMarkerEnabled(executable string) (bool, error) {
	_, err := os.Stat(PortableMarkerPath(executable))
	switch {
	case err == nil:
		return true, nil
	case errors.Is(err, os.ErrNotExist):
		return false, nil
	default:
		return false, fmt.Errorf("inspect portable marker: %w", err)
	}
}

func SetPortableMarker(executable string, enabled bool) error {
	path := PortableMarkerPath(executable)
	if !enabled {
		if err := os.Remove(path); err != nil && !errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("remove portable marker: %w", err)
		}
		return nil
	}
	file, err := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
	if errors.Is(err, os.ErrExist) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("create portable marker: %w", err)
	}
	return file.Close()
}

func portableLayout(executable string) Layout {
	root := filepath.Dir(executable)
	home := filepath.Join(root, ".sempre")
	return newLayout(Portable, root, home, filepath.Join(home, "logs"), filepath.Join(home, "run"), executable)
}

func PortableAt(executable string) Layout {
	paths := portableLayout(executable)
	system, err := systemLayout()
	if err == nil {
		paths.InstanceLock = system.InstanceLock
		paths.instanceLockMode = System
	}
	return paths
}

func newLayout(mode Mode, root, home, logs, run, serviceExecutable string) Layout {
	root, _ = filepath.Abs(root)
	home, _ = filepath.Abs(home)
	logs, _ = filepath.Abs(logs)
	run, _ = filepath.Abs(run)
	return Layout{
		Mode:              mode,
		Root:              root,
		Home:              home,
		State:             filepath.Join(home, "state.json"),
		WebConfig:         filepath.Join(home, "web.json"),
		UI:                filepath.Join(home, "ui"),
		UICurrent:         filepath.Join(home, "ui", "current"),
		Resources:         filepath.Join(root, "resources"),
		Endpoint:          filepath.Join(root, "endpoint.json"),
		DaemonControl:     filepath.Join(run, "sempre-control.json"),
		CoreControl:       filepath.Join(run, "control.json"),
		Lock:              filepath.Join(run, "state.lock"),
		OperationLock:     filepath.Join(run, "operation.lock"),
		ConfigLock:        filepath.Join(run, "config.lock"),
		Cores:             filepath.Join(home, "cores"),
		Configs:           filepath.Join(home, "configs"),
		Runtime:           run,
		Logs:              logs,
		StdoutLog:         filepath.Join(logs, "core.stdout.log"),
		StderrLog:         filepath.Join(logs, "core.stderr.log"),
		ManagerLog:        filepath.Join(logs, "sempre.log"),
		InstanceLock:      filepath.Join(run, "instance.lock"),
		ServiceExecutable: serviceExecutable,
		instanceLockMode:  mode,
	}
}

// At creates an isolated portable-style layout for tests and embedded callers.
func At(root string) Layout {
	root, _ = filepath.Abs(root)
	executable := filepath.Join(root, "sempre")
	if runtime.GOOS == "windows" {
		executable += ".exe"
	}
	paths := portableLayout(executable)
	paths.test = true
	return paths
}

// SystemAt creates an isolated system-style layout for tests.
func SystemAt(root string) Layout {
	root, _ = filepath.Abs(root)
	paths := newLayout(
		System,
		filepath.Join(root, "bin"),
		filepath.Join(root, "data"),
		filepath.Join(root, "logs"),
		filepath.Join(root, "run"),
		filepath.Join(root, "bin", executableName("sempre")),
	)
	paths.test = true
	return paths
}

func executableName(name string) string {
	if runtime.GOOS == "windows" {
		return name + ".exe"
	}
	return name
}

func (paths Layout) Ensure() error {
	roots := []string{paths.Home, paths.Logs, paths.Runtime}
	seen := map[string]bool{}
	for _, directory := range roots {
		if seen[directory] {
			continue
		}
		seen[directory] = true
		if err := ensureRoot(directory, paths.Mode, paths.test); err != nil {
			return err
		}
	}
	for _, directory := range []string{paths.Cores, paths.Configs} {
		if err := os.MkdirAll(directory, 0o700); err != nil {
			return fmt.Errorf("create %s: %w", directory, err)
		}
	}
	return nil
}

func ensureRoot(path string, mode Mode, test bool) error {
	_, statErr := os.Stat(path)
	created := errors.Is(statErr, os.ErrNotExist)
	if statErr != nil && !created {
		return fmt.Errorf("inspect %s: %w", path, statErr)
	}
	if err := os.MkdirAll(path, 0o700); err != nil {
		return fmt.Errorf("create %s: %w", path, err)
	}
	if test || (!created && mode != System) {
		return nil
	}
	return secureDirectory(path, mode)
}

func (paths Layout) EnsureServiceExecutableDirectory() error {
	if paths.Mode != System {
		return fmt.Errorf("service executable directory is only available in system mode")
	}
	directory := filepath.Dir(paths.ServiceExecutable)
	if err := os.MkdirAll(directory, 0o755); err != nil {
		return fmt.Errorf("create service executable directory: %w", err)
	}
	if paths.test {
		return nil
	}
	return secureExecutableDirectory(directory)
}

func (paths Layout) EnsureInstanceLockDirectory() error {
	return ensureRoot(filepath.Dir(paths.InstanceLock), paths.instanceLockMode, paths.test)
}

func (paths Layout) CoreVersionDir(core, repository, version string) string {
	if repository == "" {
		return filepath.Join(paths.Cores, core, version)
	}
	return filepath.Join(paths.Cores, core, "sources", filepath.FromSlash(repository), version)
}

func (paths Layout) CoreBinary(core, repository, version string) string {
	return filepath.Join(paths.CoreVersionDir(core, repository, version), executableName(core))
}

func (paths Layout) Config(core, hash string) string {
	return filepath.Join(paths.Configs, core, hash+".json")
}
