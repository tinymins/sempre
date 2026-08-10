package singbox

import (
	"strconv"
	"strings"
)

type PlatformPolicy struct {
	LegacySniffOverride bool
	TUNDNSMode          string
	TUNStack            string
}

func ResolveCompilerVersion(coreVersion string) (string, []string) {
	version := "13"
	parts := strings.Split(strings.TrimPrefix(coreVersion, "v"), ".")
	if len(parts) < 2 {
		return version, []string{"unrecognized sing-box version; using the default v13 compiler"}
	}
	major, majorErr := strconv.Atoi(parts[0])
	minor, minorErr := strconv.Atoi(parts[1])
	if majorErr != nil || minorErr != nil || major != 1 {
		return version, []string{"unknown sing-box major version; using the default v13 compiler"}
	}
	switch {
	case minor < 11:
		return "11", []string{"installed sing-box is older than the minimum compiler target; using v11"}
	case minor <= 14:
		return strconv.Itoa(minor), nil
	default:
		return "14", []string{"no exact compiler for this sing-box minor version; using the newest compatible v14 compiler"}
	}
}

func ResolvePlatformPolicy(version, platform string) PlatformPolicy {
	policy := PlatformPolicy{}
	if platform != "macos" {
		return policy
	}
	switch version {
	case "11", "12":
		policy.LegacySniffOverride = true
		policy.TUNStack = "gvisor"
	case "13":
		policy.TUNStack = "gvisor"
	case "14":
		policy.TUNDNSMode = "hijack"
	}
	return policy
}
