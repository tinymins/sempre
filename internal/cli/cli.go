package cli

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"

	"github.com/tinymins/sempre/internal/app"
	"github.com/tinymins/sempre/internal/buildinfo"
	"github.com/tinymins/sempre/internal/elevate"
	"github.com/tinymins/sempre/internal/layout"
)

type Options struct {
	Yes       bool
	NoRestart bool
	JSON      bool
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
	if len(arguments) == 0 {
		return runLauncher(ctx, input, output, errorOutput)
	}
	if err := validateCommandOptions(arguments, options); err != nil {
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
	case "install":
		return command.install(ctx, arguments[1:], options)
	case "bundle":
		return command.bundle(ctx, arguments[1:], options)
	case "uninstall":
		return command.uninstall(ctx, arguments[1:], options)
	case "web":
		return command.web(ctx, arguments[1:], options)
	case "ui":
		return command.ui(ctx, arguments[1:], options)
	case "runtime":
		return command.runtime(ctx, arguments[1:], options)
	case "core":
		return command.core(ctx, arguments[1:], options)
	case "subscription":
		return command.subscription(ctx, arguments[1:], options)
	case "custom-node":
		return command.customNode(ctx, arguments[1:], options)
	case "config":
		return command.config(ctx, arguments[1:], options)
	case "service":
		return command.service(ctx, arguments[1:], options)
	case "portable":
		if len(arguments) == 2 && arguments[1] == "run" {
			return command.runPortable(ctx)
		}
		return fmt.Errorf("portable accepts run, enable, or disable")
	case "run":
		return command.run(ctx, arguments[1:])
	case "update":
		if len(arguments) != 1 {
			return usageError()
		}
		change, err := command.manager.UpdateSubscription(ctx)
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
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

func (command *CLI) bundle(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	switch arguments[0] {
	case "export":
		if len(arguments) != 2 {
			return usageError()
		}
		result, err := command.manager.ExportBundle(ctx, arguments[1])
		if err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Bundle directory:", result.Directory)
		fmt.Fprintln(command.output, "Bundle archive:", result.Archive)
		return nil
	case "install":
		if len(arguments) != 1 {
			return usageError()
		}
		fmt.Fprintln(command.output, "WARNING: 'bundle install' is deprecated; performing a configuration-preserving install.")
		if options.Yes {
			fmt.Fprintln(command.output, "WARNING: ignoring --yes so an installed custom UI cannot be replaced without confirmation.")
			options.Yes = false
		}
		return command.install(ctx, nil, options)
	case "restore":
		if len(arguments) != 1 {
			return usageError()
		}
		if err := command.restoreBundle(ctx, options); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Sempre snapshot restored, enabled, and started.")
		return waitAndOpenSystem(ctx, command.output)
	default:
		return usageError()
	}
}

func (command *CLI) restoreBundle(ctx context.Context, options Options) error {
	err := command.manager.RestoreBundleApplication(ctx, options.Yes)
	var confirmation *app.ConfirmationRequired
	if !errors.As(err, &confirmation) {
		return err
	}
	if !command.confirmReplacement(confirmation.Summary) {
		return fmt.Errorf("bundle restore cancelled")
	}
	return command.manager.RestoreBundleApplication(ctx, true)
}
