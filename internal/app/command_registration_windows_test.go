//go:build windows

package app

import "testing"

func TestAddWindowsPathEntry(t *testing.T) {
	t.Parallel()
	const root = `C:\Program Files\Sempre`
	value, changed := addWindowsPathEntry(`C:\Windows;%SystemRoot%\System32`, root)
	if !changed || value != `C:\Windows;%SystemRoot%\System32;C:\Program Files\Sempre` {
		t.Fatalf("added PATH = %q, %v", value, changed)
	}
	value, changed = addWindowsPathEntry(value, `c:\program files\sempre\`)
	if changed || value != `C:\Windows;%SystemRoot%\System32;C:\Program Files\Sempre` {
		t.Fatalf("idempotent PATH = %q, %v", value, changed)
	}
}

func TestRemoveWindowsPathEntry(t *testing.T) {
	t.Parallel()
	value, changed := removeWindowsPathEntry(
		`C:\Other;C:\Program Files\Sempre;C:\Program Files\Sempre Tools`,
		`c:\program files\sempre\`,
	)
	if !changed || value != `C:\Other;C:\Program Files\Sempre Tools` {
		t.Fatalf("removed PATH = %q, %v", value, changed)
	}
}
