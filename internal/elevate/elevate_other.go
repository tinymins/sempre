//go:build !windows

package elevate

func Ensure(arguments []string) (bool, int, error) {
	return false, 0, nil
}
