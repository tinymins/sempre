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

	"github.com/sempre-lab/sempre/internal/state"
)

type swapOperation struct {
	staged    string
	target    string
	backup    string
	hadTarget bool
	active    bool
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
			for rollbackIndex := index - 1; rollbackIndex >= 0; rollbackIndex-- {
				_ = operations[rollbackIndex].rollback()
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
		if err := os.Rename(operation.target, backup); err != nil {
			return fmt.Errorf("back up %s: %w", operation.target, err)
		}
		operation.backup = backup
		operation.hadTarget = true
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if err := os.Rename(operation.staged, operation.target); err != nil {
		if operation.hadTarget {
			_ = os.Rename(operation.backup, operation.target)
		}
		return fmt.Errorf("activate %s: %w", operation.target, err)
	}
	operation.active = true
	return nil
}

func (operation *swapOperation) rollback() error {
	if !operation.active {
		return nil
	}
	if err := os.RemoveAll(operation.target); err != nil {
		return err
	}
	if operation.hadTarget {
		if err := os.Rename(operation.backup, operation.target); err != nil {
			return err
		}
	}
	operation.active = false
	return nil
}

func (operation *swapOperation) commit() {
	if operation.hadTarget {
		_ = os.RemoveAll(operation.backup)
		operation.backup = ""
	}
	operation.active = false
}

func (operation *swapOperation) cleanup() {
	if operation.staged != "" {
		_ = os.RemoveAll(operation.staged)
	}
	if !operation.active && operation.backup != "" {
		_ = os.RemoveAll(operation.backup)
	}
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

func commitSwaps(operations []*swapOperation) {
	for _, operation := range operations {
		operation.commit()
	}
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
