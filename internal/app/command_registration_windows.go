//go:build windows

package app

import (
	"errors"
	"fmt"
	"path/filepath"
	"strings"
	"syscall"
	"unsafe"

	"github.com/tinymins/sempre/internal/layout"
	"golang.org/x/sys/windows"
	"golang.org/x/sys/windows/registry"
)

const machineEnvironmentKey = `SYSTEM\CurrentControlSet\Control\Session Manager\Environment`

var sendMessageTimeout = windows.NewLazySystemDLL("user32.dll").NewProc("SendMessageTimeoutW")

func registerCommand(paths layout.Layout) (func() error, error) {
	root := filepath.Dir(paths.CommandExecutable)
	key, err := registry.OpenKey(registry.LOCAL_MACHINE, machineEnvironmentKey, registry.QUERY_VALUE|registry.SET_VALUE)
	if err != nil {
		return nil, fmt.Errorf("open machine environment: %w", err)
	}
	defer key.Close()
	value, valueType, err := key.GetStringValue("Path")
	missing := errors.Is(err, registry.ErrNotExist)
	if err != nil && !missing {
		return nil, fmt.Errorf("read machine PATH: %w", err)
	}
	updated, changed := addWindowsPathEntry(value, root)
	if !changed {
		return func() error { return nil }, nil
	}
	if missing {
		valueType = registry.EXPAND_SZ
	}
	if err := setRegistryString(key, "Path", updated, valueType); err != nil {
		return nil, fmt.Errorf("update machine PATH: %w", err)
	}
	rollback := func() error {
		return restoreMachinePath(value, valueType, missing)
	}
	if err := broadcastEnvironmentChange(); err != nil {
		return nil, errors.Join(err, rollback())
	}
	return rollback, nil
}

func restoreMachinePath(value string, valueType uint32, missing bool) error {
	key, err := registry.OpenKey(registry.LOCAL_MACHINE, machineEnvironmentKey, registry.SET_VALUE)
	if err != nil {
		return fmt.Errorf("open machine environment: %w", err)
	}
	defer key.Close()
	if missing {
		err = key.DeleteValue("Path")
		if errors.Is(err, registry.ErrNotExist) {
			err = nil
		}
	} else {
		err = setRegistryString(key, "Path", value, valueType)
	}
	return errors.Join(err, broadcastEnvironmentChange())
}

func unregisterCommand(paths layout.Layout) error {
	root := filepath.Dir(paths.CommandExecutable)
	key, err := registry.OpenKey(registry.LOCAL_MACHINE, machineEnvironmentKey, registry.QUERY_VALUE|registry.SET_VALUE)
	if err != nil {
		return fmt.Errorf("open machine environment: %w", err)
	}
	defer key.Close()
	value, valueType, err := key.GetStringValue("Path")
	if errors.Is(err, registry.ErrNotExist) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("read machine PATH: %w", err)
	}
	updated, changed := removeWindowsPathEntry(value, root)
	if !changed {
		return nil
	}
	if err := setRegistryString(key, "Path", updated, valueType); err != nil {
		return fmt.Errorf("update machine PATH: %w", err)
	}
	return broadcastEnvironmentChange()
}

func checkCommandRegistration(paths layout.Layout) error {
	root := filepath.Dir(paths.CommandExecutable)
	key, err := registry.OpenKey(registry.LOCAL_MACHINE, machineEnvironmentKey, registry.QUERY_VALUE)
	if err != nil {
		return fmt.Errorf("open machine environment: %w", err)
	}
	defer key.Close()
	value, _, err := key.GetStringValue("Path")
	if err != nil {
		return fmt.Errorf("read machine PATH: %w", err)
	}
	_, changed := addWindowsPathEntry(value, root)
	if changed {
		return fmt.Errorf("%s is not registered in the machine PATH", root)
	}
	return nil
}

func addWindowsPathEntry(value, entry string) (string, bool) {
	for _, current := range strings.Split(value, ";") {
		if sameWindowsPath(current, entry) {
			return value, false
		}
	}
	if value == "" {
		return entry, true
	}
	if strings.HasSuffix(value, ";") {
		return value + entry, true
	}
	return value + ";" + entry, true
}

func removeWindowsPathEntry(value, entry string) (string, bool) {
	parts := strings.Split(value, ";")
	kept := make([]string, 0, len(parts))
	changed := false
	for _, current := range parts {
		if sameWindowsPath(current, entry) {
			changed = true
			continue
		}
		kept = append(kept, current)
	}
	return strings.Join(kept, ";"), changed
}

func sameWindowsPath(left, right string) bool {
	normalize := func(value string) string {
		value = strings.TrimSpace(value)
		value = strings.Trim(value, `"`)
		value = strings.ReplaceAll(value, "/", `\`)
		value = strings.TrimRight(value, `\`)
		return strings.ToLower(value)
	}
	return normalize(left) == normalize(right)
}

func setRegistryString(key registry.Key, name, value string, valueType uint32) error {
	if valueType == registry.EXPAND_SZ {
		return key.SetExpandStringValue(name, value)
	}
	return key.SetStringValue(name, value)
}

func broadcastEnvironmentChange() error {
	name, err := syscall.UTF16PtrFromString("Environment")
	if err != nil {
		return err
	}
	result, _, callErr := sendMessageTimeout.Call(
		0xffff,
		0x001a,
		0,
		uintptr(unsafe.Pointer(name)),
		0x0002,
		5000,
		0,
	)
	if result == 0 {
		return fmt.Errorf("broadcast environment change: %w", callErr)
	}
	return nil
}
