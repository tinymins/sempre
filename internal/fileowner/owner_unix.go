//go:build !windows

package fileowner

import (
	"errors"
	"os"
	"path/filepath"
	"syscall"
)

func MatchParent(path string) error {
	if os.Geteuid() != 0 {
		return nil
	}
	parent, err := os.Stat(filepath.Dir(path))
	if err != nil {
		return err
	}
	stat, ok := parent.Sys().(*syscall.Stat_t)
	if !ok {
		return errors.New("parent ownership is unavailable")
	}
	return os.Chown(path, int(stat.Uid), int(stat.Gid))
}
