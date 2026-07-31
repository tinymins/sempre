//go:build windows

package app

import (
	"fmt"
	"strings"

	"golang.org/x/sys/windows"
)

func checkProtectedPath(path string) error {
	descriptor, err := windows.GetNamedSecurityInfo(
		path,
		windows.SE_FILE_OBJECT,
		windows.DACL_SECURITY_INFORMATION,
	)
	if err != nil {
		return err
	}
	sddl := descriptor.String()
	for _, trustee := range []string{";;;BU)", ";;;WD)", ";;;AU)", ";;;OW)"} {
		if strings.Contains(sddl, trustee) {
			return fmt.Errorf("DACL grants access to an unprivileged trustee")
		}
	}
	if !strings.Contains(sddl, ";;;SY)") || !strings.Contains(sddl, ";;;BA)") {
		return fmt.Errorf("DACL does not grant SYSTEM and Administrators access")
	}
	return nil
}
