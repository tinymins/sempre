//go:build !windows

package app

import (
	"fmt"
	"os"
	"syscall"
)

func checkProtectedPath(path string) error {
	info, err := os.Stat(path)
	if err != nil {
		return err
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok {
		return fmt.Errorf("ownership is unavailable")
	}
	if stat.Uid != 0 {
		return fmt.Errorf("owner UID is %d, want 0", stat.Uid)
	}
	if info.Mode().Perm()&0o022 != 0 {
		return fmt.Errorf("is writable by group or other users")
	}
	return nil
}
