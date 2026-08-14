package installscript

import (
	"fmt"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/tinymins/sempre/internal/state"
)

var argumentPattern = regexp.MustCompile(`^[a-z][a-z-]*$`)

var entrypointLabels = map[string]string{
	"install": "Install Sempre",
	"restore": "Restore Sempre Snapshot",
}

func Write(directory, executable, goos, entrypoint string, arguments ...string) error {
	label, ok := entrypointLabels[entrypoint]
	if !ok {
		return fmt.Errorf("invalid installer entrypoint %q", entrypoint)
	}
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
		return state.WriteAtomic(filepath.Join(directory, entrypoint+".cmd"), []byte(windows), 0o755)
	case "darwin":
		if err := state.WriteAtomic(filepath.Join(directory, entrypoint+".command"), []byte(unix), 0o755); err != nil {
			return err
		}
		return state.WriteAtomic(filepath.Join(directory, entrypoint+".sh"), []byte(unix), 0o755)
	default:
		if err := state.WriteAtomic(filepath.Join(directory, entrypoint+".sh"), []byte(unix), 0o755); err != nil {
			return err
		}
		desktop := fmt.Sprintf("[Desktop Entry]\nType=Application\nName=%s\nTerminal=true\nExec=sh -c 'cd \"$(dirname \"$1\")\" && sh %s.sh' sh %%k\n", label, entrypoint)
		return state.WriteAtomic(filepath.Join(directory, entrypoint+".desktop"), []byte(desktop), 0o755)
	}
}
