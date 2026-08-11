package cli

import (
	"context"
	"fmt"
	"io"

	"github.com/tinymins/sempre/internal/buildinfo"
	"github.com/tinymins/sempre/internal/layout"
	"github.com/tinymins/sempre/internal/service"
)

func parseGlobalOptions(arguments []string) ([]string, Options, error) {
	options := Options{}
	result := make([]string, 0, len(arguments))
	for _, argument := range arguments {
		switch argument {
		case "--yes":
			options.Yes = true
		case "--no-restart":
			options.NoRestart = true
		case "--json":
			options.JSON = true
		case "--elevated":
			options.Elevated = true
		case "--system":
			if options.Mode == layout.Portable {
				return nil, Options{}, fmt.Errorf("--system and --portable cannot be used together")
			}
			options.Mode = layout.System
		case "--portable":
			if options.Mode == layout.System {
				return nil, Options{}, fmt.Errorf("--system and --portable cannot be used together")
			}
			options.Mode = layout.Portable
		default:
			result = append(result, argument)
		}
	}
	if options.Yes && options.NoRestart {
		return nil, Options{}, fmt.Errorf("--yes and --no-restart cannot be used together")
	}
	return result, options, nil
}

func invocationArguments(arguments []string, options Options) []string {
	result := []string{"--" + string(options.Mode)}
	result = append(result, arguments...)
	if options.Yes {
		result = append(result, "--yes")
	}
	if options.NoRestart {
		result = append(result, "--no-restart")
	}
	if options.JSON {
		result = append(result, "--json")
	}
	if options.Elevated {
		result = append(result, "--elevated")
	}
	return result
}

func requiresAdministrator(arguments []string, mode layout.Mode) bool {
	if len(arguments) == 0 {
		return false
	}
	switch arguments[0] {
	case "help", "-h", "--help", "version", "open":
		return false
	case "install":
		return true
	case "portable":
		return len(arguments) == 2 && arguments[1] == "run"
	case "service":
		return len(arguments) < 2 || arguments[1] != "status"
	case "bundle":
		if len(arguments) >= 2 && arguments[1] == "export" {
			return mode == layout.System
		}
		return true
	case "run":
		return true
	default:
		return mode == layout.System
	}
}

func runStateless(
	ctx context.Context,
	arguments []string,
	executable string,
	output, errorOutput io.Writer,
) (bool, int) {
	if len(arguments) == 1 {
		switch arguments[0] {
		case "help", "-h", "--help":
			fmt.Fprint(output, usage)
			return true, 0
		case "version":
			fmt.Fprintf(output, "Sempre %s (%s, %s)\n", buildinfo.Version, buildinfo.Commit, buildinfo.Date)
			return true, 0
		case "open":
			if err := openSystemUI(ctx, output); err != nil {
				fmt.Fprintln(errorOutput, "ERROR:", err)
				return true, 1
			}
			return true, 0
		}
	}
	if len(arguments) == 2 && arguments[0] == "portable" {
		var enabled bool
		switch arguments[1] {
		case "enable":
			enabled = true
		case "disable":
			enabled = false
		default:
			return false, 0
		}
		if err := layout.SetPortableMarker(executable, enabled); err != nil {
			fmt.Fprintln(errorOutput, "ERROR:", err)
			return true, 1
		}
		state := "disabled"
		if enabled {
			state = "enabled"
		}
		fmt.Fprintf(output, "Portable marker %s at %s.\n", state, layout.PortableMarkerPath(executable))
		return true, 0
	}
	if len(arguments) == 2 && arguments[0] == "service" && arguments[1] == "status" {
		current, err := service.New().Status(ctx)
		if err != nil {
			fmt.Fprintln(errorOutput, "ERROR:", err)
			return true, 1
		}
		fmt.Fprintln(output, current)
		return true, 0
	}
	return false, 0
}

func usageError() error {
	return fmt.Errorf("invalid command; run 'sempre help' for usage")
}
