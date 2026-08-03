//go:build !windows

package app

import "os"

func renamePath(source, target string) error {
	return os.Rename(source, target)
}
