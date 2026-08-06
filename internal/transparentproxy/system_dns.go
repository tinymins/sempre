//go:build linux

package transparentproxy

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/tinymins/sempre/internal/state"
)

const systemDNSStateFile = "resolv-conf.json"

type systemDNSManager struct {
	allowed    bool
	stateDir   string
	resolvConf string
}

type systemDNSState struct {
	Original string `json:"original"`
}

var systemDNSChattr = runSystemDNSChattr

func (manager *systemDNSManager) Apply() error {
	if manager == nil || !manager.allowed {
		return fmt.Errorf("system DNS takeover is only available in Linux system mode")
	}
	info, err := os.Lstat(manager.resolvConf)
	if err != nil {
		return fmt.Errorf("inspect %s: %w", manager.resolvConf, err)
	}
	if info.Mode()&os.ModeSymlink != 0 {
		return fmt.Errorf("system DNS takeover does not support symlink-managed %s", manager.resolvConf)
	}
	current, err := os.ReadFile(manager.resolvConf)
	if err != nil {
		return fmt.Errorf("read %s: %w", manager.resolvConf, err)
	}
	if bytes.Equal(current, systemDNSContent()) {
		return nil
	}
	if err := os.MkdirAll(manager.stateDir, 0o700); err != nil {
		return fmt.Errorf("create system DNS state directory: %w", err)
	}
	if _, err := os.Stat(manager.statePath()); errors.Is(err, os.ErrNotExist) {
		encoded, marshalErr := json.MarshalIndent(systemDNSState{Original: string(current)}, "", "  ")
		if marshalErr != nil {
			return marshalErr
		}
		if err := state.WriteAtomic(manager.statePath(), append(encoded, '\n'), 0o600); err != nil {
			return fmt.Errorf("write system DNS backup: %w", err)
		}
	} else if err != nil {
		return fmt.Errorf("inspect system DNS backup: %w", err)
	}
	if err := state.WriteAtomic(manager.resolvConf, systemDNSContent(), 0o644); err != nil {
		return fmt.Errorf("write %s: %w", manager.resolvConf, err)
	}
	_ = systemDNSChattr(manager.resolvConf, true)
	return nil
}

func (manager *systemDNSManager) Restore() error {
	if manager == nil || manager.stateDir == "" {
		return nil
	}
	backup, err := os.ReadFile(manager.statePath())
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("read system DNS backup: %w", err)
	}
	var saved systemDNSState
	if err := json.Unmarshal(backup, &saved); err != nil {
		return fmt.Errorf("decode system DNS backup: %w", err)
	}
	_ = systemDNSChattr(manager.resolvConf, false)
	current, err := os.ReadFile(manager.resolvConf)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("read %s: %w", manager.resolvConf, err)
	}
	if systemDNSManaged(current) || errors.Is(err, os.ErrNotExist) {
		if writeErr := state.WriteAtomic(manager.resolvConf, []byte(saved.Original), 0o644); writeErr != nil {
			return fmt.Errorf("restore %s: %w", manager.resolvConf, writeErr)
		}
	}
	if removeErr := os.Remove(manager.statePath()); removeErr != nil && !errors.Is(removeErr, os.ErrNotExist) {
		return fmt.Errorf("remove system DNS backup: %w", removeErr)
	}
	return nil
}

func (manager *systemDNSManager) Verify() error {
	if manager == nil || !manager.allowed {
		return nil
	}
	current, err := os.ReadFile(manager.resolvConf)
	if err != nil {
		return fmt.Errorf("read %s: %w", manager.resolvConf, err)
	}
	if !systemDNSManaged(current) {
		return fmt.Errorf("%s is not managed by Sempre system DNS takeover", manager.resolvConf)
	}
	return nil
}

func (manager *systemDNSManager) statePath() string {
	return filepath.Join(manager.stateDir, systemDNSStateFile)
}

func systemDNSContent() []byte {
	return []byte("# Managed by Sempre system DNS takeover. Do not edit while enabled.\nnameserver 127.0.0.1\noptions timeout:1 attempts:1\n")
}

func runSystemDNSChattr(path string, immutable bool) error {
	binary, err := exec.LookPath("chattr")
	if errors.Is(err, exec.ErrNotFound) {
		return nil
	}
	if err != nil {
		return err
	}
	flag := "-i"
	if immutable {
		flag = "+i"
	}
	output, err := exec.Command(binary, flag, path).CombinedOutput()
	if err != nil {
		return fmt.Errorf("chattr %s: %w: %s", flag, err, strings.TrimSpace(string(output)))
	}
	return nil
}

func systemDNSManaged(data []byte) bool {
	for _, line := range strings.Split(string(data), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 || strings.HasPrefix(fields[0], "#") {
			continue
		}
		if fields[0] != "nameserver" {
			continue
		}
		return len(fields) == 2 && fields[1] == "127.0.0.1"
	}
	return false
}
