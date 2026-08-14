package main

import (
	"archive/zip"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"github.com/tinymins/sempre/internal/installscript"
)

func writeReleaseInstallers(packageDir, executableName, goos string) error {
	return installscript.Write(packageDir, executableName, goos, "install", "install")
}

func zipDirectory(destination, source string) error {
	archive, err := os.Create(destination)
	if err != nil {
		return err
	}
	writer := zip.NewWriter(archive)
	err = filepath.WalkDir(source, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() {
			return nil
		}
		relative, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		return addFileToZIP(writer, path, filepath.ToSlash(relative), 0o600)
	})
	return errors.Join(err, writer.Close(), archive.Close())
}

func zipDirectoryWithPrefix(destination, source, prefix string) error {
	archive, err := os.Create(destination)
	if err != nil {
		return err
	}
	writer := zip.NewWriter(archive)
	closeWithError := func(cause error) error {
		return errors.Join(cause, writer.Close(), archive.Close())
	}
	err = filepath.WalkDir(source, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if path == source {
			return nil
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return fmt.Errorf("refuse symlink while archiving %s", path)
		}
		relative, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		name := filepath.ToSlash(filepath.Join(prefix, relative))
		if entry.IsDir() {
			header := &zip.FileHeader{Name: name + "/", Method: zip.Store}
			header.SetMode(0o700 | os.ModeDir)
			_, err := writer.CreateHeader(header)
			return err
		}
		return addFileToZIP(writer, path, name, info.Mode())
	})
	if err != nil {
		return closeWithError(err)
	}
	return closeWithError(nil)
}

func addFileToZIP(writer *zip.Writer, source, name string, mode os.FileMode) error {
	file, err := os.Open(source)
	if err != nil {
		return err
	}
	defer file.Close()
	header := &zip.FileHeader{Name: name, Method: zip.Deflate}
	header.SetMode(mode)
	destination, err := writer.CreateHeader(header)
	if err != nil {
		return err
	}
	_, err = io.Copy(destination, file)
	return err
}

func copyDirectory(source, target string, mode os.FileMode) error {
	return filepath.WalkDir(source, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		relative, err := filepath.Rel(source, path)
		if err != nil {
			return err
		}
		if relative == "." {
			return os.MkdirAll(target, 0o755)
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return fmt.Errorf("refuse symlink while copying %s", path)
		}
		destination := filepath.Join(target, relative)
		if entry.IsDir() {
			return os.MkdirAll(destination, 0o755)
		}
		return copyFile(path, destination, mode)
	})
}

func copyFile(source, target string, mode os.FileMode) error {
	sourceFile, err := os.Open(source)
	if err != nil {
		return err
	}
	defer sourceFile.Close()
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		return err
	}
	targetFile, err := os.OpenFile(target, os.O_CREATE|os.O_EXCL|os.O_WRONLY, mode)
	if err != nil {
		return err
	}
	_, copyErr := io.Copy(targetFile, sourceFile)
	closeErr := targetFile.Close()
	return errors.Join(copyErr, closeErr)
}
