//go:build windows

package cli

import "github.com/sempre-lab/sempre/internal/layout"

func menuRequiresAdministrator(layout.Mode) bool {
	return true
}
