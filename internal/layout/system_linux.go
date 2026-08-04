//go:build linux

package layout

func systemLayout() (Layout, error) {
	paths := newLayout(
		System,
		"/usr/local/libexec/sempre",
		"/var/lib/sempre",
		"/var/log/sempre",
		"/run/sempre",
		"/usr/local/libexec/sempre/sempre",
	)
	paths.CommandExecutable = "/usr/local/bin/sempre"
	return paths, nil
}
