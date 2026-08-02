package supervisor

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"sync"

	"github.com/tinymins/sempre/internal/fileowner"
)

type RollingWriter struct {
	path    string
	maxSize int64
	backups int
	mu      sync.Mutex
}

func NewRollingWriter(path string, maxSize int64, backups int) *RollingWriter {
	return &RollingWriter{path: path, maxSize: maxSize, backups: backups}
}

func (writer *RollingWriter) Write(data []byte) (int, error) {
	writer.mu.Lock()
	defer writer.mu.Unlock()
	if err := os.MkdirAll(filepath.Dir(writer.path), 0o700); err != nil {
		return 0, err
	}
	if info, err := os.Stat(writer.path); err == nil && info.Size()+int64(len(data)) > writer.maxSize {
		if err := writer.rotate(); err != nil {
			return 0, err
		}
	}
	_, statErr := os.Stat(writer.path)
	created := os.IsNotExist(statErr)
	file, err := os.OpenFile(writer.path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return 0, err
	}
	if created {
		if err := fileowner.MatchParent(writer.path); err != nil {
			file.Close()
			return 0, err
		}
	}
	written, writeErr := file.Write(data)
	closeErr := file.Close()
	if writeErr != nil {
		return written, writeErr
	}
	return written, closeErr
}

func (writer *RollingWriter) rotate() error {
	if writer.backups < 1 {
		return os.Remove(writer.path)
	}
	_ = os.Remove(writer.path + "." + strconv.Itoa(writer.backups))
	for index := writer.backups - 1; index >= 1; index-- {
		source := writer.path + "." + strconv.Itoa(index)
		target := writer.path + "." + strconv.Itoa(index+1)
		if err := os.Rename(source, target); err != nil && !os.IsNotExist(err) {
			return fmt.Errorf("rotate %s: %w", source, err)
		}
	}
	if err := os.Rename(writer.path, writer.path+".1"); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("rotate %s: %w", writer.path, err)
	}
	return nil
}
