//go:build linux

package layout

func systemLayout() (Layout, error) {
	return newLayout(
		System,
		"/usr/local/libexec/sempre",
		"/var/lib/sempre",
		"/var/log/sempre",
		"/run/sempre",
		"/usr/local/libexec/sempre/sempre",
	), nil
}
