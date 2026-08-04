package cli

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strconv"

	"github.com/tinymins/sempre/internal/app"
	subscriptions "github.com/tinymins/sempre/internal/subscription"
)

func (command *CLI) subscriptionCommand(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	switch arguments[0] {
	case "list":
		if len(arguments) != 1 {
			return usageError()
		}
		catalog, active, schedule, auto, err := command.manager.SubscriptionCatalog()
		if err != nil {
			return err
		}
		if options.JSON {
			return writeCLIJSON(command.output, map[string]any{"profiles": catalog.Profiles, "active_profile_id": active, "schedule": schedule, "auto_restart": auto})
		}
		for _, profile := range catalog.Profiles {
			marker := " "
			if profile.ID == active {
				marker = "*"
			}
			name := profile.Name
			if name == "" {
				name = "Default"
			}
			fmt.Fprintf(command.output, "%s %s\t%s\t%d sources\n", marker, profile.ID, name, len(profile.Sources))
		}
		return nil
	case "create":
		if len(arguments) != 2 {
			return usageError()
		}
		profile, err := command.manager.CreateSubscriptionProfile(arguments[1])
		if err != nil {
			return err
		}
		return writeCLIJSON(command.output, profile)
	case "show":
		if len(arguments) > 2 {
			return usageError()
		}
		catalog, active, _, _, err := command.manager.SubscriptionCatalog()
		if err != nil {
			return err
		}
		id := active
		if len(arguments) == 2 {
			id = arguments[1]
		}
		profile, err := subscriptions.FindProfile(&catalog, id)
		if err != nil {
			return err
		}
		return writeCLIJSON(command.output, profile)
	case "save":
		if len(arguments) != 3 {
			return usageError()
		}
		profile, err := readProfileFile(arguments[2])
		if err != nil {
			return err
		}
		change, result, err := command.manager.SaveSubscriptionProfile(ctx, arguments[1], profile)
		if err != nil {
			return err
		}
		command.printChange(change)
		if options.JSON {
			return writeCLIJSON(command.output, result)
		}
		return nil
	case "use":
		if len(arguments) != 2 {
			return usageError()
		}
		change, _, err := command.manager.UseSubscriptionProfile(ctx, arguments[1])
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	case "remove":
		if len(arguments) != 2 {
			return usageError()
		}
		change, err := command.manager.RemoveSubscriptionProfile(arguments[1])
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	case "set":
		if len(arguments) != 2 {
			return usageError()
		}
		change, err := command.manager.SetSubscription(ctx, arguments[1])
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	case "update":
		if len(arguments) > 2 {
			return usageError()
		}
		var change app.Change
		var err error
		if len(arguments) == 2 {
			change, _, err = command.manager.RefreshSubscriptionProfile(ctx, arguments[1])
		} else {
			change, err = command.manager.UpdateSubscription(ctx)
		}
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	case "schedule":
		if len(arguments) != 2 {
			return usageError()
		}
		change, err := command.manager.SetSubscriptionSchedule(arguments[1])
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	case "auto-restart":
		if len(arguments) != 2 {
			return usageError()
		}
		enabled, err := strconv.ParseBool(arguments[1])
		if err != nil {
			return fmt.Errorf("auto-restart expects true or false")
		}
		change, err := command.manager.SetSubscriptionAutoRestart(enabled)
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	case "status":
		if len(arguments) != 1 {
			return usageError()
		}
		output, err := command.manager.SubscriptionStatus()
		if err == nil {
			fmt.Fprintln(command.output, output)
		}
		return err
	case "clear":
		if len(arguments) != 1 {
			return usageError()
		}
		change, err := command.manager.ClearSubscription()
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	case "render", "debug":
		if len(arguments) < 2 || len(arguments) > 3 {
			return usageError()
		}
		format := "sing-box-v13"
		if len(arguments) == 3 {
			format = arguments[2]
		}
		result, err := command.manager.RenderSubscriptionProfile(ctx, arguments[1], format, true)
		if err != nil {
			return err
		}
		if arguments[0] == "render" && !options.JSON {
			fmt.Fprint(command.output, result.Content)
			return nil
		}
		return writeCLIJSON(command.output, result)
	case "source":
		return command.subscriptionSource(ctx, arguments[1:], options)
	default:
		return usageError()
	}
}

func (command *CLI) subscriptionSource(ctx context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	catalog, active, _, _, err := command.manager.SubscriptionCatalog()
	if err != nil {
		return err
	}
	profile, err := subscriptions.FindProfile(&catalog, active)
	if err != nil {
		return err
	}
	switch arguments[0] {
	case "add-url":
		if len(arguments) != 2 {
			return usageError()
		}
		candidate := *profile
		candidate.Sources = append(candidate.Sources, subscriptions.Source{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true, URL: arguments[1], UserAgent: subscriptions.DefaultUserAgent, FetchMode: subscriptions.FetchAuto})
		change, _, err := command.manager.SaveSubscriptionProfile(ctx, active, candidate)
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	case "add-raw":
		if len(arguments) != 2 {
			return usageError()
		}
		data, err := os.ReadFile(arguments[1])
		if err != nil {
			return err
		}
		candidate := *profile
		candidate.Sources = append(candidate.Sources, subscriptions.Source{ID: subscriptions.NewID(), Type: subscriptions.SourceRaw, Enabled: true, Content: string(data), Remark: arguments[1]})
		change, _, err := command.manager.SaveSubscriptionProfile(ctx, active, candidate)
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	case "remove":
		if len(arguments) != 2 {
			return usageError()
		}
		candidate := *profile
		found := false
		sources := candidate.Sources[:0]
		for _, source := range candidate.Sources {
			if source.ID == arguments[1] {
				found = true
				continue
			}
			sources = append(sources, source)
		}
		if !found {
			return fmt.Errorf("source %q was not found", arguments[1])
		}
		candidate.Sources = sources
		change, _, err := command.manager.SaveSubscriptionProfile(ctx, active, candidate)
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	case "test":
		if len(arguments) != 2 {
			return usageError()
		}
		source := subscriptions.Source{ID: subscriptions.NewID(), Type: subscriptions.SourceURL, Enabled: true, URL: arguments[1], UserAgent: subscriptions.DefaultUserAgent, FetchMode: subscriptions.FetchAuto}
		if data, readErr := os.ReadFile(arguments[1]); readErr == nil {
			source.Type = subscriptions.SourceRaw
			source.URL = ""
			source.Content = string(data)
		}
		result, err := command.manager.TestSubscriptionSource(ctx, source)
		if err != nil {
			return err
		}
		if options.JSON {
			return writeCLIJSON(command.output, result)
		}
		fmt.Fprintf(command.output, "Format: %s\nNodes: %d\nBytes: %d\n", result.Parse.Format, len(result.Parse.Nodes), result.Bytes)
		for _, diagnostic := range result.Parse.Diagnostics {
			fmt.Fprintln(command.output, diagnostic)
		}
		return nil
	default:
		return usageError()
	}
}

func (command *CLI) customNode(_ context.Context, arguments []string, options Options) error {
	if len(arguments) == 0 {
		return usageError()
	}
	switch arguments[0] {
	case "list":
		if len(arguments) != 1 {
			return usageError()
		}
		nodes, err := command.manager.CustomNodes()
		if err != nil {
			return err
		}
		if options.JSON {
			return writeCLIJSON(command.output, nodes)
		}
		for _, node := range nodes {
			fmt.Fprintf(command.output, "%s\t%s\n", node.ID, node.Name)
		}
		return nil
	case "add", "update":
		expected := 2
		if arguments[0] == "update" {
			expected = 3
		}
		if len(arguments) != expected {
			return usageError()
		}
		path := arguments[len(arguments)-1]
		node, err := readCustomNodeFile(path)
		if err != nil {
			return err
		}
		if arguments[0] == "update" {
			node.ID = arguments[1]
		} else {
			node.ID = ""
		}
		saved, err := command.manager.SaveCustomNode(node)
		if err != nil {
			return err
		}
		return writeCLIJSON(command.output, saved)
	case "remove":
		if len(arguments) != 2 {
			return usageError()
		}
		change, err := command.manager.RemoveCustomNode(arguments[1])
		if err != nil {
			return err
		}
		command.printChange(change)
		return nil
	default:
		return usageError()
	}
}

func readProfileFile(path string) (subscriptions.Profile, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return subscriptions.Profile{}, err
	}
	var profile subscriptions.Profile
	if err := json.Unmarshal(data, &profile); err != nil {
		return subscriptions.Profile{}, fmt.Errorf("decode profile: %w", err)
	}
	return profile, nil
}
func readCustomNodeFile(path string) (subscriptions.CustomNode, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return subscriptions.CustomNode{}, err
	}
	var node subscriptions.CustomNode
	if err := json.Unmarshal(data, &node); err == nil && len(node.Proxy) > 0 {
		return node, nil
	}
	var proxy map[string]any
	if err := json.Unmarshal(data, &proxy); err != nil {
		return subscriptions.CustomNode{}, fmt.Errorf("decode custom node: %w", err)
	}
	name, _ := proxy["name"].(string)
	return subscriptions.CustomNode{Name: name, Proxy: proxy}, nil
}
