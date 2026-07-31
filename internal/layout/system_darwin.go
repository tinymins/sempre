//go:build darwin

package layout

func systemLayout() (Layout, error) {
	return newLayout(
		System,
		"/Library/Application Support/Sempre/bin",
		"/Library/Application Support/Sempre/data",
		"/Library/Logs/Sempre",
		"/var/run/sempre",
		"/Library/Application Support/Sempre/bin/sempre",
	), nil
}
