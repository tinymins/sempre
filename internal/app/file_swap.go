package app

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/tinymins/sempre/internal/state"
)

type swapOperation struct {
	staged       string
	target       string
	backup       string
	hadTarget    bool
	active       bool
	needsRestore bool
	rename       func(string, string) error
	removeAll    func(string) error
}

func stageExecutable(source, target string) (*swapOperation, error) {
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		return nil, err
	}
	data, err := os.ReadFile(source)
	if err != nil {
		return nil, fmt.Errorf("read Sempre executable: %w", err)
	}
	staging, err := unusedSibling(target, ".sempre-bin-*")
	if err != nil {
		return nil, err
	}
	if err := state.WriteAtomic(staging, data, 0o755); err != nil {
		return nil, err
	}
	return &swapOperation{staged: staging, target: target}, nil
}

func stageStateFile(target string, document state.Document) (*swapOperation, error) {
	data, err := json.MarshalIndent(document, "", "  ")
	if err != nil {
		return nil, err
	}
	data = append(data, '\n')
	if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
		return nil, err
	}
	staging, err := unusedSibling(target, ".sempre-state-*")
	if err != nil {
		return nil, err
	}
	if err := state.WriteAtomic(staging, data, 0o600); err != nil {
		return nil, err
	}
	return &swapOperation{staged: staging, target: target}, nil
}

func stageDirectory(target string) (string, error) {
	if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
		return "", err
	}
	staging, err := os.MkdirTemp(filepath.Dir(target), ".sempre-deploy-*")
	if err != nil {
		return "", err
	}
	if err := os.Chmod(staging, 0o700); err != nil {
		os.RemoveAll(staging)
		return "", err
	}
	return staging, nil
}

func activateSwaps(operations []*swapOperation) error {
	for index, operation := range operations {
		if err := operation.activate(); err != nil {
			rollbackErr := error(nil)
			for rollbackIndex := index; rollbackIndex >= 0; rollbackIndex-- {
				rollbackErr = errors.Join(rollbackErr, operations[rollbackIndex].rollback())
			}
			if rollbackErr != nil {
				return errors.Join(err, fmt.Errorf("roll back activated files: %w", rollbackErr))
			}
			return err
		}
	}
	return nil
}

func (operation *swapOperation) activate() error {
	if _, err := os.Stat(operation.target); err == nil {
		backup, err := unusedSibling(operation.target, ".sempre-backup-*")
		if err != nil {
			return err
		}
		if err := operation.renameFile(operation.target, backup); err != nil {
			return fmt.Errorf("back up %s: %w", operation.target, err)
		}
		operation.backup = backup
		operation.hadTarget = true
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if err := operation.renameFile(operation.staged, operation.target); err != nil {
		activationErr := fmt.Errorf("activate %s: %w", operation.target, err)
		if operation.hadTarget {
			if restoreErr := operation.renameFile(operation.backup, operation.target); restoreErr != nil {
				operation.needsRestore = true
				return errors.Join(activationErr, fmt.Errorf("restore %s: %w", operation.target, restoreErr))
			}
			operation.backup = ""
			operation.hadTarget = false
		}
		return activationErr
	}
	operation.staged = ""
	operation.active = true
	return nil
}

func (operation *swapOperation) rollback() error {
	if operation.needsRestore {
		if err := operation.renameFile(operation.backup, operation.target); err != nil {
			return fmt.Errorf("restore %s from %s: %w", operation.target, operation.backup, err)
		}
		operation.backup = ""
		operation.hadTarget = false
		operation.needsRestore = false
		return nil
	}
	if !operation.active {
		return nil
	}
	if err := operation.removePath(operation.target); err != nil {
		return err
	}
	if operation.hadTarget {
		if err := operation.renameFile(operation.backup, operation.target); err != nil {
			return err
		}
		operation.backup = ""
		operation.hadTarget = false
	}
	operation.active = false
	return nil
}

func (operation *swapOperation) commit() error {
	if operation.hadTarget {
		if err := operation.removePath(operation.backup); err != nil {
			return fmt.Errorf("remove committed backup %s: %w", operation.backup, err)
		}
		operation.backup = ""
		operation.hadTarget = false
	}
	operation.active = false
	return nil
}

func (operation *swapOperation) cleanup() {
	if operation.staged != "" {
		_ = os.RemoveAll(operation.staged)
	}
	// Backups are removed only by a confirmed commit or successful rollback.
}

func (operation *swapOperation) renameFile(source, target string) error {
	if operation.rename != nil {
		return operation.rename(source, target)
	}
	return os.Rename(source, target)
}

func (operation *swapOperation) removePath(path string) error {
	if operation.removeAll != nil {
		return operation.removeAll(path)
	}
	return os.RemoveAll(path)
}

func unusedSibling(target, pattern string) (string, error) {
	file, err := os.CreateTemp(filepath.Dir(target), pattern)
	if err != nil {
		return "", err
	}
	path := file.Name()
	if err := file.Close(); err != nil {
		return "", err
	}
	if err := os.Remove(path); err != nil {
		return "", err
	}
	return path, nil
}

func cleanupStaged(operations []*swapOperation) {
	for _, operation := range operations {
		operation.cleanup()
	}
}

func commitSwaps(operations []*swapOperation) error {
	var result error
	for _, operation := range operations {
		result = errors.Join(result, operation.commit())
	}
	return result
}

func rollbackSwaps(operations []*swapOperation) error {
	var result error
	for index := len(operations) - 1; index >= 0; index-- {
		result = errors.Join(result, operations[index].rollback())
	}
	return result
}

func copyDirectoryIfExists(source, target string, fileMode os.FileMode) error {
	if _, err := os.Stat(source); errors.Is(err, os.ErrNotExist) {
		return nil
	} else if err != nil {
		return err
	}
	return copyDirectory(source, target, fileMode)
}

func copyDirectory(source, target string, fileMode os.FileMode) error {
	if err := os.MkdirAll(target, 0o700); err != nil {
		return err
	}
	return filepath.WalkDir(source, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		relative, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		destination := filepath.Join(target, relative)
		if entry.Type()&os.ModeSymlink != 0 {
			return fmt.Errorf("refuse symlink while copying %s", path)
		}
		if entry.IsDir() {
			return os.MkdirAll(destination, 0o700)
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		return state.WriteAtomic(destination, data, fileMode)
	})
}

func sameFile(left, right string) bool {
	leftInfo, leftErr := os.Stat(left)
	rightInfo, rightErr := os.Stat(right)
	if leftErr == nil && rightErr == nil {
		return os.SameFile(leftInfo, rightInfo)
	}
	if runtime.GOOS == "windows" {
		return strings.EqualFold(filepath.Clean(left), filepath.Clean(right))
	}
	return filepath.Clean(left) == filepath.Clean(right)
}
