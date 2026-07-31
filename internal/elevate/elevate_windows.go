//go:build windows

package elevate

import (
	"fmt"
	"os"
	"strings"
	"syscall"
	"unsafe"

	"golang.org/x/sys/windows"
)

const (
	seeMaskNoCloseProcess = 0x00000040
	swShowNormal          = 1
)

var shellExecuteEx = windows.NewLazySystemDLL("shell32.dll").NewProc("ShellExecuteExW")

type shellExecuteInfo struct {
	Size       uint32
	Mask       uint32
	Window     windows.Handle
	Verb       *uint16
	File       *uint16
	Parameters *uint16
	Directory  *uint16
	Show       int32
	Instance   windows.Handle
	IDList     uintptr
	Class      *uint16
	ClassKey   windows.Handle
	HotKey     uint32
	Icon       windows.Handle
	Process    windows.Handle
}

func Ensure(arguments []string) (bool, int, error) {
	if !requiresAdministrator(arguments) {
		return false, 0, nil
	}
	if windows.GetCurrentProcessToken().IsElevated() {
		return false, 0, nil
	}
	for _, argument := range arguments {
		if argument == "--elevated" {
			return false, 0, fmt.Errorf("Windows denied administrator access")
		}
	}
	executable, err := os.Executable()
	if err != nil {
		return false, 0, err
	}
	elevatedArguments := append([]string{"--elevated"}, arguments...)
	escaped := make([]string, 0, len(elevatedArguments))
	for _, argument := range elevatedArguments {
		escaped = append(escaped, syscall.EscapeArg(argument))
	}
	verb, _ := windows.UTF16PtrFromString("runas")
	file, _ := windows.UTF16PtrFromString(executable)
	parameters, _ := windows.UTF16PtrFromString(strings.Join(escaped, " "))
	directory, _ := windows.UTF16PtrFromString(filepathDir(executable))
	information := shellExecuteInfo{
		Mask:       seeMaskNoCloseProcess,
		Verb:       verb,
		File:       file,
		Parameters: parameters,
		Directory:  directory,
		Show:       swShowNormal,
	}
	information.Size = uint32(unsafe.Sizeof(information))
	result, _, callErr := shellExecuteEx.Call(uintptr(unsafe.Pointer(&information)))
	if result == 0 {
		return false, 0, fmt.Errorf("request administrator access: %w", callErr)
	}
	defer windows.CloseHandle(information.Process)
	if _, err := windows.WaitForSingleObject(information.Process, windows.INFINITE); err != nil {
		return false, 0, err
	}
	var exitCode uint32
	if err := windows.GetExitCodeProcess(information.Process, &exitCode); err != nil {
		return false, 0, err
	}
	return true, int(exitCode), nil
}

func requiresAdministrator(arguments []string) bool {
	var values []string
	for _, argument := range arguments {
		if argument != "--elevated" && argument != "--yes" && argument != "--no-restart" {
			values = append(values, strings.ToLower(argument))
		}
	}
	if len(values) == 0 {
		return false
	}
	switch values[0] {
	case "run":
		return true
	case "core":
		return false
	case "subscription":
		return false
	case "config":
		return false
	case "service":
		return len(values) > 1 && values[1] != "status"
	default:
		return false
	}
}

func filepathDir(path string) string {
	index := strings.LastIndexAny(path, `\/`)
	if index < 0 {
		return "."
	}
	return path[:index]
}
