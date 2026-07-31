//go:build windows

package layout

import "golang.org/x/sys/windows"

func secureDirectory(path string, mode Mode) error {
	sddl := "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
	if mode == Portable {
		sddl += "(A;OICI;FA;;;OW)"
	}
	descriptor, err := windows.SecurityDescriptorFromString(sddl)
	if err != nil {
		return err
	}
	acl, _, err := descriptor.DACL()
	if err != nil {
		return err
	}
	return windows.SetNamedSecurityInfo(
		path,
		windows.SE_FILE_OBJECT,
		windows.DACL_SECURITY_INFORMATION|windows.PROTECTED_DACL_SECURITY_INFORMATION,
		nil,
		nil,
		acl,
		nil,
	)
}

func secureExecutableDirectory(path string) error {
	return secureDirectory(path, System)
}
