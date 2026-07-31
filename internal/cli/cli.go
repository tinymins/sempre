package cli

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"strings"

	"github.com/sempre-lab/sempre/internal/app"
	"github.com/sempre-lab/sempre/internal/buildinfo"
	"github.com/sempre-lab/sempre/internal/elevate"
	"github.com/sempre-lab/sempre/internal/layout"
	"github.com/sempre-lab/sempre/internal/service"
)

type Options struct {
	Yes       bool
	NoRestart bool
	Mode      layout.Mode
	Elevated  bool
}

type CLI struct {
	manager *app.Manager
	input   *bufio.Reader
	output  io.Writer
	errors  io.Writer
}

func Run(ctx context.Context, arguments []string, input io.Reader, output, errorOutput io.Writer) int {
	arguments, options, err := parseGlobalOptions(arguments)
	if err != nil {
		fmt.Fprintln(errorOutput, "ERROR:", err)
		return 1
	}
	executable, err := layout.CurrentExecutable()
	if err != nil {
		fmt.Fprintln(errorOutput, "ERROR:", err)
		return 1
	}
	options.Mode, err = resolveMode(options.Mode, executable)
	if err != nil {
		fmt.Fprintln(errorOutput, "ERROR:", err)
		return 1
	}
	if handled, code := runStateless(ctx, arguments, executable, output, errorOutput); handled {
		return code
	}
	elevatedArguments := invocationArguments(arguments, options)
	handled, code, err := elevate.Ensure(elevatedArguments, requiresAdministrator(arguments, options.Mode))
	if err != nil {
		fmt.Fprintln(errorOutput, "ERROR:", err)
		return 1
	}
	if handled {
		if code != 0 {
			fmt.Fprintf(errorOutput, "ERROR: elevated command exited with code %d\n", code)
			return 1
		}
		return 0
	}
	paths, err := layout.ForMode(options.Mode)
	if err != nil {
		fmt.Fprintln(errorOutput, "ERROR:", err)
		return 1
	}
	manager, err := app.New(paths, output, errorOutput)
	if err != nil {
		fmt.Fprintln(errorOutput, "ERROR:", err)
		return 1
	}
	command := &CLI{
		manager: manager,
		input:   bufio.NewReader(input),
		output:  output,
		errors:  errorOutput,
	}
	if len(arguments) == 0 {
		return command.menu(ctx)
	}
	if err := command.execute(ctx, arguments, options); err != nil {
		fmt.Fprintln(errorOutput, "ERROR:", err)
		return 1
	}
	return 0
}

func resolveMode(requested layout.Mode, executable string) (layout.Mode, error) {
	if requested != "" {
		return requested, nil
	}
	portable, err := layout.PortableMarkerEnabled(executable)
	if err != nil {
		return "", err
	}
	if portable {
		return layout.Portable, nil
	}
	return layout.System, nil
}

func (command *CLI) execute(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	switch arguments[0] {
	case "help", "-h", "--help":
		fmt.Fprint(command.output, usage)
		return nil
	case "version":
		if len(arguments) != 1 {
			return usageError()
		}
		fmt.Fprintf(command.output, "Sempre %s (%s, %s)\n", buildinfo.Version, buildinfo.Commit, buildinfo.Date)
		return nil
	case "daemon":
		if len(arguments) != 1 {
			return usageError()
		}
		return command.manager.RunDaemon(ctx)
	case "core":
		return command.core(ctx, arguments[1:], options)
	case "subscription":
		return command.subscription(ctx, arguments[1:], options)
	case "config":
		return command.config(ctx, arguments[1:], options)
	case "service":
		return command.service(ctx, arguments[1:])
	case "portable":
		return fmt.Errorf("portable mode is changed before data initialization; run 'sempre portable enable|disable'")
	case "run":
		return command.run(ctx, arguments[1:])
	case "update":
		if len(arguments) != 1 {
			return usageError()
		}
		change, err := command.manager.UpdateSubscription(ctx)
		return command.finishChange(ctx, change, options, err)
	case "status":
		if len(arguments) != 1 {
			return usageError()
		}
		status, err := command.manager.Status(ctx)
		if err == nil {
			fmt.Fprintln(command.output, status)
		}
		return err
	case "logs":
		follow := len(arguments) == 2 && arguments[1] == "--follow"
		if len(arguments) > 2 || (len(arguments) == 2 && !follow) {
			return usageError()
		}
		paths := command.manager.Paths()
		return app.FollowLogs(ctx, command.output, []string{paths.ManagerLog, paths.StdoutLog, paths.StderrLog}, follow)
	case "doctor":
		if len(arguments) != 1 {
			return usageError()
		}
		report, err := command.manager.Doctor(ctx)
		if report != "" {
			fmt.Fprintln(command.output, report)
		}
		return err
	default:
		return usageError()
	}
}

func (command *CLI) core(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	switch arguments[0] {
	case "list":
		if len(arguments) > 2 {
			return usageError()
		}
		filter := ""
		if len(arguments) == 2 {
			filter = arguments[1]
		}
		output, err := command.manager.ListCores(filter)
		if err == nil {
			fmt.Fprintln(command.output, output)
		}
		return err
	case "current":
		if len(arguments) != 1 {
			return usageError()
		}
		output, err := command.manager.CurrentCore()
		if err == nil {
			fmt.Fprintln(command.output, output)
		}
		return err
	case "install":
		if len(arguments) != 2 {
			return usageError()
		}
		change, err := command.manager.InstallCore(ctx, arguments[1])
		return command.finishChange(ctx, change, options, err)
	case "update":
		if len(arguments) > 2 {
			return usageError()
		}
		value := ""
		if len(arguments) == 2 {
			value = arguments[1]
		}
		changes, err := command.manager.UpdateCores(ctx, value)
		if err != nil {
			return err
		}
		combined := app.Change{}
		for _, change := range changes {
			command.printChange(change)
			combined.Changed = combined.Changed || change.Changed
			combined.NeedsRestart = combined.NeedsRestart || change.NeedsRestart
		}
		return command.applyRestart(ctx, combined, options)
	case "use":
		if len(arguments) != 2 {
			return usageError()
		}
		change, err := command.manager.UseCore(arguments[1])
		return command.finishChange(ctx, change, options, err)
	case "remove":
		if len(arguments) != 2 {
			return usageError()
		}
		change, err := command.manager.RemoveCore(arguments[1])
		return command.finishChange(ctx, change, options, err)
	default:
		return usageError()
	}
}

func (command *CLI) subscription(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	switch arguments[0] {
	case "set":
		if len(arguments) != 2 {
			return usageError()
		}
		change, err := command.manager.SetSubscription(ctx, arguments[1])
		return command.finishChange(ctx, change, options, err)
	case "update":
		if len(arguments) != 1 {
			return usageError()
		}
		change, err := command.manager.UpdateSubscription(ctx)
		return command.finishChange(ctx, change, options, err)
	case "schedule":
		if len(arguments) != 2 {
			return usageError()
		}
		change, err := command.manager.SetSubscriptionSchedule(arguments[1])
		return command.finishChange(ctx, change, options, err)
	case "status":
		if len(arguments) != 1 {
			return usageError()
		}
		output, err := command.manager.SubscriptionStatus()
		if err == nil {
			fmt.Fprintln(command.output, output)
		}
		return err
	default:
		return usageError()
	}
}

func (command *CLI) config(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) != 2 || arguments[0] != "import" {
		return usageError()
	}
	change, err := command.manager.ImportConfig(ctx, arguments[1])
	return command.finishChange(ctx, change, options, err)
}

func (command *CLI) service(ctx context.Context, arguments []string) error {
	if len(arguments) != 1 {
		return usageError()
	}
	switch arguments[0] {
	case "install":
		if err := command.manager.InstallService(ctx); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Service installed, enabled, and started.")
	case "uninstall":
		if err := command.manager.UninstallService(ctx); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Service uninstalled. Sempre data was retained.")
	case "start":
		if err := command.manager.StartService(ctx); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Service started.")
	case "stop":
		if err := command.manager.StopService(ctx); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Service stopped.")
	case "restart":
		if err := command.manager.RestartService(ctx); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Service restarted.")
	case "status":
		state, err := command.manager.ServiceState(ctx)
		if err != nil {
			return err
		}
		fmt.Fprintln(command.output, state)
	default:
		return usageError()
	}
	return nil
}

func (command *CLI) run(ctx context.Context, arguments []string) error {
	reference := ""
	if len(arguments) == 2 && arguments[0] == "--core" {
		reference = arguments[1]
	} else if len(arguments) != 0 {
		return usageError()
	}
	return command.manager.RunDirect(ctx, reference)
}

func (command *CLI) finishChange(ctx context.Context, change app.Change, options Options, err error) error {
	if err != nil {
		return err
	}
	command.printChange(change)
	return command.applyRestart(ctx, change, options)
}

func (command *CLI) printChange(change app.Change) {
	if change.Message != "" {
		fmt.Fprintln(command.output, change.Message+".")
	}
	if change.PreviousDetail != "" {
		fmt.Fprintln(command.output, "Previous:", change.PreviousDetail)
	}
	if change.CurrentDetail != "" {
		fmt.Fprintln(command.output, "Current:", change.CurrentDetail)
	}
}

func (command *CLI) applyRestart(ctx context.Context, change app.Change, options Options) error {
	if !change.Changed || !change.NeedsRestart {
		return nil
	}
	if command.manager.Paths().Mode != layout.System {
		fmt.Fprintln(command.output, "Change saved; portable deployments are applied the next time they run.")
		return nil
	}
	current, err := command.manager.ServiceState(ctx)
	if err != nil {
		fmt.Fprintln(command.output, "Change saved; service status is unavailable. Run 'sempre service restart' to apply it.")
		return nil
	}
	if current != service.Running {
		fmt.Fprintln(command.output, "Change saved; it will take effect the next time the service starts.")
		return nil
	}
	if options.NoRestart {
		fmt.Fprintln(command.output, "Change saved; the running service was not restarted.")
		return nil
	}
	restart := options.Yes
	if !options.Yes {
		fmt.Fprint(command.output, "Restart the running service now? [y/N]: ")
		line, readErr := command.input.ReadString('\n')
		if readErr != nil && !errors.Is(readErr, io.EOF) {
			return readErr
		}
		restart = strings.EqualFold(strings.TrimSpace(line), "y") ||
			strings.EqualFold(strings.TrimSpace(line), "yes")
	}
	if !restart {
		fmt.Fprintln(command.output, "Change saved; run 'sempre service restart' when ready.")
		return nil
	}
	if err := command.execute(ctx, []string{"service", "restart"}, options); err != nil {
		return fmt.Errorf("change saved, but service restart failed: %w", err)
	}
	fmt.Fprintln(command.output, "Change applied and service restarted successfully.")
	return nil
}

func parseGlobalOptions(arguments []string) ([]string, Options, error) {
	options := Options{}
	result := make([]string, 0, len(arguments))
	for _, argument := range arguments {
		switch argument {
		case "--yes":
			options.Yes = true
		case "--no-restart":
			options.NoRestart = true
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
	if options.Elevated {
		result = append(result, "--elevated")
	}
	return result
}

func requiresAdministrator(arguments []string, mode layout.Mode) bool {
	if len(arguments) == 0 {
		return mode == layout.System
	}
	switch arguments[0] {
	case "help", "-h", "--help", "version", "portable":
		return false
	case "service":
		return len(arguments) < 2 || arguments[1] != "status"
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
			fmt.Fprintln(errorOutput, "ERROR: portable accepts enable or disable")
			return true, 1
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

const usage = `Sempre - cross-platform lifecycle manager for proxy cores

Core versions:
  sempre core list
  sempre core install <core[@stable|@version]>
  sempre core update [core[@stable]]
  sempre core use <core@stable|core@version>
  sempre core remove <core@stable|core@version>
  sempre core current
  sempre run [--core core@stable|core@version]

Configuration:
  sempre subscription set <https-url>
  sempre subscription update
  sempre subscription schedule <duration|off>
  sempre subscription status
  sempre config import <file>
  sempre update

Service and diagnostics:
  sempre service <install|uninstall|start|stop|restart|status>
  sempre status
  sempre logs [--follow]
  sempre doctor
  sempre version

Modes:
  sempre --system <command>       Use protected machine-wide data (default)
  sempre --portable <command>     Use .sempre beside the executable
  sempre portable enable|disable Manage the .sempre-portable marker

Mutating commands accept --yes to restart a running service without prompting,
or --no-restart to save the change without restarting.
`
