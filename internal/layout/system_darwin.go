//go:build darwin

package layout

import "path/filepath"

func systemLayout() (Layout, error) {
	paths := newLayout(
		System,
		"/Library/Application Support/Sempre/bin",
		"/Library/Application Support/Sempre/data",
		"/Library/Logs/Sempre",
		"/var/run/sempre",
		"/Library/Application Support/Sempre/bin/sempre",
	)
	paths.CommandExecutable = "/usr/local/bin/sempre"
	paths.ServiceRoot = filepath.Dir(paths.Root)
	return paths, nil
}
