//go:build !windows

package cli

import "github.com/tinymins/sempre/internal/layout"

func menuRequiresAdministrator(mode layout.Mode) bool {
	return mode == layout.System
}
