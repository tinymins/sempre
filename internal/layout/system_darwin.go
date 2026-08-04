//go:build darwin

package layout

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
	return paths, nil
}
