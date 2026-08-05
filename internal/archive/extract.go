package archive

import (
	"archive/tar"
	"archive/zip"
	"compress/gzip"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
)

const MaxExpandedSize = int64(2 << 30)

type ExtractOptions struct {
	Format         string
	SingleFileName string
}

func Extract(path, destination string, options ExtractOptions) error {
	if err := os.MkdirAll(destination, 0o700); err != nil {
		return err
	}
	switch options.Format {
	case "zip":
		return extractZIP(path, destination)
	case "tar.gz":
		return extractTarGZ(path, destination)
	case "gz":
		return extractGZ(path, destination, options.SingleFileName)
	default:
		return fmt.Errorf("unsupported archive format %q", options.Format)
	}
}

func extractGZ(path, destination, name string) error {
	return extractGZWithLimit(path, destination, name, MaxExpandedSize)
}

func extractGZWithLimit(path, destination, name string, limit int64) error {
	if name == "" || name != filepath.Base(name) || name == "." {
		return fmt.Errorf("single-file gzip output name must be a non-empty base name")
	}
	file, err := os.Open(path)
	if err != nil {
		return err
	}
	defer file.Close()
	reader, err := gzip.NewReader(file)
	if err != nil {
		return fmt.Errorf("open gzip: %w", err)
	}
	defer reader.Close()
	target, err := safeTarget(destination, name)
	if err != nil {
		return err
	}
	limited := io.LimitReader(reader, limit+1)
	if err := writeFile(target, limited, 0o600); err != nil {
		_ = os.Remove(target)
		return err
	}
	info, err := os.Stat(target)
	if err != nil {
		return err
	}
	if info.Size() > limit {
		_ = os.Remove(target)
		return fmt.Errorf("gzip archive expands beyond %d bytes", limit)
	}
	return nil
}

func Find(root, name string) (string, error) {
	var result string
	err := filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if !entry.IsDir() && strings.EqualFold(entry.Name(), name) {
			result = path
			return filepath.SkipAll
		}
		return nil
	})
	if err != nil {
		return "", err
	}
	if result == "" {
		return "", fmt.Errorf("archive does not contain %s", name)
	}
	return result, nil
}

func extractZIP(path, destination string) error {
	reader, err := zip.OpenReader(path)
	if err != nil {
		return fmt.Errorf("open ZIP: %w", err)
	}
	defer reader.Close()
	var expanded int64
	for _, entry := range reader.File {
		if entry.UncompressedSize64 > uint64(MaxExpandedSize) ||
			expanded > MaxExpandedSize-int64(entry.UncompressedSize64) {
			return fmt.Errorf("ZIP archive expands beyond %d bytes", MaxExpandedSize)
		}
		expanded += int64(entry.UncompressedSize64)
		target, err := safeTarget(destination, entry.Name)
		if err != nil {
			return err
		}
		if entry.FileInfo().IsDir() {
			if err := os.MkdirAll(target, 0o700); err != nil {
				return err
			}
			continue
		}
		if !entry.Mode().IsRegular() {
			continue
		}
		source, err := entry.Open()
		if err != nil {
			return err
		}
		writeErr := writeFile(target, source, entry.Mode())
		closeErr := source.Close()
		if writeErr != nil {
			return writeErr
		}
		if closeErr != nil {
			return closeErr
		}
	}
	return nil
}

func extractTarGZ(path, destination string) error {
	file, err := os.Open(path)
	if err != nil {
		return err
	}
	defer file.Close()
	gzipReader, err := gzip.NewReader(file)
	if err != nil {
		return fmt.Errorf("open gzip: %w", err)
	}
	defer gzipReader.Close()
	reader := tar.NewReader(gzipReader)
	var expanded int64
	for {
		header, err := reader.Next()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return fmt.Errorf("read tar: %w", err)
		}
		target, err := safeTarget(destination, header.Name)
		if err != nil {
			return err
		}
		switch header.Typeflag {
		case tar.TypeDir:
			if err := os.MkdirAll(target, 0o700); err != nil {
				return err
			}
		case tar.TypeReg, tar.TypeRegA:
			if header.Size < 0 || header.Size > MaxExpandedSize || expanded > MaxExpandedSize-header.Size {
				return fmt.Errorf("tar archive expands beyond %d bytes", MaxExpandedSize)
			}
			expanded += header.Size
			if err := writeFile(target, reader, os.FileMode(header.Mode)); err != nil {
				return err
			}
		}
	}
}

func safeTarget(destination, name string) (string, error) {
	converted := filepath.FromSlash(name)
	if filepath.IsAbs(converted) || filepath.VolumeName(converted) != "" {
		return "", fmt.Errorf("archive entry is absolute: %q", name)
	}
	target := filepath.Join(destination, converted)
	relative, err := filepath.Rel(destination, target)
	if err != nil || relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("archive entry escapes extraction directory: %q", name)
	}
	return target, nil
}

func writeFile(path string, source io.Reader, mode os.FileMode) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	permissions := mode.Perm()
	if permissions == 0 {
		permissions = 0o600
	}
	file, err := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_WRONLY, permissions)
	if err != nil {
		return err
	}
	_, copyErr := io.Copy(file, source)
	closeErr := file.Close()
	if copyErr != nil {
		return copyErr
	}
	return closeErr
}
