package cli

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/tinymins/sempre/internal/app"
	"github.com/tinymins/sempre/internal/state"
)

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
		change, err := command.manager.UseCore(ctx, arguments[1])
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
	return command.subscriptionCommand(ctx, arguments, options)
}

func (command *CLI) config(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) != 2 || arguments[0] != "import" {
		return usageError()
	}
	change, err := command.manager.ImportConfig(ctx, arguments[1])
	if err != nil {
		return err
	}
	command.printChange(change)
	return nil
}

func (command *CLI) service(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	switch arguments[0] {
	case "install":
		if len(arguments) != 1 {
			return usageError()
		}
		if err := command.installService(ctx, options); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Service installed, enabled, and started.")
	case "deploy":
		if len(arguments) != 2 {
			return usageError()
		}
		component, err := app.ParseDeployComponent(arguments[1])
		if err != nil {
			return err
		}
		if err := command.deployService(ctx, component, options); err != nil {
			return err
		}
		fmt.Fprintf(command.output, "System service %s deployment completed.\n", component)
	case "uninstall":
		if len(arguments) != 1 {
			return usageError()
		}
		if err := command.manager.UninstallService(ctx); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Service uninstalled. Sempre data was retained.")
	case "start":
		if len(arguments) != 1 {
			return usageError()
		}
		if err := command.manager.StartService(ctx); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Service started.")
	case "stop":
		if len(arguments) != 1 {
			return usageError()
		}
		if err := command.manager.StopService(ctx); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Service stopped.")
	case "restart":
		if len(arguments) != 1 {
			return usageError()
		}
		if err := command.manager.RestartService(ctx); err != nil {
			return err
		}
		fmt.Fprintln(command.output, "Service restarted.")
	case "status":
		if len(arguments) != 1 {
			return usageError()
		}
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

func (command *CLI) installService(ctx context.Context, options Options) error {
	err := command.manager.InstallService(ctx, options.Yes)
	var confirmation *app.ConfirmationRequired
	if !errors.As(err, &confirmation) {
		return err
	}
	if !command.confirmReplacement(confirmation.Summary) {
		return fmt.Errorf("service installation cancelled")
	}
	return command.manager.InstallService(ctx, true)
}

func (command *CLI) deployService(
	ctx context.Context,
	component app.DeployComponent,
	options Options,
) error {
	err := command.manager.DeployService(ctx, component, options.Yes)
	var confirmation *app.ConfirmationRequired
	if !errors.As(err, &confirmation) {
		return err
	}
	if !command.confirmReplacement(confirmation.Summary) {
		return fmt.Errorf("service deployment cancelled")
	}
	return command.manager.DeployService(ctx, component, true)
}

func (command *CLI) confirmReplacement(summary string) bool {
	fmt.Fprintln(command.output, summary)
	fmt.Fprint(command.output, "Replace this system data? [y/N]: ")
	line, err := command.input.ReadString('\n')
	if err != nil && !errors.Is(err, io.EOF) {
		return false
	}
	value := strings.TrimSpace(line)
	return strings.EqualFold(value, "y") || strings.EqualFold(value, "yes")
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
	document, err := command.manager.State()
	if err != nil {
		return err
	}
	if document.DesiredState == state.DesiredStopped {
		fmt.Fprintln(command.output, "Change saved; the managed core is stopped and the change will take effect the next time it starts.")
		return nil
	}
	if _, err := os.Stat(command.manager.Paths().DaemonControl); errors.Is(err, os.ErrNotExist) {
		fmt.Fprintln(command.output, "Change saved; it will take effect the next time the Sempre daemon starts.")
		return nil
	} else if err != nil {
		return err
	}
	if options.NoRestart {
		fmt.Fprintln(command.output, "Change saved; the running managed core was not restarted.")
		return nil
	}
	restart := options.Yes
	if !options.Yes {
		fmt.Fprint(command.output, "Restart the running managed core now? [y/N]: ")
		line, readErr := command.input.ReadString('\n')
		if readErr != nil && !errors.Is(readErr, io.EOF) {
			return readErr
		}
		restart = strings.EqualFold(strings.TrimSpace(line), "y") ||
			strings.EqualFold(strings.TrimSpace(line), "yes")
	}
	if !restart {
		fmt.Fprintln(command.output, "Change saved; run 'sempre runtime restart' when ready.")
		return nil
	}
	if err := command.runtimeLifecycle(ctx, []string{"restart"}, options); err != nil {
		return fmt.Errorf("change saved, but managed core restart failed: %w", err)
	}
	fmt.Fprintln(command.output, "Change applied and the managed core restarted successfully.")
	return nil
}
