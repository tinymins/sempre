package cli

import (
	"context"
	"errors"
	"fmt"
	"io"
	"strings"

	"github.com/tinymins/sempre/internal/elevate"
	"github.com/tinymins/sempre/internal/layout"
)

func (command *CLI) menu(ctx context.Context, options Options) int {
	for {
		fmt.Fprintln(command.output, "\nSempre")
		fmt.Fprintf(command.output, "  Mode: %s\n", command.manager.Paths().Mode)
		fmt.Fprintln(command.output, "\nOverview")
		fmt.Fprintln(command.output, "  1. Status")
		fmt.Fprintln(command.output, "\nCore")
		fmt.Fprintln(command.output, "  2. List installed versions")
		fmt.Fprintln(command.output, "  3. Show selected and active core")
		fmt.Fprintln(command.output, "  4. Install latest stable sing-box")
		fmt.Fprintln(command.output, "  5. Install an exact sing-box version")
		fmt.Fprintln(command.output, "  6. Update installed core channels")
		fmt.Fprintln(command.output, "  7. Select a core version")
		fmt.Fprintln(command.output, "  8. Remove a core version")
		fmt.Fprintln(command.output, "  9. Run selected core in foreground")
		fmt.Fprintln(command.output, "\nConfiguration and subscription")
		fmt.Fprintln(command.output, " 10. Show subscription status")
		fmt.Fprintln(command.output, " 11. Set subscription URL")
		fmt.Fprintln(command.output, " 12. Clear subscription URL")
		fmt.Fprintln(command.output, " 13. Update subscription now")
		fmt.Fprintln(command.output, " 14. Configure subscription schedule")
		fmt.Fprintln(command.output, " 15. Import local configuration")
		fmt.Fprintln(command.output, "\nSystem service")
		fmt.Fprintln(command.output, " 16. Show service status")
		fmt.Fprintln(command.output, " 17. Install or repair service")
		fmt.Fprintln(command.output, " 18. Deploy all portable assets")
		fmt.Fprintln(command.output, " 19. Deploy portable cores")
		fmt.Fprintln(command.output, " 20. Deploy Sempre binary")
		fmt.Fprintln(command.output, " 21. Deploy portable data")
		fmt.Fprintln(command.output, " 22. Start service")
		fmt.Fprintln(command.output, " 23. Stop service")
		fmt.Fprintln(command.output, " 24. Restart service")
		fmt.Fprintln(command.output, " 25. Uninstall service")
		fmt.Fprintln(command.output, "\nDiagnostics")
		fmt.Fprintln(command.output, " 26. Show logs")
		fmt.Fprintln(command.output, " 27. Follow logs")
		fmt.Fprintln(command.output, " 28. Doctor")
		fmt.Fprintln(command.output, "\nMode")
		fmt.Fprintln(command.output, " 29. Enable portable mode")
		fmt.Fprintln(command.output, " 30. Disable portable mode")
		fmt.Fprintln(command.output, "  0. Exit")
		fmt.Fprint(command.output, "\nSelect [0-30]: ")
		line, err := command.input.ReadString('\n')
		if err != nil && !errors.Is(err, io.EOF) {
			fmt.Fprintln(command.errors, "ERROR:", err)
			return 1
		}
		choice := strings.TrimSpace(line)
		if choice == "0" || (choice == "" && errors.Is(err, io.EOF)) {
			return 0
		}
		if choice == "29" || choice == "30" {
			if err := command.changePortableMode(choice == "29"); err != nil {
				fmt.Fprintln(command.errors, "ERROR:", err)
				return 1
			}
			return 0
		}
		arguments := command.menuArguments(choice)
		if len(arguments) == 0 {
			fmt.Fprintln(command.errors, "Invalid selection.")
			continue
		}
		if err := command.executeMenuAction(ctx, arguments, options); err != nil {
			fmt.Fprintln(command.errors, "ERROR:", err)
		}
		fmt.Fprint(command.output, "Press Enter to return to the menu...")
		_, _ = command.input.ReadString('\n')
	}
}

func (command *CLI) executeMenuAction(ctx context.Context, arguments []string, options Options) error {
	handled, code, err := elevate.Ensure(
		invocationArguments(arguments, options),
		requiresAdministrator(arguments, options.Mode),
	)
	if err != nil {
		return err
	}
	if handled {
		if code != 0 {
			return fmt.Errorf("elevated command exited with code %d", code)
		}
		return nil
	}
	return command.execute(ctx, arguments, options)
}

func (command *CLI) changePortableMode(enabled bool) error {
	executable, err := layout.CurrentExecutable()
	if err != nil {
		return err
	}
	if err := layout.SetPortableMarker(executable, enabled); err != nil {
		return err
	}
	mode := layout.System
	if enabled {
		mode = layout.Portable
	}
	fmt.Fprintf(command.output, "Default mode changed to %s. Restart Sempre to use it.\n", mode)
	return nil
}

func (command *CLI) menuArguments(choice string) []string {
	switch choice {
	case "1":
		return []string{"status"}
	case "2":
		return []string{"core", "list"}
	case "3":
		return []string{"core", "current"}
	case "4":
		return []string{"core", "install", "sing-box@stable"}
	case "5":
		return command.promptCoreVersion("Exact sing-box version (for example 1.13.15): ", "install")
	case "6":
		return []string{"core", "update"}
	case "7":
		return command.promptArguments("Core reference (for example sing-box@stable): ", "core", "use")
	case "8":
		return command.promptArguments("Core reference to remove: ", "core", "remove")
	case "9":
		return []string{"run"}
	case "10":
		return []string{"subscription", "status"}
	case "11":
		return command.promptArguments("Subscription HTTPS URL: ", "subscription", "set")
	case "12":
		return []string{"subscription", "clear"}
	case "13":
		return []string{"subscription", "update"}
	case "14":
		return command.promptArguments("Interval (for example 24h or off): ", "subscription", "schedule")
	case "15":
		return command.promptArguments("Configuration file path: ", "config", "import")
	case "16":
		return []string{"service", "status"}
	case "17":
		return []string{"service", "install"}
	case "18":
		return []string{"service", "deploy", "all"}
	case "19":
		return []string{"service", "deploy", "core"}
	case "20":
		return []string{"service", "deploy", "bin"}
	case "21":
		return []string{"service", "deploy", "data"}
	case "22":
		return []string{"service", "start"}
	case "23":
		return []string{"service", "stop"}
	case "24":
		return []string{"service", "restart"}
	case "25":
		return []string{"service", "uninstall"}
	case "26":
		return []string{"logs"}
	case "27":
		return []string{"logs", "--follow"}
	case "28":
		return []string{"doctor"}
	default:
		return nil
	}
}

func (command *CLI) promptCoreVersion(prompt, operation string) []string {
	arguments := command.promptArguments(prompt, "core", operation)
	if len(arguments) == 0 {
		return nil
	}
	arguments[len(arguments)-1] = "sing-box@" + strings.TrimPrefix(arguments[len(arguments)-1], "v")
	return arguments
}

func (command *CLI) promptArguments(prompt string, prefix ...string) []string {
	fmt.Fprint(command.output, prompt)
	value, err := command.input.ReadString('\n')
	if err != nil && !errors.Is(err, io.EOF) {
		return nil
	}
	value = strings.TrimSpace(value)
	if value == "" {
		return nil
	}
	return append(prefix, value)
}
