package installscript

import (
	"fmt"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/tinymins/sempre/internal/state"
)

var argumentPattern = regexp.MustCompile(`^[a-z][a-z-]*$`)

func Write(directory, executable, goos string, arguments ...string) error {
	for _, argument := range arguments {
		if !argumentPattern.MatchString(argument) {
			return fmt.Errorf("invalid installer argument %q", argument)
		}
	}
	executableName := filepath.Base(executable)
	command := strings.Join(arguments, " ")
	unix := fmt.Sprintf("#!/bin/sh\nset -eu\ncd -- \"$(dirname -- \"$0\")\"\n\"./%s\" %s \"$@\"\n", executableName, command)
	switch goos {
	case "windows":
		windows := fmt.Sprintf("@echo off\r\ncd /d \"%%~dp0\"\r\n\"%%~dp0%s\" %s %%*\r\nset EXITCODE=%%ERRORLEVEL%%\r\npause\r\nexit /b %%EXITCODE%%\r\n", executableName, command)
		return state.WriteAtomic(filepath.Join(directory, "install.cmd"), []byte(windows), 0o755)
	case "darwin":
		if err := state.WriteAtomic(filepath.Join(directory, "install.command"), []byte(unix), 0o755); err != nil {
			return err
		}
		return state.WriteAtomic(filepath.Join(directory, "install.sh"), []byte(unix), 0o755)
	default:
		if err := state.WriteAtomic(filepath.Join(directory, "install.sh"), []byte(unix), 0o755); err != nil {
			return err
		}
		desktop := "[Desktop Entry]\nType=Application\nName=Install Sempre Bundle\nTerminal=true\nExec=sh -c 'cd \"$(dirname \"$1\")\" && sh install.sh' sh %k\n"
		return state.WriteAtomic(filepath.Join(directory, "install.desktop"), []byte(desktop), 0o755)
	}
}
