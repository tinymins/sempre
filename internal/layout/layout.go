package layout

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
)

type Layout struct {
	Root         string
	Home         string
	State        string
	Lock         string
	Cores        string
	Configs      string
	Runtime      string
	Logs         string
	StdoutLog    string
	StderrLog    string
	ManagerLog   string
	InstanceLock string
}

func FromExecutable() (Layout, error) {
	executable, err := os.Executable()
	if err != nil {
		return Layout{}, fmt.Errorf("locate executable: %w", err)
	}
	executable, err = filepath.EvalSymlinks(executable)
	if err != nil {
		return Layout{}, fmt.Errorf("resolve executable: %w", err)
	}
	return At(filepath.Dir(executable)), nil
}

func At(root string) Layout {
	root, _ = filepath.Abs(root)
	home := filepath.Join(root, ".sempre")
	logs := filepath.Join(home, "logs")
	return Layout{
		Root:         root,
		Home:         home,
		State:        filepath.Join(home, "state.json"),
		Lock:         filepath.Join(home, "state.lock"),
		Cores:        filepath.Join(home, "cores"),
		Configs:      filepath.Join(home, "configs"),
		Runtime:      filepath.Join(home, "run"),
		Logs:         logs,
		StdoutLog:    filepath.Join(logs, "core.stdout.log"),
		StderrLog:    filepath.Join(logs, "core.stderr.log"),
		ManagerLog:   filepath.Join(logs, "sempre.log"),
		InstanceLock: filepath.Join(home, "instance.lock"),
	}
}

func (paths Layout) Ensure() error {
	_, statErr := os.Stat(paths.Home)
	created := os.IsNotExist(statErr)
	if statErr != nil && !created {
		return fmt.Errorf("inspect %s: %w", paths.Home, statErr)
	}
	for _, directory := range []string{paths.Home, paths.Cores, paths.Configs, paths.Runtime, paths.Logs} {
		if err := os.MkdirAll(directory, 0o700); err != nil {
			return fmt.Errorf("create %s: %w", directory, err)
		}
	}
	if !created {
		return nil
	}
	return secureDirectory(paths.Home)
}

func (paths Layout) CoreVersionDir(core, version string) string {
	return filepath.Join(paths.Cores, core, version)
}

func (paths Layout) CoreBinary(core, version string) string {
	name := core
	if runtime.GOOS == "windows" {
		name += ".exe"
	}
	return filepath.Join(paths.CoreVersionDir(core, version), name)
}

func (paths Layout) Config(core, hash string) string {
	return filepath.Join(paths.Configs, core, hash+".json")
}
