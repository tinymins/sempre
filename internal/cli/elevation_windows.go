//go:build windows

package cli

import "github.com/tinymins/sempre/internal/layout"

func menuRequiresAdministrator(layout.Mode) bool {
	return true
}
