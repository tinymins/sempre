package state

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/sempre-lab/sempre/internal/fileowner"
	"github.com/sempre-lab/sempre/internal/layout"
)

type Store struct {
	paths layout.Layout
}

type Lease struct {
	file *os.File
}

func New(paths layout.Layout) *Store {
	return &Store{paths: paths}
}

func (store *Store) Paths() layout.Layout {
	return store.paths
}

func (store *Store) Initialize() error {
	if err := store.paths.Ensure(); err != nil {
		return err
	}
	if _, err := os.Stat(store.paths.State); err == nil {
		return nil
	} else if !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("inspect state: %w", err)
	}
	return store.Update(func(document *Document) error { return nil })
}

func (store *Store) AcquireInstance() (*Lease, error) {
	if err := store.paths.EnsureInstanceLockDirectory(); err != nil {
		return nil, fmt.Errorf("create instance lock directory: %w", err)
	}
	file, err := os.OpenFile(store.paths.InstanceLock, os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return nil, fmt.Errorf("open instance lock: %w", err)
	}
	if err := fileowner.MatchParent(store.paths.InstanceLock); err != nil {
		file.Close()
		return nil, fmt.Errorf("secure instance lock: %w", err)
	}
	if err := tryLockFile(file); err != nil {
		file.Close()
		return nil, fmt.Errorf("another Sempre-managed core is already running")
	}
	return &Lease{file: file}, nil
}

func (store *Store) AcquireConfig() (*Lease, error) {
	file, err := os.OpenFile(store.paths.ConfigLock, os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return nil, fmt.Errorf("open configuration lock: %w", err)
	}
	if err := fileowner.MatchParent(store.paths.ConfigLock); err != nil {
		file.Close()
		return nil, fmt.Errorf("secure configuration lock: %w", err)
	}
	if err := lockFile(file); err != nil {
		file.Close()
		return nil, fmt.Errorf("lock configuration: %w", err)
	}
	return &Lease{file: file}, nil
}

func (store *Store) InstanceRunning() (bool, error) {
	file, err := os.OpenFile(store.paths.InstanceLock, os.O_CREATE|os.O_RDWR, 0o600)
	if errors.Is(err, os.ErrNotExist) {
		return false, nil
	}
	if err != nil {
		return false, fmt.Errorf("open instance lock: %w", err)
	}
	defer file.Close()
	if err := fileowner.MatchParent(store.paths.InstanceLock); err != nil {
		return false, fmt.Errorf("secure instance lock: %w", err)
	}
	if err := tryLockFile(file); err != nil {
		return true, nil
	}
	unlockFile(file)
	return false, nil
}

func (lease *Lease) Release() {
	if lease == nil || lease.file == nil {
		return
	}
	unlockFile(lease.file)
	_ = lease.file.Close()
	lease.file = nil
}

func (store *Store) Read() (Document, error) {
	var result Document
	err := store.withLock(func() error {
		document, err := store.readUnlocked()
		if err != nil {
			return err
		}
		result = document
		return nil
	})
	return result, err
}

func (store *Store) Update(change func(*Document) error) error {
	if err := store.paths.Ensure(); err != nil {
		return err
	}
	return store.withLock(func() error {
		document, err := store.readUnlocked()
		if err != nil {
			return err
		}
		if err := change(&document); err != nil {
			return err
		}
		document.Normalize()
		document.UpdatedAt = time.Now().UTC()
		return writeJSONAtomic(store.paths.State, document, 0o600)
	})
}

func (store *Store) readUnlocked() (Document, error) {
	data, err := os.ReadFile(store.paths.State)
	if errors.Is(err, os.ErrNotExist) {
		return NewDocument(), nil
	}
	if err != nil {
		return Document{}, fmt.Errorf("read state: %w", err)
	}
	var document Document
	if err := json.Unmarshal(data, &document); err != nil {
		return Document{}, fmt.Errorf("decode state: %w", err)
	}
	if document.Schema < 0 || document.Schema > SchemaVersion {
		return Document{}, fmt.Errorf("unsupported state schema %d", document.Schema)
	}
	document.Normalize()
	return document, nil
}

func (store *Store) withLock(action func() error) error {
	file, err := os.OpenFile(store.paths.Lock, os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return fmt.Errorf("open state lock: %w", err)
	}
	defer file.Close()
	if err := fileowner.MatchParent(store.paths.Lock); err != nil {
		return fmt.Errorf("secure state lock: %w", err)
	}
	if err := lockFile(file); err != nil {
		return fmt.Errorf("lock state: %w", err)
	}
	defer unlockFile(file)
	return action()
}

func writeJSONAtomic(path string, value any, mode os.FileMode) error {
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return fmt.Errorf("encode %s: %w", path, err)
	}
	data = append(data, '\n')
	return WriteAtomic(path, data, mode)
}

func WriteAtomic(path string, data []byte, mode os.FileMode) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return fmt.Errorf("create parent for %s: %w", path, err)
	}
	file, err := os.CreateTemp(filepath.Dir(path), ".sempre-*")
	if err != nil {
		return fmt.Errorf("create temporary file: %w", err)
	}
	temporary := file.Name()
	defer os.Remove(temporary)
	if err := file.Chmod(mode); err != nil {
		file.Close()
		return err
	}
	if err := fileowner.MatchParent(temporary); err != nil {
		file.Close()
		return fmt.Errorf("preserve file ownership: %w", err)
	}
	if _, err := file.Write(data); err != nil {
		file.Close()
		return err
	}
	if err := file.Sync(); err != nil {
		file.Close()
		return err
	}
	if err := file.Close(); err != nil {
		return err
	}

	backup := path + ".previous"
	_ = os.Remove(backup)
	if _, err := os.Stat(path); err == nil {
		if err := os.Rename(path, backup); err != nil {
			return fmt.Errorf("back up %s: %w", path, err)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if err := os.Rename(temporary, path); err != nil {
		_ = os.Rename(backup, path)
		return fmt.Errorf("replace %s: %w", path, err)
	}
	_ = os.Remove(backup)
	return nil
}
