//go:build windows

package supervisor

import (
	"os/exec"
	"unsafe"

	"golang.org/x/sys/windows"
)

const createNewProcessGroup = 0x00000200

var generateConsoleCtrlEvent = windows.NewLazySystemDLL("kernel32.dll").NewProc("GenerateConsoleCtrlEvent")

type processHandle struct {
	pid uint32
	job windows.Handle
}

func configureCommand(command *exec.Cmd) {
	command.SysProcAttr = &windows.SysProcAttr{
		CreationFlags: createNewProcessGroup | windows.CREATE_NO_WINDOW,
	}
}

func attachProcess(command *exec.Cmd) (*processHandle, error) {
	job, err := windows.CreateJobObject(nil, nil)
	if err != nil {
		return nil, err
	}
	information := windows.JOBOBJECT_EXTENDED_LIMIT_INFORMATION{}
	information.BasicLimitInformation.LimitFlags = windows.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
	if _, err := windows.SetInformationJobObject(
		job,
		windows.JobObjectExtendedLimitInformation,
		uintptr(unsafe.Pointer(&information)),
		uint32(unsafe.Sizeof(information)),
	); err != nil {
		windows.CloseHandle(job)
		return nil, err
	}
	process, err := windows.OpenProcess(
		windows.PROCESS_SET_QUOTA|windows.PROCESS_TERMINATE,
		false,
		uint32(command.Process.Pid),
	)
	if err != nil {
		windows.CloseHandle(job)
		return nil, err
	}
	defer windows.CloseHandle(process)
	if err := windows.AssignProcessToJobObject(job, process); err != nil {
		windows.CloseHandle(job)
		return nil, err
	}
	return &processHandle{pid: uint32(command.Process.Pid), job: job}, nil
}

func gracefulStop(process *processHandle) error {
	result, _, err := generateConsoleCtrlEvent.Call(uintptr(windows.CTRL_BREAK_EVENT), uintptr(process.pid))
	if result == 0 {
		return err
	}
	return nil
}

func forceStop(process *processHandle) error {
	return windows.TerminateJobObject(process.job, 1)
}

func closeProcess(process *processHandle) {
	_ = windows.CloseHandle(process.job)
}
