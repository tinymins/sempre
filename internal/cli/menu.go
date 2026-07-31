package cli

import (
	"context"
	"errors"
	"fmt"
	"io"
	"strings"
)

func (command *CLI) menu(ctx context.Context) int {
	for {
		fmt.Fprintln(command.output, "\nSempre")
		fmt.Fprintf(command.output, "  Mode: %s\n", command.manager.Paths().Mode)
		fmt.Fprintln(command.output, "  1. Status")
		fmt.Fprintln(command.output, "  2. List core versions")
		fmt.Fprintln(command.output, "  3. Install latest stable sing-box")
		fmt.Fprintln(command.output, "  4. Select core version")
		fmt.Fprintln(command.output, "  5. Run selected core in foreground")
		fmt.Fprintln(command.output, "  6. Set subscription URL")
		fmt.Fprintln(command.output, "  7. Update subscription")
		fmt.Fprintln(command.output, "  8. Configure subscription schedule")
		fmt.Fprintln(command.output, "  9. Import local configuration")
		fmt.Fprintln(command.output, " 10. Install or repair service")
		fmt.Fprintln(command.output, " 11. Start service")
		fmt.Fprintln(command.output, " 12. Stop service")
		fmt.Fprintln(command.output, " 13. Restart service")
		fmt.Fprintln(command.output, " 14. Show logs")
		fmt.Fprintln(command.output, " 15. Doctor")
		fmt.Fprintln(command.output, "  0. Exit")
		fmt.Fprint(command.output, "\nSelect [0-15]: ")
		line, err := command.input.ReadString('\n')
		if err != nil && !errors.Is(err, io.EOF) {
			fmt.Fprintln(command.errors, "ERROR:", err)
			return 1
		}
		choice := strings.TrimSpace(line)
		if choice == "0" || (choice == "" && errors.Is(err, io.EOF)) {
			return 0
		}
		arguments := command.menuArguments(choice)
		if len(arguments) == 0 {
			fmt.Fprintln(command.errors, "Invalid selection.")
			continue
		}
		if err := command.execute(ctx, arguments, Options{Mode: command.manager.Paths().Mode}); err != nil {
			fmt.Fprintln(command.errors, "ERROR:", err)
		}
		fmt.Fprint(command.output, "Press Enter to return to the menu...")
		_, _ = command.input.ReadString('\n')
	}
}

func (command *CLI) menuArguments(choice string) []string {
	switch choice {
	case "1":
		return []string{"status"}
	case "2":
		return []string{"core", "list"}
	case "3":
		return []string{"core", "install", "sing-box@stable"}
	case "4":
		return command.promptArguments("Core reference (for example sing-box@stable): ", "core", "use")
	case "5":
		return []string{"run"}
	case "6":
		return command.promptArguments("Subscription HTTPS URL: ", "subscription", "set")
	case "7":
		return []string{"subscription", "update"}
	case "8":
		return command.promptArguments("Interval (for example 24h or off): ", "subscription", "schedule")
	case "9":
		return command.promptArguments("Configuration file path: ", "config", "import")
	case "10":
		return []string{"service", "install"}
	case "11":
		return []string{"service", "start"}
	case "12":
		return []string{"service", "stop"}
	case "13":
		return []string{"service", "restart"}
	case "14":
		return []string{"logs"}
	case "15":
		return []string{"doctor"}
	default:
		return nil
	}
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
