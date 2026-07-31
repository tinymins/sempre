//go:build !windows

package supervisor

import (
	"os/exec"
	"syscall"
)

type processHandle struct {
	pid int
}

func configureCommand(command *exec.Cmd) {
	command.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
}

func attachProcess(command *exec.Cmd) (*processHandle, error) {
	return &processHandle{pid: command.Process.Pid}, nil
}

func gracefulStop(process *processHandle) error {
	return syscall.Kill(-process.pid, syscall.SIGTERM)
}

func forceStop(process *processHandle) error {
	return syscall.Kill(-process.pid, syscall.SIGKILL)
}

func closeProcess(process *processHandle) {}
