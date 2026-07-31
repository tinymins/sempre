//go:build windows

package layout

import (
	"path/filepath"

	"golang.org/x/sys/windows"
)

func systemLayout() (Layout, error) {
	programData, err := windows.KnownFolderPath(windows.FOLDERID_ProgramData, windows.KF_FLAG_DEFAULT)
	if err != nil {
		return Layout{}, err
	}
	programFiles, err := windows.KnownFolderPath(windows.FOLDERID_ProgramFiles, windows.KF_FLAG_DEFAULT)
	if err != nil {
		return Layout{}, err
	}
	root := filepath.Join(programFiles, "Sempre")
	home := filepath.Join(programData, "Sempre")
	return newLayout(
		System,
		root,
		home,
		filepath.Join(home, "logs"),
		filepath.Join(home, "run"),
		filepath.Join(root, "sempre.exe"),
	), nil
}
