package main

import (
	"context"
	"fmt"
	"os"
	"os/signal"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "missing command")
		os.Exit(2)
	}
	switch os.Args[1] {
	case "version":
		fmt.Println("sing-box version 1.2.3")
	case "check":
		if err := checkConfig(os.Args[2:]); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(2)
		}
	case "run":
		if err := checkConfig(os.Args[2:]); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(2)
		}
		ctx, cancel := signal.NotifyContext(context.Background(), handledSignals()...)
		defer cancel()
		fmt.Println("test core started")
		<-ctx.Done()
	default:
		fmt.Fprintln(os.Stderr, "unsupported command")
		os.Exit(2)
	}
}

func checkConfig(arguments []string) error {
	for index := 0; index+1 < len(arguments); index++ {
		if arguments[index] != "-c" {
			continue
		}
		info, err := os.Stat(arguments[index+1])
		if err != nil {
			return fmt.Errorf("inspect configuration: %w", err)
		}
		if info.IsDir() {
			return fmt.Errorf("configuration is a directory")
		}
		return nil
	}
	return fmt.Errorf("configuration argument is missing")
}
