package bundle

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/tinymins/sempre/internal/state"
)

const (
	MetadataName  = ".sempre-bundle.json"
	SchemaVersion = 1
)

type Kind string

const (
	Release  Kind = "release"
	Snapshot Kind = "snapshot"
)

type Metadata struct {
	Schema int  `json:"schema"`
	Kind   Kind `json:"kind"`
}

func Write(root string, kind Kind) error {
	metadata := Metadata{Schema: SchemaVersion, Kind: kind}
	if err := metadata.Validate(); err != nil {
		return err
	}
	data, err := json.MarshalIndent(metadata, "", "  ")
	if err != nil {
		return err
	}
	return state.WriteAtomic(filepath.Join(root, MetadataName), append(data, '\n'), 0o600)
}

func Read(root string) (Metadata, error) {
	data, err := os.ReadFile(filepath.Join(root, MetadataName))
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return Metadata{}, err
		}
		return Metadata{}, fmt.Errorf("read bundle metadata: %w", err)
	}
	var metadata Metadata
	if err := json.Unmarshal(data, &metadata); err != nil {
		return Metadata{}, fmt.Errorf("decode bundle metadata: %w", err)
	}
	if err := metadata.Validate(); err != nil {
		return Metadata{}, err
	}
	return metadata, nil
}

func (metadata Metadata) Validate() error {
	if metadata.Schema != SchemaVersion {
		return fmt.Errorf("unsupported bundle metadata schema %d", metadata.Schema)
	}
	switch metadata.Kind {
	case Release, Snapshot:
		return nil
	default:
		return fmt.Errorf("unsupported bundle kind %q", metadata.Kind)
	}
}
