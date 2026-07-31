//go:build !windows

package layout

import "os"

func secureDirectory(path string, _ Mode) error {
	return os.Chmod(path, 0o700)
}

func secureExecutableDirectory(path string) error {
	return os.Chmod(path, 0o755)
}
